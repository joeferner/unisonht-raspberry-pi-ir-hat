#[macro_use]
extern crate rocket;

use clap::App;
use clap::Arg;
use raspberry_pi_ir_hat::Config;
use raspberry_pi_ir_hat::{Hat, HatError};
use rocket::http::ContentType;
use rocket::response::Responder;
use rocket::State;
use std::sync::Mutex;

struct MyState {
    hat: Mutex<Hat>,
}

#[derive(Responder)]
enum MyResponse {
    #[response(status = 200)]
    Ok(String),
    #[response(status = 404)]
    NotFound(String),
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

#[get("/config")]
fn get_config(state: &State<MyState>) -> MyResponse {
    return match state.hat.lock() {
        Result::Err(err) => MyResponse::ServerError(format!("failed to lock: {}", err)),
        Result::Ok(hat) => match serde_json::to_string(hat.get_config()) {
            Result::Err(err) => MyResponse::ServerError(format!("{}", err)),
            Result::Ok(config_json) => MyResponse::Ok(config_json),
        },
    };
}

#[post("/transmit/<remote_name>/<button_name>")]
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
                _ => MyResponse::ServerError(format!("{}", err)),
            },
            Result::Ok(_) => MyResponse::Ok(format!("success")),
        },
    };
}

fn init() -> std::result::Result<Hat, String> {
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
    let mut hat = Hat::new(
        config,
        port,
        tolerance,
        Box::new(|message| {
            println!("{:#?}", message);
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
            println!("{}", err);
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
