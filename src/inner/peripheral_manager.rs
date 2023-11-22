use std::collections::HashSet;
use std::sync::Arc;
use std::time::Duration;

use btleplug::api::{BDAddr, Central, CentralEvent, Peripheral as _, ScanFilter};
use btleplug::platform::{Adapter, Peripheral};
use futures_util::StreamExt;
use log::{debug, info};
use retainer::Cache;
use tokio::task::{JoinHandle, JoinSet};

use crate::inner::configuration::CONFIGURATION_MANAGER;
use crate::inner::dto::PeripheralKey;
use crate::inner::error::CollectorError;

pub(crate) struct PeripheralManager {
    pub(crate) adapter: Arc<Adapter>,
    pub(crate) connected_devices: HashSet<PeripheralKey>,
    pub(crate) peripheral_cache: Arc<Cache<BDAddr, Arc<Peripheral>>>,
    pub(crate) cache_monitor: JoinHandle<()>,
    // discovery_events: kanal::AsyncReceiver<CentralEvent>
}

impl Drop for PeripheralManager {
    fn drop(&mut self) {
        self.cache_monitor.abort();
    }
}

impl PeripheralManager {
    pub(crate) fn new(adapter: Adapter) -> Self {
        let cache = Arc::new(Cache::new());
        let clone = cache.clone();

        let monitor =
            tokio::spawn(async move { clone.monitor(10, 0.25, Duration::from_secs(10)).await });

        Self {
            adapter: Arc::new(adapter),
            connected_devices: HashSet::new(),
            peripheral_cache: cache,
            cache_monitor: monitor,
        }
    }

    async fn get_cached_peripheral(&self, address: &BDAddr) -> Option<Arc<Peripheral>> {
        if let Some(p) = self.peripheral_cache.get(address).await {
            return Some(Arc::clone(&p));
        }
        None
    }
    pub(crate) async fn get_peripheral(
        &self,
        address: &BDAddr,
    ) -> btleplug::Result<Option<Arc<Peripheral>>> {
        match self.get_cached_peripheral(address).await {
            None => self.populate_cache().await?,
            existing => return Ok(existing),
        }
        Ok(self.get_cached_peripheral(&address).await)
    }

    pub(crate) async fn populate_cache(&self) -> btleplug::Result<()> {
        for peripheral in self.adapter.peripherals().await? {
            peripheral.discover_services().await?;
            self.peripheral_cache
                .insert(
                    peripheral.address(),
                    Arc::new(peripheral),
                    Duration::from_secs(60),
                )
                .await;
        }

        Ok(())
    }

    pub(crate) fn is_connected(&self, peripheral_key: &PeripheralKey) -> bool {
        self.connected_devices.contains(peripheral_key)
    }
    pub(crate) async fn start_discovery(self: Arc<Self>) -> btleplug::Result<()> {
        self.adapter.start_scan(ScanFilter::default()).await?;

        let mut join_set = JoinSet::new();
        join_set.spawn(async move { discover_task(self).await });

        if let Some(result) = join_set.join_next().await {
            info!("Ending everything: {result:?}");
        }

        Ok(())
    }
}

async fn discover_task(peripheral_manager: Arc<PeripheralManager>) -> Result<(), CollectorError> {
    let mut stream = peripheral_manager.adapter.events().await?;
    while let Some(event) = stream.next().await {
        match event {
            CentralEvent::DeviceDiscovered(peripheral_id)
            | CentralEvent::DeviceUpdated(peripheral_id)
            | CentralEvent::DeviceConnected(peripheral_id)
            | CentralEvent::ManufacturerDataAdvertisement {
                id: peripheral_id, ..
            }
            | CentralEvent::ServiceDataAdvertisement {
                id: peripheral_id, ..
            }
            | CentralEvent::ServicesAdvertisement {
                id: peripheral_id, ..
            } => {
                let mut peripheral_key = PeripheralKey::try_from(peripheral_id)?;
                if let Some(peripheral) = peripheral_manager
                    .get_peripheral(&peripheral_key.peripheral_address)
                    .await?
                {
                    if let Some(props) = peripheral.properties().await? {
                        peripheral_key.name = props.local_name;
                    }
                }
                debug!("Peripheral: {:?}", peripheral_key);

                if peripheral_manager.is_connected(&peripheral_key) {
                    continue;
                }
                if let Some(config) = CONFIGURATION_MANAGER.get_matching_config(&peripheral_key).await {
                    info!("Found matching configuration: {:?}", config);
                }
            }
            CentralEvent::DeviceDisconnected(peripheral_id) => {}
        }
    }
    Err(CollectorError::EndOfStream)
}
