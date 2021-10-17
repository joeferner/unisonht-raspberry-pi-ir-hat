#[macro_use]
extern crate rocket;

use clap::App;
use clap::Arg;
use log::{error, info};
use raspberry_pi_ir_hat::Config;
use raspberry_pi_ir_hat::{ButtonPress, Hat, HatError, HatMessage};
use rocket::http::ContentType;
use rocket::response::Responder;
use rocket::State;
use simple_logger::SimpleLogger;
use std::sync::Mutex;

struct MyState {
    hat: Mutex<Hat>,
}

#[derive(Responder)]
enum MyResponse {
    #[response(status = 200)]
    Ok(String, ContentType),
    #[response(status = 404)]
    NotFound(String),
    #[response(status = 408)]
    Timeout(String),
    #[response(status = 500)]
    ServerError(String),
}

static INDEX_HTML: &str = include_str!("static/index.html");

// created using https://github.com/remy/inliner
static SWAGGER_HTML: &str = include_str!("static/swagger.html");
static SWAGGER_JSON: &str = include_str!("static/swagger.json");

#[get("/")]
fn index_html() -> (ContentType, &'static str) {
    return (ContentType::HTML, INDEX_HTML);
}

#[get("/swagger.html")]
fn swagger_html() -> (ContentType, &'static str) {
    return (ContentType::HTML, SWAGGER_HTML);
}

#[get("/swagger.json")]
fn swagger_json() -> (ContentType, &'static str) {
    return (ContentType::JSON, SWAGGER_JSON);
}

#[get("/api/v1/config")]
fn get_config(state: &State<MyState>) -> MyResponse {
    return match state.hat.lock() {
        Result::Err(err) => MyResponse::ServerError(format!("failed to lock: {}", err)),
        Result::Ok(hat) => match hat.get_config().to_json_string() {
            Result::Err(err) => MyResponse::ServerError(format!("{}", err)),
            Result::Ok(config_json) => MyResponse::Ok(config_json, ContentType::JSON),
        },
    };
}

#[post("/api/v1/transmit/<remote_name>/<button_name>")]
fn transmit(state: &State<MyState>, remote_name: &str, button_name: &str) -> MyResponse {
    return match state.hat.lock() {
        Result::Err(err) => MyResponse::ServerError(format!("failed to lock: {}", err)),
        Result::Ok(mut hat) => match hat.transmit(remote_name, button_name) {
            Result::Err(err) => match err {
                HatError::InvalidButton {
                    remote_name,
                    button_name,
                } => MyResponse::NotFound(format!(
                    "button not found {}:{}",
                    remote_name, button_name
                )),
                HatError::Timeout(err) => MyResponse::Timeout(format!("{}", err)),
                _ => MyResponse::ServerError(format!("{}", err)),
            },
            Result::Ok(_) => MyResponse::Ok("{}".to_string(), ContentType::JSON),
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
    match button.get_json().get("unisonhtAction") {
        Option::None => return Result::Ok(()),
        Option::Some(action_value) => {
            let action = action_value.as_object().ok_or_else(|| {
                format!(
                    "button {}:{} unisonhtAction should be an object",
                    button_press.remote_name, button_press.button_name
                )
            })?;
            let action_type = action
                .get("type")
                .ok_or_else(|| {
                    format!(
                        "button {}:{} unisonhtAction should have 'type'",
                        button_press.remote_name, button_press.button_name
                    )
                })?
                .as_str()
                .ok_or_else(|| {
                    format!(
                        "button {}:{} unisonhtAction should have string 'type'",
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

    let args = App::new("UnisonHT - Raspberry Pi IrHat")
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

#[launch]
fn rocket() -> _ {
    match init() {
        Result::Err(err) => {
            error!("{}", err);
            std::process::exit(1);
        }
        Result::Ok(hat) => rocket::build()
            .manage(MyState {
                hat: Mutex::new(hat),
            })
            .mount(
                "/",
                routes![index_html, swagger_html, swagger_json, get_config, transmit],
            ),
    }
}
