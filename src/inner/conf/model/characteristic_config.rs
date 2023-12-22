use std::sync::Arc;
use std::time::Duration;

use crate::inner::conf::dto::characteristic::CharacteristicConfigDto;
use crate::inner::conf::dto::publish::{PublishMetricConfigDto, PublishMqttConfigDto};
use crate::inner::conf::dto::service::ServiceConfigDto;
use crate::inner::conv::converter::Converter;
use serde::{Deserialize, Serialize};
use serde_with::serde_as;
use serde_with::DurationSeconds;
use uuid::Uuid;

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
        publish_metrics: Option<PublishMetricConfigDto>,
        publish_mqtt: Option<PublishMqttConfigDto>,
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
        publish_metrics: Option<PublishMetricConfigDto>,
        publish_mqtt: Option<PublishMqttConfigDto>,
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
                publish_metrics,
                publish_mqtt,
            } => Ok(CharacteristicConfig::Subscribe {
                name: name.clone(),
                service_name,
                service_uuid,
                uuid: *uuid,
                history_size: history_size.unwrap_or(service_conf.default_history_size),
                converter: converter.clone(),
                publish_metrics: publish_metrics.clone(),
                publish_mqtt: publish_mqtt.clone(),
            }),
            CharacteristicConfigDto::Poll {
                name,
                uuid,
                delay: delay_sec,
                history_size,
                converter,
                publish_metrics,
                publish_mqtt,
            } => Ok(CharacteristicConfig::Poll {
                name: name.clone(),
                uuid: *uuid,
                service_name,
                service_uuid,
                delay_sec: delay_sec.unwrap_or(service_conf.default_delay),
                history_size: history_size.unwrap_or(service_conf.default_history_size),
                converter: converter.clone(),
                publish_metrics: publish_metrics.clone(),
                publish_mqtt: publish_mqtt.clone(),
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

    pub(crate) fn publish_metrics(&self) -> Option<&PublishMetricConfigDto> {
        match self {
            CharacteristicConfig::Subscribe {
                publish_metrics, ..
            } => publish_metrics.as_ref(),
            CharacteristicConfig::Poll {
                publish_metrics, ..
            } => publish_metrics.as_ref(),
        }
    }

    pub(crate) fn publish_mqtt(&self) -> Option<&PublishMqttConfigDto> {
        match self {
            CharacteristicConfig::Subscribe { publish_mqtt, .. } => publish_mqtt.as_ref(),
            CharacteristicConfig::Poll { publish_mqtt, .. } => publish_mqtt.as_ref(),
        }
    }
}
