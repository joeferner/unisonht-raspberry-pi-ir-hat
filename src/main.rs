use actix_web;
use actix_web::{get, post, web, HttpResponse, HttpServer, Responder};
use clap;
use clap::Arg;
use log::{error, info};
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

static INDEX_HTML: &str = include_str!("static/index.html");

// created using https://github.com/remy/inliner
static SWAGGER_HTML: &str = include_str!("static/swagger.html");
static SWAGGER_JSON: &str = include_str!("static/swagger.json");

#[get("/")]
pub async fn index_html() -> impl Responder {
    return HttpResponse::Ok()
        .content_type("text/html")
        .body(INDEX_HTML);
}

#[get("/swagger.html")]
pub async fn swagger_html() -> impl Responder {
    return HttpResponse::Ok()
        .content_type("text/html")
        .body(SWAGGER_HTML);
}

#[get("/swagger.json")]
pub async fn swagger_json() -> impl Responder {
    return HttpResponse::Ok()
        .content_type("application/json")
        .body(SWAGGER_JSON);
}

#[get("/api/v1/config")]
async fn get_config(state: web::Data<AppState>) -> impl Responder {
    match state.hat.lock() {
        Result::Err(err) => {
            error!("failed to lock {}", err);
            process::exit(1);
        }
        Result::Ok(hat) => match hat.get_config().to_json_string() {
            Result::Err(err) => {
                error!("get config error {}", err);
                return HttpResponse::InternalServerError().finish();
            }
            Result::Ok(config_json) => {
                return HttpResponse::Ok().json(config_json);
            }
        },
    };
}

#[get("/api/v1/status")]
async fn get_status(state: web::Data<AppState>) -> impl Responder {
    match state.hat.lock() {
        Result::Err(err) => {
            error!("failed to lock {}", err);
            process::exit(1);
        }
        Result::Ok(mut hat) => {
            let mut devices: HashMap<String, GetStatusResponseDevice> = HashMap::new();
            for ch in [CurrentChannel::Channel0, CurrentChannel::Channel1] {
                match hat.get_current(ch) {
                    Result::Err(err) => match err {
                        HatError::Timeout(err) => {
                            error!("timeout {}", err);
                            return HttpResponse::RequestTimeout().finish();
                        }
                        _ => {
                            error!("transmit error {}", err);
                            return HttpResponse::InternalServerError().finish();
                        }
                    },
                    Result::Ok(resp) => {
                        let device_name = match ch {
                            CurrentChannel::Channel0 => "device0",
                            CurrentChannel::Channel1 => "device1",
                        };
                        devices.insert(
                            device_name.to_string(),
                            GetStatusResponseDevice {
                                milliamps: resp.milliamps,
                            },
                        );
                    }
                }
            }
            return HttpResponse::Ok().json(GetStatusResponse { devices });
        }
    };
}

#[post("/api/v1/transmit/<remote_name>/<button_name>")]
async fn transmit(
    state: web::Data<AppState>,
    web::Path((remote_name, button_name)): web::Path<(String, String)>,
) -> impl Responder {
    match state.hat.lock() {
        Result::Err(err) => {
            error!("failed to lock {}", err);
            process::exit(1);
        }
        Result::Ok(mut hat) => match hat.transmit(&remote_name, &button_name) {
            Result::Err(err) => match err {
                HatError::InvalidButton {
                    remote_name,
                    button_name,
                } => {
                    error!("button not found {}:{}", remote_name, button_name);
                    return HttpResponse::NotFound().finish();
                }
                HatError::Timeout(err) => {
                    error!("timeout {}", err);
                    return HttpResponse::RequestTimeout().finish();
                }
                _ => {
                    error!("transmit error {}", err);
                    return HttpResponse::InternalServerError().finish();
                }
            },
            Result::Ok(_) => {
                return HttpResponse::Ok().json({});
            }
        },
    };
}

