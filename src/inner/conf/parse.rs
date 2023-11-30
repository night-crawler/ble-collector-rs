use std::sync::Arc;
use std::time::Duration;

use crate::inner::conf::dto::characteristic::CharacteristicConfigDto;
use crate::inner::conf::dto::service::ServiceConfigDto;
use regex::Regex;
use serde::{Deserialize, Serialize};
use serde_with::serde_as;
use serde_with::DurationSeconds;
use uuid::Uuid;

use crate::inner::conf::flat::FlatPeripheralConfig;
use crate::inner::conv::converter::Converter;
use crate::inner::dto::PeripheralKey;
use crate::inner::error::CollectorError;

#[serde_as]
#[derive(Debug, Clone, Serialize, Deserialize, Eq, PartialEq)]
pub(crate) enum CharacteristicConfig {
    Subscribe {
        name: Option<Arc<String>>,
        service_name: Option<Arc<String>>,
        service_uuid: Uuid,
        uuid: Uuid,
        history_size: usize,
        #[serde(default)]
        converter: Converter,
    },
    Poll {
        name: Option<Arc<String>>,
        uuid: Uuid,
        service_name: Option<Arc<String>>,
        service_uuid: Uuid,
        #[serde_as(as = "DurationSeconds")]
        delay_sec: Duration,
        history_size: usize,
        #[serde(default)]
        converter: Converter,
    },
}

impl TryFrom<(&CharacteristicConfigDto, &ServiceConfigDto)> for CharacteristicConfig {
    type Error = CollectorError;

    fn try_from(
        (char_conf, service_conf): (&CharacteristicConfigDto, &ServiceConfigDto),
    ) -> Result<Self, Self::Error> {
        let service_name = service_conf.name.clone();
        let service_uuid = service_conf.uuid;

        match char_conf {
            CharacteristicConfigDto::Subscribe {
                name,
                uuid,
                history_size,
                converter,
            } => Ok(CharacteristicConfig::Subscribe {
                name: name.clone(),
                service_name,
                service_uuid,
                uuid: *uuid,
                history_size: history_size.unwrap_or(service_conf.default_history_size),
                converter: converter.clone(),
            }),
            CharacteristicConfigDto::Poll {
                name,
                uuid,
                delay_sec,
                history_size,
                converter,
            } => Ok(CharacteristicConfig::Poll {
                name: name.clone(),
                uuid: *uuid,
                service_name,
                service_uuid,
                delay_sec: delay_sec.unwrap_or(service_conf.default_delay_sec),
                history_size: history_size.unwrap_or(service_conf.default_history_size),
                converter: converter.clone(),
            }),
        }
    }
}

impl CharacteristicConfig {
    pub(crate) fn name(&self) -> Option<Arc<String>> {
        match self {
            CharacteristicConfig::Subscribe { name, .. } => name.clone(),
            CharacteristicConfig::Poll { name, .. } => name.clone(),
        }
    }

    pub(crate) fn history_size(&self) -> usize {
        match self {
            CharacteristicConfig::Subscribe { history_size, .. } => *history_size,
            CharacteristicConfig::Poll { history_size, .. } => *history_size,
        }
    }
    pub(crate) fn service_name(&self) -> Option<Arc<String>> {
        match self {
            CharacteristicConfig::Subscribe { service_name, .. } => service_name.clone(),
            CharacteristicConfig::Poll { service_name, .. } => service_name.clone(),
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
