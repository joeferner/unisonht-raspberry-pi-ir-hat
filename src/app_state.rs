use crate::config::UnisonConfig;
use raspberry_pi_ir_hat::Hat;
use rumqttc;

pub struct AppState {
    pub hat: Option<Hat>,
    pub mqtt_client: Option<rumqttc::Client>,
    pub topic_prefix: String,
    pub unison_config: UnisonConfig,
}
