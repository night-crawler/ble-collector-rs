use std::sync::Arc;

use crate::inner::conf::model::characteristic_config::CharacteristicConfig;
use btleplug::api::{BDAddr, Central, Peripheral as _};
use btleplug::platform::{Peripheral, PeripheralId};
use futures_util::{stream, StreamExt};
use tracing::info;

use crate::inner::error::{CollectorError, CollectorResult};
use crate::inner::metrics::{Measure, SERVICE_DISCOVERY_DURATION};
use crate::inner::model::connected_peripherals::ConnectedPeripherals;
use crate::inner::model::fqcn::Fqcn;
use crate::inner::model::peripheral_key::PeripheralKey;
use crate::inner::peripheral_manager::PeripheralManager;

impl PeripheralManager {
    #[tracing::instrument(level = "info", skip(self))]
    pub(crate) async fn get_peripheral(&self, peripheral: &BDAddr) -> CollectorResult<Option<Arc<Peripheral>>> {
        match self.get_cached_peripheral(peripheral).await {
            None => self.populate_cache().await?,
            existing => return Ok(existing),
        }
        Ok(self.get_cached_peripheral(peripheral).await)
    }
    pub(super) async fn get_cached_peripheral(&self, address: &BDAddr) -> Option<Arc<Peripheral>> {
        if let Some(p) = self.peripheral_cache.get(address).await {
            return Some(Arc::clone(&p));
        }
        None
    }

    #[tracing::instrument(level = "info", skip(self), err)]
    pub(crate) async fn populate_cache(&self) -> CollectorResult<()> {
        let mut update_at = self.peripheral_cache_updated_at.lock().await;
        if update_at.elapsed() < self.app_conf.peripheral_cache_ttl {
            return Ok(());
        }
        *update_at = std::time::Instant::now();

        info!("Cache miss");

        let caching_results = stream::iter(self.adapter.peripherals().await?)
            .map(|peripheral| async {
                self.discover_services(&peripheral).await?;
                self.peripheral_cache
                    .insert(
                        peripheral.address(),
                        Arc::new(peripheral),
                        self.app_conf.peripheral_cache_ttl,
                    )
                    .await;

                Ok::<(), CollectorError>(())
            })
            .buffer_unordered(self.app_conf.service_discovery_parallelism)
            .collect::<Vec<_>>()
            .await;

        for result in caching_results {
            result?;
        }

        info!("Cache updated");
        Ok(())
    }

    #[tracing::instrument(level = "info", skip_all, err)]
    pub(super) async fn discover_services(&self, peripheral: &Peripheral) -> CollectorResult<()> {
        peripheral
            .discover_services()
            .measure_execution_time(SERVICE_DISCOVERY_DURATION)
            .await?;
        Ok(())
    }

    pub(super) async fn build_peripheral_key(&self, peripheral_id: &PeripheralId) -> CollectorResult<PeripheralKey> {
        let mut peripheral_key = PeripheralKey::try_from(peripheral_id)?;
        if let Some(peripheral) = self.get_peripheral(&peripheral_key.peripheral_address).await? {
            if let Some(props) = peripheral.properties().await? {
                peripheral_key.name = props.local_name;
            }
        }

        Ok(peripheral_key)
    }

    pub(crate) async fn get_all_connected_peripherals(&self) -> ConnectedPeripherals {
        let poll_handle_map = self.poll_handle_map.lock().await;
        let subscription_map = self.subscription_map.lock().await;
        let subscribed_characteristic = self.subscribed_characteristics.lock().await;

        ConnectedPeripherals::new(
            poll_handle_map.keys().map(|fqcn| fqcn.peripheral),
            subscription_map.keys().cloned(),
            subscribed_characteristic.keys().map(|fqcn| fqcn.peripheral),
        )
    }

    pub(super) async fn get_characteristic_conf(&self, fqcn: &Fqcn) -> Option<Arc<CharacteristicConfig>> {
        self.subscribed_characteristics.lock().await.get(fqcn).cloned()
    }
}
