use crate::inner::model::characteristic_payload::CharacteristicPayload;
use std::sync::Arc;

pub(crate) mod api_publisher;
pub(crate) mod metric_publisher;
pub(crate) mod processor;

pub(crate) trait ProcessPayload {
    fn process(&self, payload: Arc<CharacteristicPayload>);
}
