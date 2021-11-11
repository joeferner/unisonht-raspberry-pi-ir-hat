use crate::message::StatusMessage;
use crate::message::TransmitMessage;
use crate::AppState;
use crate::ConfigEnv;
use crate::StatusMessageDevice;
use log::{debug, error};
use raspberry_pi_ir_hat::CurrentChannel;
use raspberry_pi_ir_hat::HatError;
use rumqttc;
use rumqttc::Event;
use rumqttc::Packet;
use rumqttc::Publish;
use rumqttc::{MqttOptions, QoS};
use std::collections::HashMap;
use std::process;
use std::sync::Arc;
use std::sync::Mutex;
use std::thread;

pub fn init_mqtt(app_state: &Arc<Mutex<AppState>>) -> Result<rumqttc::Client, String> {
    let config_env = ConfigEnv::get()?;
    let create_opts = MqttOptions::new(
        config_env.mqtt_client_id,
        config_env.mqtt_uri,
        config_env.mqtt_port,
    );
    let (mut client, mut connection) = rumqttc::Client::new(create_opts, 100);

    let topic_pattern = get_topic_prefix(&app_state) + "#";
    client
        .subscribe(topic_pattern.clone(), QoS::AtMostOnce)
        .map_err(|err| format!("failed to subscribe to {}: {}", topic_pattern, err))?;

    let thread_app_state = app_state.clone();
    thread::spawn(move || mqtt_poll(&thread_app_state, &mut connection));

    return Result::Ok(client);
}

fn mqtt_poll(app_state: &Arc<Mutex<AppState>>, connection: &mut rumqttc::Connection) {
    for (_i, notification) in connection.iter().enumerate() {
        match notification {
            Result::Err(err) => {
                error!("mqtt connection error {}", err);
            }
            Result::Ok(event) => match event {
                Event::Incoming(incoming_event) => match incoming_event {
                    Packet::Publish(publish_packet) => {
                        debug!("publish packet: {:?}", publish_packet);
                        handle_mqtt_message(app_state, &publish_packet)
                            .unwrap_or_else(|err| error!("handle mqtt message error: {}", err));
                    }
                    _ => {
                        debug!("incoming event: {:?}", incoming_event);
                    }
                },
                Event::Outgoing(outgoing_event) => {
                    debug!("outgoing event: {:?}", outgoing_event);
                }
            },
        }
    }
}

fn handle_mqtt_message(
    app_state: &Arc<Mutex<AppState>>,
    publish_packet: &Publish,
) -> Result<(), String> {
    let topic = &publish_packet.topic;
    let payload = String::from_utf8_lossy(&publish_packet.payload);

    let prefix = get_topic_prefix(app_state);
    if !topic.starts_with(&prefix) {
        return Result::Err(format!("topic must start with: {}", prefix));
    }
    let topic_part = &topic[prefix.len()..];
    return match topic_part {
        "tx" => handle_mqtt_message_transmit(app_state, &payload),
        "request-status" => handle_mqtt_message_request_status(app_state, &payload),
        _ => Result::Err(format!("unhandled topic for incoming message: {}", topic)),
    };
}

fn handle_mqtt_message_request_status(
    app_state: &Arc<Mutex<AppState>>,
    _payload: &str,
) -> Result<(), String> {
    debug!("handling request-status request");
    return send_status_message(&app_state);
}

fn handle_mqtt_message_transmit(
    app_state: &Arc<Mutex<AppState>>,
    payload: &str,
) -> Result<(), String> {
    let message: TransmitMessage = serde_json::from_str(payload)
        .map_err(|err| format!("invalid transmit message: {}", err))?;
    debug!(
        "handling transmit request {}:{}",
        message.remote_name, message.button_name
    );
    match app_state.lock() {
        Result::Err(err) => {
            // cannot recover from a bad lock
            error!("failed to lock {}", err);
            process::exit(1);
        }
        Result::Ok(mut app_state) => match app_state
            .hat
            .as_mut()
            .expect("hat not set")
            .transmit(&message.remote_name, &message.button_name)
        {
            Result::Err(err) => match err {
                HatError::InvalidButton {
                    remote_name,
                    button_name,
                } => {
                    return Result::Err(format!(
                        "button not found {}:{}",
                        remote_name, button_name
                    ));
                }
                HatError::Timeout(err) => {
                    return Result::Err(format!("timeout {}", err));
                }
                _ => {
                    return Result::Err(format!("transmit error {}", err));
                }
            },
            Result::Ok(_) => {
                return Result::Ok(());
            }
        },
    };
}

pub fn send_status_message(app_state: &Arc<Mutex<AppState>>) -> Result<(), String> {
    match app_state.lock() {
        Result::Err(err) => {
            // need to exit here since there is no recovering from a broken lock
            error!("failed to lock {}", err);
            process::exit(1);
        }
        Result::Ok(mut app_state) => {
            let mut devices: HashMap<String, StatusMessageDevice> = HashMap::new();
            for ch in [CurrentChannel::Channel0, CurrentChannel::Channel1] {
                match app_state.hat.as_mut().expect("hat not set").get_current(ch) {
                    Result::Err(err) => match err {
                        HatError::Timeout(err) => {
                            return Result::Err(format!("timeout: {}", err));
                        }
                        _ => {
                            return Result::Err(format!("transmit error {}", err));
                        }
                    },
                    Result::Ok(resp) => {
                        let device_config = match ch {
                            CurrentChannel::Channel0 => app_state.unison_config.devices.get(0),
                            CurrentChannel::Channel1 => app_state.unison_config.devices.get(1),
                        };
                        if let Option::Some(device_config) = device_config {
                            devices.insert(
                                device_config.name.to_string(),
                                StatusMessageDevice {
                                    milliamps: resp.milliamps,
                                    is_on: resp.milliamps > device_config.on_threshold_milliamps,
                                },
                            );
                        }
                    }
                }
            }
            let status = StatusMessage { devices };
            let status_string: String = serde_json::to_string(&status)
                .map_err(|err| format!("could not convert status to json: {}", err))?;
            let status_topic = app_state.topic_prefix.clone() + "status";
            debug!("sending status");
            app_state
                .mqtt_client
                .as_mut()
                .expect("mqtt_client not set")
                .publish(status_topic, QoS::AtLeastOnce, false, status_string)
                .map_err(|err| format!("failed to publish mqtt message: {}", err))?;
            return Result::Ok(());
        }
    }
}

fn get_topic_prefix(app_state: &Arc<Mutex<AppState>>) -> String {
    match app_state.lock() {
        Result::Err(err) => {
            // need to exit here since there is no recovering from a broken lock
            error!("failed to lock {}", err);
            process::exit(1);
        }
        Result::Ok(app_state) => return app_state.topic_prefix.clone(),
    };
}
