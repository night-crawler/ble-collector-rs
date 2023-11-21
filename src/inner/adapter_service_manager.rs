use std::sync::Arc;

use btleplug::api::{Central, CentralEvent, ScanFilter};
use btleplug::platform::Adapter;
use futures_util::StreamExt;
use log::info;
use tokio::task::JoinSet;

use crate::inner::error::CollectorError;

pub(crate) struct AdapterServiceManager {
    pub(crate) adapter: Arc<Adapter>,
    // discovery_events: kanal::AsyncReceiver<CentralEvent>
}


impl AdapterServiceManager {
    pub(crate) fn new(adapter: Adapter) -> Self {
        Self {
            adapter: Arc::new(adapter),
        }
    }

    pub(crate) async fn start_discovery(&self) -> btleplug::Result<()> {
        self.adapter.start_scan(ScanFilter::default()).await?;
        let adapter = Arc::clone(&self.adapter);

        let (sender, receiver) = kanal::unbounded_async::<CentralEvent>();

        let mut join_set = JoinSet::new();
        join_set.spawn(async move {
            discover_task(adapter, sender).await
        });

        join_set.spawn(async move {
            process_events(receiver).await
        });

        if let Some(result) = join_set.join_next().await {
            info!("Ending everything: {result:?}");
        }

        Ok(())
    }
}


async fn discover_task(adapter: Arc<Adapter>, sender: kanal::AsyncSender<CentralEvent>) -> Result<(), CollectorError> {
    let mut stream = adapter.events().await?;
    while let Some(event) = stream.next().await {
        sender.send(event).await?;
    }
    Err(CollectorError::EndOfStream)
}

async fn process_events(receiver: kanal::AsyncReceiver<CentralEvent>) -> Result<(), CollectorError> {
    // CONFIGURATION_MANAGER.
    while let Some(event) = receiver.stream().next().await {
        info!("Event: {event:?}");
    }

    Err(CollectorError::EndOfStream)
}
