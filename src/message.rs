use serde::{Deserialize, Serialize};
use std::collections::HashMap;

#[derive(Serialize, Deserialize)]
pub struct StatusMessage {
    pub devices: HashMap<String, StatusMessageDevice>,
}

#[derive(Serialize, Deserialize)]
pub struct StatusMessageDevice {
    pub milliamps: u32,
    pub is_on: bool,
}

#[derive(Serialize, Deserialize)]
pub struct TransmitMessage {
    pub remote_name: String,
    pub button_name: String,
}
