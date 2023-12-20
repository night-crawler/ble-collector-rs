use std::sync::Arc;

use metrics::{counter, Label};
use tracing::debug;

use crate::inner::metrics::PAYLOAD_PROCESSED_COUNT;
use crate::inner::model::characteristic_payload::CharacteristicPayload;
use crate::inner::process::ProcessPayload;

pub(crate) struct PayloadProcessor {
    receiver: kanal::Receiver<CharacteristicPayload>,
    processors: Vec<Arc<dyn ProcessPayload + Send + Sync>>,
}

impl PayloadProcessor {
    pub(crate) fn new(
        receiver: kanal::Receiver<CharacteristicPayload>,
        processors: Vec<Arc<dyn ProcessPayload + Send + Sync>>,
    ) -> Self {
        Self {
            receiver,
            processors,
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
            self.process(payload);
            counter!(PAYLOAD_PROCESSED_COUNT.metric_name, 1, metric_labels);
            if index % 10000 == 0 {
                debug!("Processed {index} payloads");
            }
        }
    }

    pub(crate) fn process(&self, payload: CharacteristicPayload) {
        let payload = Arc::new(payload);
        for processor in &self.processors {
            processor.process(Arc::clone(&payload));
        }
    }
}