fn handle_button_press(config: &Config, button_press: &ButtonPress) -> Result<(), String> {
    let button = config
        .get_button(&button_press.remote_name, &button_press.button_name)
        .ok_or_else(|| {
            format!(
                "could not find button {}:{}",
                button_press.remote_name, button_press.button_name
            )
        })?;
    match button.get_json().get("action") {
        Option::None => return Result::Ok(()),
        Option::Some(action_value) => {
            let action = action_value.as_object().ok_or_else(|| {
                format!(
                    "button {}:{} action should be an object",
                    button_press.remote_name, button_press.button_name
                )
            })?;
            let action_type = action
                .get("type")
                .ok_or_else(|| {
                    format!(
                        "button {}:{} action should have 'type'",
                        button_press.remote_name, button_press.button_name
                    )
                })?
                .as_str()
                .ok_or_else(|| {
                    format!(
                        "button {}:{} action should have string 'type'",
                        button_press.remote_name, button_press.button_name
                    )
                })?;
            match action_type {
                "http" => {
                    return do_http_action(&action).map_err(|err| {
                        format!(
                            "error executing action {}:{}: {}",
                            button_press.remote_name, button_press.button_name, err
                        )
                    });
                }
                _ => {
                    return Result::Err(format!(
                        "button {}:{} has invalid type: {}",
                        button_press.remote_name, button_press.button_name, action_type
                    ));
                }
            }
        }
    }
}

fn do_http_action(action: &serde_json::Map<String, serde_json::Value>) -> Result<(), String> {
    let url = action
        .get("url")
        .ok_or_else(|| format!("'http' actions should have a url"))?
        .as_str()
        .ok_or_else(|| format!("'http' actions should have a string url"))?;
    let method = match action.get("method") {
        Option::None => Result::Ok("post".to_string()),
        Option::Some(method_value) => match method_value.as_str() {
            Option::None => Result::Err(format!("'http' actions should have a string url")),
            Option::Some(m) => Result::Ok(m.to_lowercase()),
        },
    }?;
    info!("calling {} {}", method, url);
    let response = match method.as_ref() {
        "get" => ureq::get(url).call(),
        "post" => ureq::post(url).call(),
        _ => return Result::Err(format!("unexpected http method: {}", method)),
    };
    return match response {
        Result::Ok(_) => Result::Ok(()),
        Result::Err(err) => Result::Err(format!("failed to call: {} {}: {}", method, url, err)),
    };
}

fn init() -> std::result::Result<Hat, String> {
    SimpleLogger::new()
        .init()
        .map_err(|err| format!("{}", err))?;

    let args = clap::App::new("UnisonHT - Raspberry Pi IrHat")
        .version("1.0.0")
        .author("Joe Ferner <joe@fernsroth.com>")
        .about("UnisonHT Raspberry Pi IrHat web server")
        .arg(
            Arg::with_name("config")
                .short("c")
                .long("config")
                .value_name("FILE")
                .help("File to load IR signals to")
                .required(true)
                .takes_value(true),
        )
        .arg(
            Arg::with_name("port")
                .short("p")
                .long("port")
                .default_value("/dev/serial0")
                .help("Path to serial port")
                .takes_value(true),
        )
        .arg(
            Arg::with_name("tolerance")
                .short("t")
                .long("tolerance")
                .default_value("0.15")
                .help("Signal matching tolerance")
                .takes_value(true),
        )
        .get_matches();

    let config_filename = args.value_of("config").unwrap();
    let config = Config::read(config_filename, false)
        .map_err(|err| format!("failed to read config {}", err))?;
    let port = args.value_of("port").unwrap();
    let tolerance = args
        .value_of("tolerance")
        .unwrap()
        .parse::<f32>()
        .map_err(|err| {
            format!(
                "invalid tolerance: {} ({})",
                args.value_of("tolerance").unwrap(),
                err
            )
        })?;
    let callback_config = config.clone();
    let mut hat = Hat::new(
        config,
        port,
        tolerance,
        Box::new(move |message| {
            match message {
                HatMessage::ButtonPress(button_press) => {
                    if let Result::Err(err) = handle_button_press(&callback_config, &button_press) {
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

#[actix_web::main]
async fn main() -> std::io::Result<()> {
    match init() {
        Result::Err(err) => {
            error!("{}", err);
            std::process::exit(1);
        }
        Result::Ok(hat) => {
            let bind = format!(
                "{}:{}",
                env::var("HOST").unwrap_or("0.0.0.0".to_string()),
                env::var("PORT").unwrap_or("8080".to_string())
            );
            let hat = Arc::new(Mutex::new(hat));
            let http = HttpServer::new(move || {
                let data = AppState { hat: hat.clone() };
                actix_web::App::new()
                    .data(data)
                    .service(index_html)
                    .service(swagger_html)
                    .service(swagger_json)
                    .service(get_config)
                    .service(transmit)
            })
            .bind(bind.clone())?
            .run();
            info!("listening http://{}", bind);
            return http.await;
        }
    }
}
