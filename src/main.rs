use log::{debug, error, info, warn};
use paho_mqtt;
use raspberry_pi_ir_hat::Config;
use raspberry_pi_ir_hat::CurrentChannel;
use raspberry_pi_ir_hat::{ButtonPress, Hat, HatError, HatMessage};
use serde::{Deserialize, Serialize};
use simple_logger::SimpleLogger;
use std::collections::HashMap;
use std::env;
use std::fs;
use std::process;
use std::sync::Arc;
use std::sync::Mutex;
use std::thread;
use std::time::Duration;

#[derive(Clone)]
pub struct AppState {
    hat: Option<Arc<Mutex<Hat>>>,
    mqtt_client: Option<Arc<Mutex<paho_mqtt::AsyncClient>>>,
    topic_prefix: String,
}

#[derive(Debug, PartialEq, Serialize, Deserialize)]
pub struct UnisonConfig {
    remotes: HashMap<String, UnisonConfigRemote>,
}

#[derive(Debug, PartialEq, Serialize, Deserialize)]
pub struct UnisonConfigRemote {
    buttons: HashMap<String, UnisonConfigButton>,
}

#[derive(Debug, PartialEq, Serialize, Deserialize)]
pub struct UnisonConfigButton {
    action: Option<UnisonConfigAction>,
}

#[derive(Debug, PartialEq, Serialize, Deserialize)]
pub struct UnisonConfigAction {
    #[serde(rename = "type")]
    action_type: String,

    // type: http
    url: Option<String>,

    // type: http
    method: Option<String>,

    // type: mqtt
    topic: Option<String>,

    // type: mqtt
    payload: Option<String>,
}

#[derive(Serialize, Deserialize)]
pub struct StatusMessage {
    pub devices: HashMap<String, StatusMessageDevice>,
}

#[derive(Serialize, Deserialize)]
pub struct StatusMessageDevice {
    pub milliamps: u32,
}

#[derive(Serialize, Deserialize)]
pub struct TransmitMessage {
    remote_name: String,
    button_name: String,
}

