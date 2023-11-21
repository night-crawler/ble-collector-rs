use std::collections::HashSet;
use std::sync::Arc;

use btleplug::api::{Central, CentralEvent, ScanFilter};
use btleplug::platform::Adapter;
use futures_util::StreamExt;
use log::info;
use tokio::task::JoinSet;

use crate::inner::adapter_manager::ADAPTER_MANAGER;
use crate::inner::configuration::CONFIGURATION_MANAGER;
use crate::inner::dto::PeripheralKey;
use crate::inner::error::CollectorError;

pub(crate) struct PeripheralManager {
    pub(crate) adapter: Arc<Adapter>,
    pub(crate) connected_devices: HashSet<PeripheralKey>,
    // discovery_events: kanal::AsyncReceiver<CentralEvent>
}


impl PeripheralManager {
    pub(crate) fn new(adapter: Adapter) -> Self {
        Self {
            adapter: Arc::new(adapter),
            connected_devices: HashSet::new(),
        }
    }

    pub(crate) fn is_connected(&self, peripheral_id: &PeripheralKey) -> bool {
        self.connected_devices.contains(peripheral_id)
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
    while let Some(event) = receiver.stream().next().await {
        match event {
            CentralEvent::DeviceDiscovered(peripheral_id) |
            CentralEvent::DeviceUpdated(peripheral_id) |
            CentralEvent::DeviceConnected(peripheral_id) |
            CentralEvent::ManufacturerDataAdvertisement { id: peripheral_id, .. } |
            CentralEvent::ServiceDataAdvertisement { id: peripheral_id, .. } |
            CentralEvent::ServicesAdvertisement { id: peripheral_id, .. } => {
                let peripheral_key = PeripheralKey::try_from(peripheral_id)?;
                if ADAPTER_MANAGER.is_connected(&peripheral_key).await {
                    continue;
                }
                if CONFIGURATION_MANAGER.has_rules(&peripheral_key).await {
                    continue;
                }

                info!("Peripheral: {:?}", peripheral_key);
            }
            CentralEvent::DeviceDisconnected(peripheral_id) => {}
        }
    }

    Err(CollectorError::EndOfStream)
}
