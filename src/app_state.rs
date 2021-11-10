use paho_mqtt;
use raspberry_pi_ir_hat::Hat;
use std::sync::Arc;
use std::sync::Mutex;

#[derive(Clone)]
pub struct AppState {
    pub hat: Option<Arc<Mutex<Hat>>>,
    pub mqtt_client: Option<Arc<Mutex<paho_mqtt::AsyncClient>>>,
    pub topic_prefix: String,
}
