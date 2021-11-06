use futures;
use futures::executor::block_on;
use log::{error, info, warn};
use paho_mqtt;
use raspberry_pi_ir_hat::Config;
use raspberry_pi_ir_hat::CurrentChannel;
use raspberry_pi_ir_hat::{ButtonPress, Hat, HatError, HatMessage};
use serde::{Deserialize, Serialize};
use simple_logger::SimpleLogger;
use std::collections::HashMap;
use std::env;
use std::process;
use std::sync::Arc;
use std::sync::Mutex;
use std::thread;
use std::time::Duration;

#[derive(Clone)]
pub struct AppState {
    hat: Arc<Mutex<Hat>>,
}

#[derive(Serialize, Deserialize)]
pub struct GetStatusResponse {
    pub devices: HashMap<String, GetStatusResponseDevice>,
}

#[derive(Serialize, Deserialize)]
pub struct GetStatusResponseDevice {
    pub milliamps: u32,
}

// #[get("/api/v1/status")]
// async fn get_status(state: web::Data<AppState>) -> impl Responder {
//     match state.hat.lock() {
//         Result::Err(err) => {
//             error!("failed to lock {}", err);
//             process::exit(1);
//         }
//         Result::Ok(mut hat) => {
//             let mut devices: HashMap<String, GetStatusResponseDevice> = HashMap::new();
//             for ch in [CurrentChannel::Channel0, CurrentChannel::Channel1] {
//                 match hat.get_current(ch) {
//                     Result::Err(err) => match err {
//                         HatError::Timeout(err) => {
//                             error!("timeout {}", err);
//                             return HttpResponse::RequestTimeout().finish();
//                         }
//                         _ => {
//                             error!("transmit error {}", err);
//                             return HttpResponse::InternalServerError().finish();
//                         }
//                     },
//                     Result::Ok(resp) => {
//                         let device_name = match ch {
//                             CurrentChannel::Channel0 => "device0",
//                             CurrentChannel::Channel1 => "device1",
//                         };
//                         devices.insert(
//                             device_name.to_string(),
//                             GetStatusResponseDevice {
//                                 milliamps: resp.milliamps,
//                             },
//                         );
//                     }
//                 }
//             }
//             return HttpResponse::Ok().json(GetStatusResponse { devices });
//         }
//     };
// }

// #[post("/api/v1/transmit/<remote_name>/<button_name>")]
// async fn transmit(
//     state: web::Data<AppState>,
//     web::Path((remote_name, button_name)): web::Path<(String, String)>,
// ) -> impl Responder {
//     match state.hat.lock() {
//         Result::Err(err) => {
//             error!("failed to lock {}", err);
//             process::exit(1);
//         }
//         Result::Ok(mut hat) => match hat.transmit(&remote_name, &button_name) {
//             Result::Err(err) => match err {
//                 HatError::InvalidButton {
//                     remote_name,
//                     button_name,
//                 } => {
//                     error!("button not found {}:{}", remote_name, button_name);
//                     return HttpResponse::NotFound().finish();
//                 }
//                 HatError::Timeout(err) => {
//                     error!("timeout {}", err);
//                     return HttpResponse::RequestTimeout().finish();
//                 }
//                 _ => {
//                     error!("transmit error {}", err);
//                     return HttpResponse::InternalServerError().finish();
//                 }
//             },
//             Result::Ok(_) => {
//                 return HttpResponse::Ok().json({});
//             }
//         },
//     };
// }

// fn handle_button_press(config: &Config, button_press: &ButtonPress) -> Result<(), String> {
//     let button = config
//         .get_button(&button_press.remote_name, &button_press.button_name)
//         .ok_or_else(|| {
//             format!(
//                 "could not find button {}:{}",
//                 button_press.remote_name, button_press.button_name
//             )
//         })?;
//     match button.get_json().get("action") {
//         Option::None => return Result::Ok(()),
//         Option::Some(action_value) => {
//             let action = action_value.as_object().ok_or_else(|| {
//                 format!(
//                     "button {}:{} action should be an object",
//                     button_press.remote_name, button_press.button_name
//                 )
//             })?;
//             let action_type = action
//                 .get("type")
//                 .ok_or_else(|| {
//                     format!(
//                         "button {}:{} action should have 'type'",
//                         button_press.remote_name, button_press.button_name
//                     )
//                 })?
//                 .as_str()
//                 .ok_or_else(|| {
//                     format!(
//                         "button {}:{} action should have string 'type'",
//                         button_press.remote_name, button_press.button_name
//                     )
//                 })?;
//             match action_type {
//                 "http" => {
//                     return do_http_action(&action).map_err(|err| {
//                         format!(
//                             "error executing action {}:{}: {}",
//                             button_press.remote_name, button_press.button_name, err
//                         )
//                     });
//                 }
//                 _ => {
//                     return Result::Err(format!(
//                         "button {}:{} has invalid type: {}",
//                         button_press.remote_name, button_press.button_name, action_type
//                     ));
//                 }
//             }
//         }
//     }
// }

