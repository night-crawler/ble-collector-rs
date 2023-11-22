use std::collections::{HashMap, HashSet};
use std::sync::Arc;
use std::time::Duration;

use lazy_static::lazy_static;
use regex::Regex;
use serde::{Deserialize, Serialize};
use serde_with::DurationSeconds;
use serde_with::serde_as;
use tokio::sync::Mutex;
use uuid::Uuid;

use crate::inner::dto::PeripheralKey;
use crate::inner::error::{CollectorError, CollectorResult};

lazy_static! {
    pub(crate) static ref CONFIGURATION_MANAGER: ConfigurationManager = Default::default();
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

#[derive(Debug, Clone, Serialize, Deserialize)]
pub(crate) enum Filter {
    Contains(String),
    StartsWith(String),
    EndsWith(String),
    Equals(String),
    NotEquals(String),
    #[serde(with = "serde_regex")]
    Regex(Regex),
}

impl PartialEq<Self> for Filter {
    fn eq(&self, other: &Self) -> bool {
        match (self, other) {
            (Filter::Contains(left), Filter::Contains(right)) => left == right,
            (Filter::StartsWith(left), Filter::StartsWith(right)) => left == right,
            (Filter::EndsWith(left), Filter::EndsWith(right)) => left == right,
            (Filter::Equals(left), Filter::Equals(right)) => left == right,
            (Filter::NotEquals(left), Filter::NotEquals(right)) => left == right,
            (Filter::Regex(left), Filter::Regex(right)) => left.as_str() == right.as_str(),
            _ => false,
        }
    }
}

impl Eq for Filter {}

pub(crate) trait Evaluate<S, R> {
    fn evaluate(&self, source: S) -> R;
}

impl Evaluate<&str, bool> for Filter {
    fn evaluate(&self, source: &str) -> bool {
        match self {
            Filter::Contains(value) => source.contains(value),
            Filter::StartsWith(value) => source.starts_with(value),
            Filter::EndsWith(value) => source.ends_with(value),
            Filter::Equals(value) => source == value,
            Filter::NotEquals(value) => source != value,
            Filter::Regex(value) => value.is_match(source),
        }
    }
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

impl Evaluate<&PeripheralKey, bool> for BleServiceConfig {
    fn evaluate(&self, source: &PeripheralKey) -> bool {
        let adapter_matches = self
            .adapter
            .as_ref()
            .map(|filter| filter.evaluate(&source.adapter_id))
            .unwrap_or(true);
        let device_id_matches = self
            .device_id
            .as_ref()
            .map(|filter| filter.evaluate(&source.peripheral_address.to_string()))
            .unwrap_or(true);

        let name_matches = match (self.device_name.as_ref(), &source.name) {
            (Some(filter), Some(name)) => filter.evaluate(name),
            (None, Some(_)) => true,
            (Some(_), None) => false,
            (None, None) => true,
        };

        adapter_matches && device_id_matches && name_matches
    }
}

#[derive(Default)]
pub(crate) struct ConfigurationManager {
    services: Arc<Mutex<HashMap<String, Arc<BleServiceConfig>>>>,
}

impl ConfigurationManager {
    pub(crate) async fn add_services(&self, services: Vec<BleServiceConfig>) -> CollectorResult<()> {
        let mut unique_names = HashSet::new();
        for service in services.iter() {
            let service_name = service.name.clone();
            if !unique_names.insert(service_name.clone()) {
                return Err(CollectorError::DuplicateConfiguration(service_name));
            }
        }

        let mut existing_services = self.services.lock().await;
        for service in services.iter() {
            let service_name = service.name.clone();
            if existing_services.contains_key(&service_name) {
                return Err(CollectorError::DuplicateConfiguration(service_name));
            }
        }

        for service in services {
            existing_services.insert(service.name.clone(), Arc::new(service));
        }

        Ok(())
    }
    pub(crate) async fn add_service(&self, service: BleServiceConfig) -> CollectorResult<()> {
        let existing_services = self.services.lock().await;
        if existing_services.contains_key(&service.name) {
            return Err(CollectorError::DuplicateConfiguration(service.name));
        }
        self.services.lock().await.insert(service.name.clone(), Arc::new(service));
        Ok(())
    }
    pub(crate) async fn list_services(&self) -> Vec<Arc<BleServiceConfig>> {
        let services = self.services.lock().await;
        services.values().cloned().collect()
    }
    pub(crate) async fn get_matching_config(&self, peripheral_key: &PeripheralKey) -> Option<Arc<BleServiceConfig>> {
        let services = self.services.lock().await;
        services
            .values()
            .find(|service| service.evaluate(peripheral_key))
            .cloned()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// check serialization / deserialization
    #[test]
    fn test() {
        let config = CollectorConfiguration {
            services: vec![BleServiceConfig {
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
            }],
        };

        let serialized = serde_yaml::to_string(&config).unwrap();
        println!("{}", serialized);
        let deserialized: CollectorConfiguration = serde_yaml::from_str(&serialized).unwrap();
        assert_eq!(config, deserialized);
    }
}
