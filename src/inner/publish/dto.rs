use crate::inner::conv::converter::CharacteristicValue;
use crate::inner::model::characteristic_payload::CharacteristicPayload;
use crate::inner::model::fqcn::Fqcn;
use chrono::{DateTime, Utc};
use serde::Serialize;
use std::sync::Arc;

#[derive(Debug, Serialize)]
pub(crate) struct ApiDataPoint {
    pub(crate) ts: DateTime<Utc>,
    pub(crate) value: CharacteristicValue,
}

impl From<&CharacteristicPayload> for ApiDataPoint {
    fn from(value: &CharacteristicPayload) -> Self {
        Self {
            ts: value.created_at,
            value: value.value.clone(),
        }
    }
}

#[derive(Debug, Serialize)]
pub(crate) struct MqttDataPoint {
    pub(crate) fqcn: Arc<Fqcn>,
    pub(crate) value: CharacteristicValue,
}

impl From<&CharacteristicPayload> for MqttDataPoint {
    fn from(value: &CharacteristicPayload) -> Self {
        Self {
            fqcn: value.fqcn.clone(),
            value: value.value.clone(),
        }
    }
}
