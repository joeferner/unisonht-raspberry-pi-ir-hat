use std::env;
use std::time::Duration;

pub struct ConfigEnv {
    pub topic_prefix: String,
    pub config_filename: String,
    pub hat_port: String,
    pub hat_tolerance: f32,
    pub status_interval: Duration,
    pub mqtt_uri: String,
    pub mqtt_client_id: String,
}

impl ConfigEnv {
    pub fn get() -> Result<ConfigEnv, String> {
        return Result::Ok(ConfigEnv {
            topic_prefix: ConfigEnv::get_topic_prefix(),
            config_filename: env::var("HAT_CONFIG").unwrap_or("./config.yaml".to_string()),
            hat_port: env::var("HAT_PORT").unwrap_or("/dev/serial0".to_string()),
            hat_tolerance: ConfigEnv::get_hat_tolerance()?,
            status_interval: ConfigEnv::get_status_interval()?,
            mqtt_uri: env::var("MQTT_URI").unwrap_or("tcp://localhost:1883".to_string()),
            mqtt_client_id: env::var("MQTT_CLIENT_ID").unwrap_or("raspirhat".to_string()),
        });
    }

    fn get_topic_prefix() -> String {
        let mut topic_prefix = env::var("MQTT_TOPIC_PREFIX").unwrap_or("ir/".to_string());
        if !topic_prefix.ends_with("/") {
            topic_prefix = topic_prefix + "/";
        }
        return topic_prefix;
    }

    fn get_hat_tolerance() -> Result<f32, String> {
        let tolerance_string = env::var("HAT_TOLERANCE").unwrap_or("0.15".to_string());
        return tolerance_string
            .parse::<f32>()
            .map_err(|err| format!("invalid tolerance: {} ({})", tolerance_string, err));
    }

    fn get_status_interval() -> Result<Duration, String> {
        let status_interval_str = env::var("STATUS_INTERVAL").unwrap_or("60".to_string());
        let status_interval_seconds = status_interval_str
            .parse::<u64>()
            .map_err(|err| format!("invalid status interval: {} ({})", status_interval_str, err))?;
        return Result::Ok(Duration::from_secs(status_interval_seconds));
    }
}
