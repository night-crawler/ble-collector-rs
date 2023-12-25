use kanal::Receiver;
use std::sync::Arc;

use metrics::{counter, Label};
use tracing::debug;

use crate::inner::metrics::PAYLOAD_PROCESSED_COUNT;
use crate::inner::model::characteristic_payload::CharacteristicPayload;
use crate::inner::model::collector_event::CollectorEvent;
use crate::inner::publish::PublishPayload;

pub(crate) struct MultiPublisher {
    receiver: Receiver<CollectorEvent>,
    publishers: Vec<Arc<dyn PublishPayload + Send + Sync>>,
}

impl MultiPublisher {
    pub(crate) fn new(
        receiver: Receiver<CollectorEvent>,
        publishers: Vec<Arc<dyn PublishPayload + Send + Sync>>,
    ) -> Self {
        Self { receiver, publishers }
    }
    pub(crate) fn block_on_receiving(self: Arc<Self>) {
        let receiver = self.receiver.clone();
        for (index, payload) in receiver.enumerate() {
            let CollectorEvent::Payload(payload) = payload else {
                continue;
            };
            let metric_labels = vec![
                Label::new("adapter", payload.adapter_info.id.to_string()),
                Label::new("scope", "processing"),
                Label::new("peripheral", payload.fqcn.peripheral.to_string()),
                Label::new("service", payload.fqcn.service.to_string()),
                Label::new("characteristic", payload.fqcn.characteristic.to_string()),
            ];
            self.publish(payload);
            counter!(PAYLOAD_PROCESSED_COUNT.metric_name, 1, metric_labels);
            if index % 10000 == 0 {
                debug!("Processed {index} payloads");
            }
        }
    }

    pub(crate) fn publish(&self, payload: Arc<CharacteristicPayload>) {
        for publisher in &self.publishers {
            publisher.publish(Arc::clone(&payload));
        }
    }
}
