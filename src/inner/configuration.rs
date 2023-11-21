use std::sync::Arc;
use std::time::Duration;
use lazy_static::lazy_static;

use serde::{Deserialize, Serialize};
use tokio::sync::Mutex;
use uuid::Uuid;


lazy_static! {
    pub(crate) static ref CONFIGURATION_MANAGER: ConfigurationManager = {
        Default::default()
    };
}


#[derive(Debug, Clone, Serialize, Deserialize, Eq, PartialEq)]
pub(crate) enum CharacteristicConfig {
    Subscribe {
        name: Option<String>,
        uuid: Uuid,
    },
    Poll {
        name: Option<String>,
        uuid: Uuid,
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


#[derive(Debug, Clone, Serialize, Deserialize, Eq, PartialEq)]
pub(crate) struct BleServiceConfig {
    name: String,
    adapter: Option<Filter>,
    device_id: Option<Filter>,
    device_name: Option<Filter>,
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
    pub(crate) fn new(services: Vec<BleServiceConfig>) -> Self {
        Self {
            services: Arc::new(Mutex::new(services))
        }
    }
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
    /// check serialization / deserialization
    #[test]
    fn test() {}
}