fn handle_mqtt_message(
    client: &paho_mqtt::AsyncClient,
    topic: &str,
    payload: &str,
) -> Result<(), String> {
    let prefix = get_topic_prefix();
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
    match get_app_state(client).hat.as_ref().unwrap().lock() {
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

fn handle_button_press(
    app_state: &AppState,
    config: &UnisonConfig,
    button_press: &ButtonPress,
) -> Result<(), String> {
    let remote = config.remotes.get(&button_press.remote_name);
    if remote.is_none() {
        return Result::Ok(());
    }
    let remote = remote.unwrap();
    let button = remote.buttons.get(&button_press.button_name);
    if button.is_none() {
        return Result::Ok(());
    }
    let button = button.unwrap();
    if button.action.is_none() {
        return Result::Ok(());
    }
    let action = button.action.as_ref().unwrap();
    let action_type: &str = &action.action_type;
    match action_type {
        "http" => {
            return do_http_action(&action).map_err(|err| {
                format!(
                    "error executing action {}:{}: {}",
                    button_press.remote_name, button_press.button_name, err
                )
            });
        }
        "mqtt" => match app_state.mqtt_client.as_ref().unwrap().lock() {
            Result::Err(err) => {
                // need to exit here since there is no recovering from a broken lock
                error!("failed to lock {}", err);
                process::exit(1);
            }
            Result::Ok(mqtt_client) => {
                return do_mqtt_action(&mqtt_client, &action).map_err(|err| {
                    format!(
                        "error executing action {}:{}: {}",
                        button_press.remote_name, button_press.button_name, err
                    )
                });
            }
        },
        _ => {
            return Result::Err(format!(
                "button {}:{} has invalid type: {}",
                button_press.remote_name, button_press.button_name, action_type
            ));
        }
    }
}

fn do_http_action(action: &UnisonConfigAction) -> Result<(), String> {
    let default_method = "post".to_string();
    let url = action
        .url
        .as_ref()
        .ok_or_else(|| format!("'http' actions require a url"))?;
    let method = action
        .method
        .as_ref()
        .unwrap_or(&default_method)
        .to_lowercase();
    info!("calling {} {}", method, url);
    let response = match method.as_ref() {
        "get" => ureq::get(&url).call(),
        "post" => ureq::post(&url).call(),
        _ => return Result::Err(format!("unexpected http method: {}", method)),
    };
    return match response {
        Result::Ok(_) => Result::Ok(()),
        Result::Err(err) => Result::Err(format!("failed to call: {} {}: {}", method, url, err)),
    };
}

fn do_mqtt_action(
    client: &paho_mqtt::AsyncClient,
    action: &UnisonConfigAction,
) -> Result<(), String> {
    let topic = action
        .topic
        .as_ref()
        .ok_or_else(|| format!("'mqtt' actions require a topic"))?;
    let payload = action
        .payload
        .as_ref()
        .ok_or_else(|| format!("'mqtt' actions require a payload"))?
        .clone();

    let message = paho_mqtt::Message::new(topic, payload, paho_mqtt::QOS_1);
    client
        .publish(message)
        .wait()
        .map_err(|err| format!("mqtt publish failed: {}", err))?;
    return Result::Ok(());
}

fn init_hat(app_state: &AppState) -> std::result::Result<Hat, String> {
    SimpleLogger::new()
        .init()
        .map_err(|err| format!("{}", err))?;

    let config_filename: &str = &env::var("HAT_CONFIG").unwrap_or("./config.yaml".to_string());
    let config_text = fs::read_to_string(config_filename)
        .map_err(|err| format!("failed to read file: {}: {}", config_filename, err))?;
    let config =
        Config::from_str(&config_text).map_err(|err| format!("failed to read config {}", err))?;
    let port = env::var("HAT_PORT").unwrap_or("/dev/serial0".to_string());
    let tolerance_string = env::var("HAT_TOLERANCE").unwrap_or("0.15".to_string());
    let tolerance = tolerance_string
        .parse::<f32>()
        .map_err(|err| format!("invalid tolerance: {} ({})", tolerance_string, err))?;
    let unison_config: UnisonConfig = serde_yaml::from_str(&config_text).map_err(|err| {
        format!(
            "could not read config: contained invalid yaml values: {}",
            err
        )
    })?;
    let hat_app_state = app_state.clone();
    let mut hat = Hat::new(
        config,
        &port,
        tolerance,
        Box::new(move |message| {
            match message {
                HatMessage::ButtonPress(button_press) => {
                    if let Result::Err(err) =
                        handle_button_press(&hat_app_state, &unison_config, &button_press)
                    {
                        error!("{}", err);
                    }
                }
                HatMessage::Error(err) => {
                    error!("{:#?}", err);
                }
            };
        }),
    );
    hat.open()
        .map_err(|err| format!("failed to open hat {}", err))?;

    return Result::Ok(hat);
}

fn mqtt_on_connect_success(client: &paho_mqtt::AsyncClient, _msg_id: u16) {
    info!("mqtt connected");
    let topic_pattern = get_app_state(client).topic_prefix.clone() + "#";
    client.subscribe(topic_pattern, paho_mqtt::QOS_1);
}

fn mqtt_on_connect_failure(client: &paho_mqtt::AsyncClient, _msg_id: u16, rc: i32) {
    error!("mqtt connection failure {}", rc);
    thread::sleep(Duration::from_millis(2500));
    client.reconnect_with_callbacks(mqtt_on_connect_success, mqtt_on_connect_failure);
}

fn init_mqtt(app_state: &AppState) -> Result<paho_mqtt::AsyncClient, paho_mqtt::Error> {
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

fn get_topic_prefix() -> String {
    let mut topic_prefix = env::var("MQTT_TOPIC_PREFIX").unwrap_or("ir/".to_string());
    if !topic_prefix.ends_with("/") {
        topic_prefix = topic_prefix + "/";
    }
    return topic_prefix;
}

fn get_app_state(client: &paho_mqtt::AsyncClient) -> &AppState {
    return client
        .user_data()
        .unwrap()
        .downcast_ref::<AppState>()
        .unwrap();
}

fn get_status_topic(app_state: &AppState) -> String {
    match app_state.mqtt_client.as_ref().unwrap().lock() {
        Result::Err(err) => {
            // cannot recover from a bad lock
            error!("failed to lock {}", err);
            process::exit(1);
        }
        Result::Ok(client) => {
            return get_app_state(&client).topic_prefix.clone() + "status";
        }
    }
}

fn init() -> Result<AppState, String> {
    let mut app_state = AppState {
        hat: Option::None,
        mqtt_client: Option::None,
        topic_prefix: get_topic_prefix(),
    };
    let hat = init_hat(&app_state).map_err(|err| format!("init hat error: {}", err))?;
    app_state.hat = Option::Some(Arc::new(Mutex::new(hat)));
    let mqtt_client = init_mqtt(&app_state).map_err(|err| format!("init mqtt error: {}", err))?;
    app_state.mqtt_client = Option::Some(Arc::new(Mutex::new(mqtt_client)));
    return Result::Ok(app_state);
}

fn main() -> Result<(), String> {
    match init() {
        Result::Err(err) => {
            error!("init failed: {}", err);
            process::exit(1);
        }
        Result::Ok(app_state) => {
            info!("started");
            let status_topic = get_status_topic(&app_state);
            let mqtt_client_mutex = app_state.mqtt_client.unwrap();
            loop {
                thread::sleep(Duration::from_secs(60));
                match mqtt_client_mutex.lock() {
                    Result::Err(err) => {
                        // cannot recover from a bad lock
                        error!("failed to lock {}", err);
                        process::exit(1);
                    }
                    Result::Ok(client) => {
                        send_status_message(&client, &status_topic).unwrap_or_else(|err| {
                            error!("failed to send status heartbeat: {}", err)
                        });
                    }
                }
            }
        }
    }
}

fn send_status_message(client: &paho_mqtt::AsyncClient, topic: &str) -> Result<(), String> {
    match get_app_state(client).hat.as_ref().unwrap().lock() {
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
                        let device_name = match ch {
                            CurrentChannel::Channel0 => "device0",
                            CurrentChannel::Channel1 => "device1",
                        };
                        devices.insert(
                            device_name.to_string(),
                            StatusMessageDevice {
                                milliamps: resp.milliamps,
                            },
                        );
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