// fn do_http_action(action: &serde_json::Map<String, serde_json::Value>) -> Result<(), String> {
//     let url = action
//         .get("url")
//         .ok_or_else(|| format!("'http' actions should have a url"))?
//         .as_str()
//         .ok_or_else(|| format!("'http' actions should have a string url"))?;
//     let method = match action.get("method") {
//         Option::None => Result::Ok("post".to_string()),
//         Option::Some(method_value) => match method_value.as_str() {
//             Option::None => Result::Err(format!("'http' actions should have a string url")),
//             Option::Some(m) => Result::Ok(m.to_lowercase()),
//         },
//     }?;
//     info!("calling {} {}", method, url);
//     let response = match method.as_ref() {
//         "get" => ureq::get(url).call(),
//         "post" => ureq::post(url).call(),
//         _ => return Result::Err(format!("unexpected http method: {}", method)),
//     };
//     return match response {
//         Result::Ok(_) => Result::Ok(()),
//         Result::Err(err) => Result::Err(format!("failed to call: {} {}: {}", method, url, err)),
//     };
// }

fn init_hat() -> std::result::Result<Hat, String> {
    SimpleLogger::new()
        .init()
        .map_err(|err| format!("{}", err))?;

    let config_filename = env::var("HAT_CONFIG").unwrap_or("./config.yaml".to_string());
    let config = Config::read(&config_filename, false)
        .map_err(|err| format!("failed to read config {}", err))?;
    let port = env::var("HAT_PORT").unwrap_or("/dev/serial0".to_string());
    let tolerance_string = env::var("HAT_TOLERANCE").unwrap_or("0.15".to_string());
    let tolerance = tolerance_string
        .parse::<f32>()
        .map_err(|err| format!("invalid tolerance: {} ({})", tolerance_string, err))?;
    let mut hat = Hat::new(
        config,
        &port,
        tolerance,
        Box::new(move |message| {
            match message {
                HatMessage::ButtonPress(button_press) => {
                    // TODO
                    // if let Result::Err(err) = handle_button_press(&callback_config, &button_press) {
                    //     error!("{}", err);
                    // }
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
    client.subscribe(format!("ir/#"), paho_mqtt::QOS_1);
}

fn mqtt_on_connect_failure(client: &paho_mqtt::AsyncClient, _msg_id: u16, rc: i32) {
    error!("mqtt connection failure {}", rc);
    thread::sleep(Duration::from_millis(2500));
    client.reconnect_with_callbacks(mqtt_on_connect_success, mqtt_on_connect_failure);
}

fn init_mqtt(app_state: AppState) -> Result<paho_mqtt::AsyncClient, paho_mqtt::Error> {
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
            println!("{} - {}", topic, payload_str);
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

fn init() -> Result<paho_mqtt::AsyncClient, String> {
    let hat = init_hat().map_err(|err| format!("init hat error: {}", err))?;
    let app_state = AppState {
        hat: Arc::new(Mutex::new(hat)),
    };
    let mqtt_client = init_mqtt(app_state).map_err(|err| format!("init mqtt error: {}", err))?;
    return Result::Ok(mqtt_client);
}

fn main() -> Result<(), String> {
    match init() {
        Result::Err(err) => {
            error!("{}", err);
            process::exit(1);
        }
        Result::Ok(mqtt_client) => {
            info!("started");
            loop {
                thread::sleep(Duration::from_secs(60));
                let msg = paho_mqtt::Message::new("ir/status", "{}", paho_mqtt::QOS_1);
                mqtt_client.publish(msg);
            }
        }
    }
}
