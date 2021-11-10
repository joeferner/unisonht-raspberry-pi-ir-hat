use crate::message::StatusMessage;
use crate::message::TransmitMessage;
use crate::AppState;
use crate::Hat;
use crate::StatusMessageDevice;
use crate::UnisonConfigDevice;
use log::{debug, error, info, warn};
use paho_mqtt;
use raspberry_pi_ir_hat::CurrentChannel;
use raspberry_pi_ir_hat::HatError;
use std::collections::HashMap;
use std::env;
use std::process;
use std::sync::Arc;
use std::sync::Mutex;
use std::thread;
use std::time::Duration;

fn mqtt_on_connect_success(client: &paho_mqtt::AsyncClient, _msg_id: u16) {
    info!("mqtt connected");
    let topic_pattern = get_topic_prefix(client) + "#";
    client.subscribe(topic_pattern, paho_mqtt::QOS_1);
}

fn mqtt_on_connect_failure(client: &paho_mqtt::AsyncClient, _msg_id: u16, rc: i32) {
    error!("mqtt connection failure {}", rc);
    thread::sleep(Duration::from_millis(2500));
    client.reconnect_with_callbacks(mqtt_on_connect_success, mqtt_on_connect_failure);
}

pub fn init_mqtt(
    app_state: Arc<Mutex<AppState>>,
) -> Result<paho_mqtt::AsyncClient, paho_mqtt::Error> {
    let mqtt_uri = env::var("MQTT_URI").unwrap_or("tcp://localhost:1883".to_string());
    let mqtt_client_id = env::var("MQTT_CLIENT_ID").unwrap_or("raspirhat".to_string());
    let create_opts = paho_mqtt::CreateOptionsBuilder::new()
        .server_uri(mqtt_uri)
        .client_id(mqtt_client_id)
        .user_data(Box::new(app_state))
        .finalize();
    let mut mqtt_client = paho_mqtt::AsyncClient::new(create_opts)?;

    mqtt_client.set_connection_lost_callback(|client| {
        warn!("mqtt connection lost. reconnecting...");
        thread::sleep(Duration::from_millis(2500));
        client.reconnect_with_callbacks(mqtt_on_connect_success, mqtt_on_connect_failure);
    });

    mqtt_client.set_message_callback(|client, message| {
        if let Option::Some(message) = message {
            let topic = message.topic();
            let payload_str = message.payload_str();
            handle_mqtt_message(client, topic, &payload_str).unwrap_or_else(|err| {
                warn!("{}", err);
                debug!("payload: {}", payload_str);
            });
        }
    });

    mqtt_client.set_disconnected_callback(|client, _props, rc| {
        warn!("mqtt disconnected. reconnecting... (rc: {})", rc);
        thread::sleep(Duration::from_millis(2500));
        client.reconnect_with_callbacks(mqtt_on_connect_success, mqtt_on_connect_failure);
    });

    let connect_opts = paho_mqtt::ConnectOptionsBuilder::new()
        .keep_alive_interval(Duration::from_secs(120))
        .mqtt_version(paho_mqtt::MQTT_VERSION_3_1_1)
        .clean_session(true)
        .finalize();
    mqtt_client.connect_with_callbacks(
        connect_opts,
        mqtt_on_connect_success,
        mqtt_on_connect_failure,
    );

    return Result::Ok(mqtt_client);
}

fn handle_mqtt_message(
    client: &paho_mqtt::AsyncClient,
    topic: &str,
    payload: &str,
) -> Result<(), String> {
    let prefix = get_topic_prefix(client);
    if !topic.starts_with(&prefix) {
        return Result::Err(format!("topic must start with: {}", prefix));
    }
    let topic_part = &topic[prefix.len()..];
    return match topic_part {
        "tx" => handle_mqtt_message_transmit(client, payload),
        _ => Result::Err(format!("unhandled topic for incoming message: {}", topic)),
    };
}

fn handle_mqtt_message_transmit(
    client: &paho_mqtt::AsyncClient,
    payload: &str,
) -> Result<(), String> {
    let message: TransmitMessage = serde_json::from_str(payload)
        .map_err(|err| format!("invalid transmit message: {}", err))?;
    match get_hat(client).lock() {
        Result::Err(err) => {
            // cannot recover from a bad lock
            error!("failed to lock {}", err);
            process::exit(1);
        }
        Result::Ok(mut hat) => match hat.transmit(&message.remote_name, &message.button_name) {
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

pub fn send_status_message(
    client: &paho_mqtt::AsyncClient,
    topic: &str,
    device_configs: &Vec<UnisonConfigDevice>,
) -> Result<(), String> {
    match get_hat(client).lock() {
        Result::Err(err) => {
            // need to exit here since there is no recovering from a broken lock
            error!("failed to lock {}", err);
            process::exit(1);
        }
        Result::Ok(mut hat) => {
            let mut devices: HashMap<String, StatusMessageDevice> = HashMap::new();
            for ch in [CurrentChannel::Channel0, CurrentChannel::Channel1] {
                match hat.get_current(ch) {
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
                            CurrentChannel::Channel0 => device_configs.get(0),
                            CurrentChannel::Channel1 => device_configs.get(1),
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
            let msg = paho_mqtt::Message::new(topic, status_string, paho_mqtt::QOS_1);
            client
                .publish(msg)
                .wait()
                .map_err(|err| format!("publish error: {}", err))?;
            return Result::Ok(());
        }
    }
}

fn get_app_state(client: &paho_mqtt::AsyncClient) -> &Arc<Mutex<AppState>> {
    return client
        .user_data()
        .unwrap()
        .downcast_ref::<Arc<Mutex<AppState>>>()
        .unwrap();
}

fn get_topic_prefix(client: &paho_mqtt::AsyncClient) -> String {
    match get_app_state(client).lock() {
        Result::Err(err) => {
            // need to exit here since there is no recovering from a broken lock
            error!("failed to lock {}", err);
            process::exit(1);
        }
        Result::Ok(app_state) => return app_state.topic_prefix.clone(),
    };
}

fn get_hat(client: &paho_mqtt::AsyncClient) -> Arc<Mutex<Hat>> {
    match get_app_state(client).lock() {
        Result::Err(err) => {
            // need to exit here since there is no recovering from a broken lock
            error!("failed to lock {}", err);
            process::exit(1);
        }
        Result::Ok(app_state) => return app_state.hat.as_ref().unwrap().clone(),
    };
}
