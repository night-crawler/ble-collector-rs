use crate::inner::model::characteristic_payload::CharacteristicPayload;
use std::sync::Arc;

pub(crate) mod api_publisher;
pub(crate) mod metric_publisher;
pub(crate) mod multi_publisher;

pub(crate) trait PublishPayload {
    fn publish(&self, payload: Arc<CharacteristicPayload>);
}

pub(crate) struct FanOutSender<T> {
    pub(crate) senders: Vec<kanal::AsyncSender<T>>,
}

impl<T> FanOutSender<T> {
    pub(crate) fn new(senders: Vec<kanal::AsyncSender<T>>) -> Self {
        Self { senders }
    }

    pub(crate) fn add(&mut self, sender: kanal::AsyncSender<T>) {
        self.senders.push(sender);
    }

    pub(crate) async fn send(&self, payload: T) -> Result<(), kanal::SendError>
    where
        T: Clone,
    {
        for sender in &self.senders {
            sender.send(payload.clone()).await?;
        }

        Ok(())
    }
}
