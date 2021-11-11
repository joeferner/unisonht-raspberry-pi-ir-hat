use crate::app_state::AppState;
use crate::config::UnisonConfig;
use crate::config::UnisonConfigDevice;
use crate::config_env::ConfigEnv;
use crate::hat::init_hat;
use crate::message::StatusMessageDevice;
use crate::mqtt::init_mqtt;
use crate::mqtt::send_status_message;
use log::{error, info};
use raspberry_pi_ir_hat::Hat;
use simple_logger::SimpleLogger;
use std::fs;
use std::process;
use std::sync::Arc;
use std::sync::Mutex;
use std::thread;

mod action;
mod app_state;
mod config;
mod config_env;
mod hat;
mod message;
mod mqtt;

fn init(config_env: &ConfigEnv) -> Result<(Arc<Mutex<AppState>>, Vec<UnisonConfigDevice>), String> {
    info!("initializing");
    let config_text = fs::read_to_string(&config_env.config_filename).map_err(|err| {
        format!(
            "failed to read file: {}: {}",
            config_env.config_filename, err
        )
    })?;
    let unison_config = UnisonConfig::from_str(&config_text)?;

    let app_state = Arc::new(Mutex::new(AppState {
        hat: Option::None,
        mqtt_client: Option::None,
        topic_prefix: config_env.topic_prefix.clone(),
    }));

    let hat =
        init_hat(&app_state, &config_text).map_err(|err| format!("init hat error: {}", err))?;
    let mqtt_client =
        init_mqtt(app_state.clone()).map_err(|err| format!("init mqtt error: {}", err))?;
    match app_state.lock() {
        Result::Err(err) => {
            // need to exit here since there is no recovering from a broken lock
            error!("failed to lock {}", err);
            process::exit(1);
        }
        Result::Ok(mut app_state) => {
            app_state.hat = Option::Some(Arc::new(Mutex::new(hat)));
            app_state.mqtt_client = Option::Some(Arc::new(Mutex::new(mqtt_client)));
        }
    }
    return Result::Ok((app_state, unison_config.devices));
}

fn main() -> Result<(), String> {
    SimpleLogger::new()
        .init()
        .map_err(|err| format!("{}", err))?;
    info!("starting");

    let config_env = ConfigEnv::get()?;
    let status_interval = config_env.status_interval;
    match init(&config_env) {
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
                thread::sleep(status_interval);
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
