use crate::action::do_action;
use crate::config::UnisonConfig;
use crate::AppState;
use log::error;
use raspberry_pi_ir_hat::Config;
use raspberry_pi_ir_hat::{ButtonPress, Hat, HatMessage};
use simple_logger::SimpleLogger;
use std::env;

pub fn init_hat(app_state: &AppState, config_text: &str) -> std::result::Result<Hat, String> {
    SimpleLogger::new()
        .init()
        .map_err(|err| format!("{}", err))?;

    let config =
        Config::from_str(config_text).map_err(|err| format!("failed to read config {}", err))?;
    let port = env::var("HAT_PORT").unwrap_or("/dev/serial0".to_string());
    let tolerance_string = env::var("HAT_TOLERANCE").unwrap_or("0.15".to_string());
    let tolerance = tolerance_string
        .parse::<f32>()
        .map_err(|err| format!("invalid tolerance: {} ({})", tolerance_string, err))?;
    let hat_app_state = app_state.clone();
    let unison_config = UnisonConfig::from_str(config_text)?;
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
    return do_action(app_state, &action).map_err(|err| {
        format!(
            "button {}:{} action error: {}",
            button_press.remote_name, button_press.button_name, err
        )
    });
}
