use crate::config::UnisonConfigAction;
use crate::AppState;
use futures::executor::block_on;
use log::debug;
use paho_mqtt;

pub fn do_action(app_state: &AppState, action: &UnisonConfigAction) -> Result<(), String> {
    let action_type: &str = &action.action_type;
    match action_type {
        "http" => {
            return do_http_action(&action);
        }
        "mqtt" => {
            return do_mqtt_action(
                app_state.mqtt_client.as_ref().expect("mqtt_client not set"),
                &action,
            );
        }
        _ => {
            return Result::Err(format!("invalid type: {}", action_type));
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
    debug!("invoking action http {} {}", method, url);
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
    debug!("invoking action mqtt {}", topic);

    let message = paho_mqtt::Message::new(topic, payload, paho_mqtt::QOS_0);
    block_on(client.publish(message)).map_err(|err| format!("mqtt publish failed: {}", err))?;
    return Result::Ok(());
}