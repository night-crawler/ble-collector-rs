use std::time::Duration;

use regex::Regex;
use serde::{Deserialize, Serialize};
use serde_with::serde_as;
use serde_with::DurationSeconds;
use uuid::Uuid;

use crate::inner::conf::flat::FlatPeripheralConfig;
use crate::inner::dto::PeripheralKey;

#[derive(Debug, Clone, Serialize, Deserialize, Eq, PartialEq, Default)]
pub(crate) enum Converter {
    #[default]
    Raw,
    I32Raw,
    I32 {
        m: i8,
        d: i32,
        b: i32,
    },
}

fn compute_r(c: i64, m: i8, d: i32, b: i32) -> f64 {
    if !(-10..=10).contains(&m) {
        panic!("Multiplier should be between -10 and +10");
    }

    (c as f64) * (m as f64) * 10f64.powi(d) * 2f64.powi(b)
}

impl Converter {
    pub(crate) fn convert(&self, value: Vec<u8>) -> Option<f64> {
        match self {
            Converter::Raw => {
                let value = i32::from_le_bytes([value[0], value[1], value[2], value[3]]);
                Some(value as f64)
            }
            Converter::I32Raw => {
                Some(i32::from_le_bytes([value[0], value[1], value[2], value[3]]) as f64)
            }
            &Converter::I32 { m, d, b } => {
                let value = i32::from_le_bytes([value[0], value[1], value[2], value[3]]);
                let value = compute_r(value as i64, m, d, b);
                Some(value)
            }
        }
    }
}

#[serde_as]
#[derive(Debug, Clone, Serialize, Deserialize, Eq, PartialEq)]
pub(crate) enum CharacteristicConfigDto {
    Subscribe {
        name: Option<String>,
        uuid: Uuid,
        history_size: usize,
        #[serde(default)]
        converter: Converter,
    },
    Poll {
        name: Option<String>,
        uuid: Uuid,
        #[serde_as(as = "Option<DurationSeconds>")]
        delay_sec: Option<Duration>,
        history_size: usize,
        #[serde(default)]
        converter: Converter,
    },
}

impl CharacteristicConfigDto {
    pub(crate) fn uuid(&self) -> &Uuid {
        match self {
            CharacteristicConfigDto::Subscribe { uuid, .. } => uuid,
            CharacteristicConfigDto::Poll { uuid, .. } => uuid,
        }
    }
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
pub(crate) struct ServiceConfigDto {
    pub(crate) uuid: Uuid,
    #[serde_as(as = "DurationSeconds")]
    pub(crate) default_timeout_sec: Duration,
    pub(crate) characteristics: Vec<CharacteristicConfigDto>,
}

#[serde_as]
#[derive(Debug, Clone, Serialize, Deserialize, Eq, PartialEq)]
pub(crate) struct PeripheralConfigDto {
    pub(crate) name: String,
    pub(crate) adapter: Option<Filter>,
    pub(crate) device_id: Option<Filter>,
    pub(crate) device_name: Option<Filter>,
    pub(crate) services: Vec<ServiceConfigDto>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Eq, PartialEq)]
pub(crate) struct CollectorConfigurationDto {
    pub(crate) peripherals: Vec<PeripheralConfigDto>,
}

impl Evaluate<&PeripheralKey, bool> for FlatPeripheralConfig {
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

#[cfg(test)]
mod tests {
    use super::*;

    /// check serialization / deserialization
    #[test]
    fn test() {
        let config = CollectorConfigurationDto {
            peripherals: vec![PeripheralConfigDto {
                name: "test".to_string(),
                adapter: Some(Filter::Contains("hci0".to_string())),
                device_id: Some(Filter::StartsWith("FA:6F".to_string())),
                device_name: Some(Filter::EndsWith("test".to_string())),
                services: vec![ServiceConfigDto {
                    uuid: Uuid::nil(),
                    default_timeout_sec: Duration::from_secs(5),
                    characteristics: vec![
                        CharacteristicConfigDto::Subscribe {
                            history_size: 10,
                            name: Some("test".to_string()),
                            uuid: Uuid::nil(),
                            converter: Default::default(),
                        },
                        CharacteristicConfigDto::Poll {
                            history_size: 10,
                            name: Some("test".to_string()),
                            uuid: Uuid::nil(),
                            delay_sec: Some(Duration::from_secs(1)),
                            converter: Default::default(),
                        },
                    ],
                }],
            }],
        };

        let serialized = serde_yaml::to_string(&config).unwrap();
        println!("{}", serialized);
        let deserialized: CollectorConfigurationDto = serde_yaml::from_str(&serialized).unwrap();
        assert_eq!(config, deserialized);
    }
}
