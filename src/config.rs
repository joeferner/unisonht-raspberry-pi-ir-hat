use serde::{Deserialize, Serialize};
use std::collections::HashMap;

#[derive(Debug, PartialEq, Serialize, Deserialize)]
pub struct UnisonConfig {
    pub remotes: HashMap<String, UnisonConfigRemote>,
    pub devices: Vec<UnisonConfigDevice>,
}

impl UnisonConfig {
    pub fn from_str(config_text: &str) -> Result<UnisonConfig, String> {
        let unison_config: UnisonConfig = serde_yaml::from_str(config_text).map_err(|err| {
            format!(
                "could not read config: contained invalid yaml values: {}",
                err
            )
        })?;
        return Result::Ok(unison_config);
    }
}

#[derive(Debug, PartialEq, Serialize, Deserialize)]
pub struct UnisonConfigRemote {
    pub buttons: HashMap<String, UnisonConfigButton>,
}

#[derive(Debug, PartialEq, Serialize, Deserialize)]
pub struct UnisonConfigButton {
    pub action: Option<UnisonConfigAction>,
}

#[derive(Debug, PartialEq, Serialize, Deserialize)]
pub struct UnisonConfigAction {
    #[serde(rename = "type")]
    pub action_type: String,

    // type: http
    pub url: Option<String>,

    // type: http
    pub method: Option<String>,

    // type: mqtt
    pub topic: Option<String>,

    // type: mqtt
    pub payload: Option<String>,
}

#[derive(Debug, PartialEq, Serialize, Deserialize)]
pub struct UnisonConfigDevice {
    pub name: String,
    pub on_threshold_milliamps: u32,
}
