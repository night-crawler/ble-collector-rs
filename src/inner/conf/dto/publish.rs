use crate::inner::metrics::MetricType;
use metrics::Label;
use serde::{Deserialize, Serialize};
use std::sync::Arc;

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
#[allow(clippy::enum_variant_names)]
#[derive(Debug, Clone, Serialize, Deserialize, Eq, PartialEq, Default, Copy)]
pub(crate) enum Qos {
    AtMostOnce,
    #[default]
    AtLeastOnce,
    ExactlyOnce,
}

#[derive(Debug, Clone, Serialize, Deserialize, Eq, PartialEq)]
pub(crate) struct DiscoverySettings {
    pub(crate) config_topic: Arc<String>,

    #[serde(default)]
    pub(crate) retain: Option<bool>,
    #[serde(default)]
    pub(crate) qos: Option<Qos>,

    #[serde(flatten)]
    pub(crate) remainder: serde_yaml::Value,
}

#[derive(Debug, Clone, Serialize, Deserialize, Eq, PartialEq)]
pub(crate) struct PublishMqttConfigDto {
    pub(crate) state_topic: Arc<String>,
    pub(crate) unit: Option<Arc<String>>,
    #[serde(default)]
    pub(crate) retain: bool,
    #[serde(default)]
    pub(crate) qos: Qos,

    pub(crate) discovery: Option<Arc<DiscoverySettings>>,
}

impl PublishMqttConfigDto {
    pub(crate) fn qos(&self) -> rumqttc::v5::mqttbytes::QoS {
        self.qos.into()
    }
}

impl From<Qos> for rumqttc::v5::mqttbytes::QoS {
    fn from(value: Qos) -> Self {
        match value {
            Qos::AtMostOnce => rumqttc::v5::mqttbytes::QoS::AtMostOnce,
            Qos::AtLeastOnce => rumqttc::v5::mqttbytes::QoS::AtLeastOnce,
            Qos::ExactlyOnce => rumqttc::v5::mqttbytes::QoS::ExactlyOnce,
        }
    }
}
