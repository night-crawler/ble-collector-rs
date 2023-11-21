use std::sync::Arc;
use std::time::Duration;
use lazy_static::lazy_static;
use serde_with::DurationSeconds;

use serde::{Deserialize, Serialize};
use serde_with::serde_as;
use tokio::sync::Mutex;
use uuid::Uuid;


lazy_static! {
    pub(crate) static ref CONFIGURATION_MANAGER: ConfigurationManager = {
        Default::default()
    };
}


#[serde_as]
#[derive(Debug, Clone, Serialize, Deserialize, Eq, PartialEq)]
pub(crate) enum CharacteristicConfig {
    Subscribe {
        name: Option<String>,
        uuid: Uuid,
    },
    Poll {
        name: Option<String>,
        uuid: Uuid,
        #[serde_as(as = "Option<DurationSeconds>")]
        timeout: Option<Duration>,
    },
}


#[derive(Debug, Clone, Serialize, Deserialize, Eq, PartialEq)]
pub(crate) enum Filter {
    Contains(String),
    StartsWith(String),
    EndsWith(String),
    Equals(String),
    NotEquals(String),
    Regex(String),
}


#[serde_as]
#[derive(Debug, Clone, Serialize, Deserialize, Eq, PartialEq)]
pub(crate) struct BleServiceConfig {
    name: String,
    adapter: Option<Filter>,
    device_id: Option<Filter>,
    device_name: Option<Filter>,

    #[serde_as(as = "Option<DurationSeconds>")]
    default_timeout: Option<Duration>,
    characteristics: Vec<CharacteristicConfig>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Eq, PartialEq)]
pub(crate) struct CollectorConfiguration {
    pub(crate) services: Vec<BleServiceConfig>,
}


#[derive(Default)]
pub(crate) struct ConfigurationManager {
    services: Arc<Mutex<Vec<BleServiceConfig>>>,
}

impl ConfigurationManager {
    pub(crate) async fn add_services(&self, services: Vec<BleServiceConfig>) {
        self.services.lock().await.extend(services);
    }
    pub(crate) async fn add_service(&self, service: BleServiceConfig) {
        self.services.lock().await.push(service);
    }
    pub(crate) async fn list_services(&self) -> Vec<BleServiceConfig> {
        self.services.lock().await.clone()
    }
}


#[cfg(test)]
mod tests {
    use super::*;
    /// check serialization / deserialization
    #[test]
    fn test() {
        let config = CollectorConfiguration {
            services: vec![
                BleServiceConfig {
                    name: "test".to_string(),
                    adapter: Some(Filter::Contains("hci0".to_string())),
                    device_id: Some(Filter::StartsWith("FA:6F".to_string())),
                    device_name: Some(Filter::EndsWith("test".to_string())),
                    default_timeout: None,
                    characteristics: vec![
                        CharacteristicConfig::Subscribe {
                            name: None,
                            uuid: Uuid::nil(),
                        },
                        CharacteristicConfig::Poll {
                            name: None,
                            uuid: Uuid::nil(),
                            timeout: Some(Duration::from_secs(1)),
                        },
                    ],
                }
            ]
        };

        let serialized = serde_yaml::to_string(&config).unwrap();
        println!("{}", serialized);
        let deserialized: CollectorConfiguration = serde_yaml::from_str(&serialized).unwrap();
        assert_eq!(config, deserialized);
    }
}