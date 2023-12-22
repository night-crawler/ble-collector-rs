use kanal::Receiver;
use std::sync::Arc;

use metrics::{counter, Label};
use tracing::debug;

use crate::inner::metrics::PAYLOAD_PROCESSED_COUNT;
use crate::inner::model::characteristic_payload::CharacteristicPayload;
use crate::inner::process::PublishPayload;

pub(crate) struct MultiPublisher {
    receiver: Receiver<Arc<CharacteristicPayload>>,
    publishers: Vec<Arc<dyn PublishPayload + Send + Sync>>,
}

impl MultiPublisher {
    pub(crate) fn new(
        receiver: Receiver<Arc<CharacteristicPayload>>,
        publishers: Vec<Arc<dyn PublishPayload + Send + Sync>>,
    ) -> Self {
        Self {
            receiver,
            publishers,
        }
    }
    pub(crate) fn block_on_receiving(self: Arc<Self>) {
        let receiver = self.receiver.clone();
        for (index, payload) in receiver.enumerate() {
            let metric_labels = vec![
                Label::new("scope", "processing"),
                Label::new("peripheral", payload.fqcn.peripheral_address.to_string()),
                Label::new("service", payload.fqcn.service_uuid.to_string()),
                Label::new(
                    "characteristic",
                    payload.fqcn.characteristic_uuid.to_string(),
                ),
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
