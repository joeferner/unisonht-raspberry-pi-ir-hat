use crate::app_state::AppState;
use crate::config::UnisonConfig;
use crate::config::UnisonConfigDevice;
use crate::hat::init_hat;
use crate::message::StatusMessageDevice;
use crate::mqtt::init_mqtt;
use crate::mqtt::send_status_message;
use log::{error, info};
use raspberry_pi_ir_hat::Hat;
use std::env;
use std::fs;
use std::process;
use std::sync::Arc;
use std::sync::Mutex;
use std::thread;
use std::time::Duration;

mod action;
mod app_state;
mod config;
mod hat;
mod message;
mod mqtt;

fn init() -> Result<(Arc<Mutex<AppState>>, Vec<UnisonConfigDevice>), String> {
    fn get_topic_prefix() -> String {
        let mut topic_prefix = env::var("MQTT_TOPIC_PREFIX").unwrap_or("ir/".to_string());
        if !topic_prefix.ends_with("/") {
            topic_prefix = topic_prefix + "/";
        }
        return topic_prefix;
    }

    let config_filename: &str = &env::var("HAT_CONFIG").unwrap_or("./config.yaml".to_string());
    let config_text = fs::read_to_string(config_filename)
        .map_err(|err| format!("failed to read file: {}: {}", config_filename, err))?;
    let unison_config = UnisonConfig::from_str(&config_text)?;

    let mut app_state = AppState {
        hat: Option::None,
        mqtt_client: Option::None,
        topic_prefix: get_topic_prefix(),
    };

    let hat =
        init_hat(&app_state, &config_text).map_err(|err| format!("init hat error: {}", err))?;
    app_state.hat = Option::Some(Arc::new(Mutex::new(hat)));
    let app_state = Arc::new(Mutex::new(app_state));
    let mqtt_client =
        init_mqtt(app_state.clone()).map_err(|err| format!("init mqtt error: {}", err))?;
    match app_state.lock() {
        Result::Err(err) => {
            // need to exit here since there is no recovering from a broken lock
            error!("failed to lock {}", err);
            process::exit(1);
        }
        Result::Ok(mut app_state) => {
            app_state.mqtt_client = Option::Some(Arc::new(Mutex::new(mqtt_client)));
        }
    }
    return Result::Ok((app_state, unison_config.devices));
}

fn main() -> Result<(), String> {
    let status_interval_str = env::var("STATUS_INTERVAL").unwrap_or("60".to_string());
    let status_interval = status_interval_str
        .parse::<u64>()
        .map_err(|err| format!("invalid status interval: {} ({})", status_interval_str, err))?;
    match init() {
        Result::Err(err) => {
            error!("init failed: {}", err);
            process::exit(1);
        }
        Result::Ok((app_state, device_configs)) => {
            info!("started");
            let (status_topic, mqtt_client_mutex) = match app_state.lock() {
                Result::Err(err) => {
                    // need to exit here since there is no recovering from a broken lock
                    error!("failed to lock {}", err);
                    process::exit(1);
                }
                Result::Ok(app_state) => {
                    let status_topic = app_state.topic_prefix.clone() + "status";
                    let mqtt_client_mutex = app_state.mqtt_client.as_ref().unwrap().clone();
                    (status_topic, mqtt_client_mutex)
                }
            };
            loop {
                thread::sleep(Duration::from_secs(status_interval));
                match mqtt_client_mutex.lock() {
                    Result::Err(err) => {
                        // cannot recover from a bad lock
                        error!("failed to lock {}", err);
                        process::exit(1);
                    }
                    Result::Ok(client) => {
                        send_status_message(&client, &status_topic, &device_configs)
                            .unwrap_or_else(|err| {
                                error!("failed to send status heartbeat: {}", err)
                            });
                    }
                }
            }
        }
    }
}
