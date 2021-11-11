use crate::config::UnisonConfig;
use paho_mqtt;
use raspberry_pi_ir_hat::Hat;

pub struct AppState {
    pub hat: Option<Hat>,
    pub mqtt_client: Option<paho_mqtt::AsyncClient>,
    pub topic_prefix: String,
    pub unison_config: UnisonConfig,
}
