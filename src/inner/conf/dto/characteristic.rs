use std::sync::Arc;
use std::time::Duration;

use metrics::Label;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::inner::conv::converter::Converter;
use crate::inner::metrics::MetricType;

#[derive(Debug, Clone, Serialize, Deserialize, Eq, PartialEq)]
pub(crate) struct PublishMetricConfigDto {
    pub(crate) metric_type: MetricType,
    pub(crate) name: Arc<String>,
    pub(crate) description: Option<Arc<String>>,
    pub(crate) unit: Arc<String>,
    pub(crate) labels: Option<Arc<Vec<(String, String)>>>,
}

impl PublishMetricConfigDto {
    pub(crate) fn labels(&self) -> Vec<Label> {
        self.labels
            .iter()
            .flat_map(|labels| labels.iter())
            .map(|(k, v)| Label::new(k.to_string(), v.to_string()))
            .collect()
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, Eq, PartialEq)]
pub(crate) enum CharacteristicConfigDto {
    Subscribe {
        name: Option<Arc<String>>,
        uuid: Uuid,
        history_size: Option<usize>,
        #[serde(default)]
        converter: Converter,
        publish_metrics: Option<PublishMetricConfigDto>,
    },
    Poll {
        name: Option<Arc<String>>,
        uuid: Uuid,
        #[serde(default)]
        #[serde(with = "humantime_serde")]
        delay: Option<Duration>,
        history_size: Option<usize>,
        #[serde(default)]
        converter: Converter,
        publish_metrics: Option<PublishMetricConfigDto>,
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
