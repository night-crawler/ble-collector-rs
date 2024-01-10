use std::collections::HashMap;
use std::sync::Arc;
use std::time::{Duration, Instant};

use anyhow::Context;
use btleplug::api::{BDAddr, Characteristic, Peripheral as _};
use btleplug::platform::{Adapter, Peripheral};
use retainer::Cache;
use tokio::sync::Mutex;
use tokio::task::JoinHandle;
use tracing::{info, Span};

use crate::inner::conf::cmd_args::AppConf;
use crate::inner::conf::manager::ConfigurationManager;
use crate::inner::conf::model::characteristic_config::CharacteristicConfig;
use crate::inner::error::CollectorResult;
use crate::inner::key_lock::KeyLock;
use crate::inner::model::adapter_info::AdapterInfo;
use crate::inner::model::collector_event::CollectorEvent;
use crate::inner::model::fqcn::Fqcn;
use crate::inner::publish::FanOutSender;

mod connection;
mod connection_context;
mod discovery;
mod ext;
pub mod util;

pub(crate) struct PeripheralManager {
    pub(crate) adapter: Arc<Adapter>,
    peripheral_cache: Arc<Cache<BDAddr, Arc<Peripheral>>>,
    peripheral_cache_updated_at: Mutex<Instant>,
    cache_monitor: JoinHandle<()>,
    poll_handle_map: Mutex<HashMap<Arc<Fqcn>, JoinHandle<()>>>,
    subscription_map: Mutex<HashMap<BDAddr, JoinHandle<()>>>,
    subscribed_characteristics: Mutex<HashMap<Arc<Fqcn>, Arc<CharacteristicConfig>>>,
    fanout_sender: Arc<FanOutSender<CollectorEvent>>,
    configuration_manager: Arc<ConfigurationManager>,
    pub(crate) app_conf: Arc<AppConf>,
    span: Span,
    connection_lock: KeyLock<BDAddr>,
    adapter_info: Arc<AdapterInfo>,
}

impl Drop for PeripheralManager {
    fn drop(&mut self) {
        self.cache_monitor.abort();
    }
}

impl PeripheralManager {
    pub(crate) fn new(
        adapter: Adapter,
        fanout_sender: Arc<FanOutSender<CollectorEvent>>,
        configuration_manager: Arc<ConfigurationManager>,
        app_conf: Arc<AppConf>,
        span: Span,
        adapter_info: AdapterInfo,
    ) -> Self {
        let cache = Arc::new(Cache::new());
        let clone = cache.clone();

        let monitor = tokio::spawn(async move { clone.monitor(10, 0.25, Duration::from_secs(10)).await });

        Self {
            adapter: Arc::new(adapter),
            peripheral_cache_updated_at: Mutex::new(Instant::now() - app_conf.peripheral_cache_ttl),
            peripheral_cache: cache,
            cache_monitor: monitor,
            poll_handle_map: Default::default(),
            subscription_map: Default::default(),
            subscribed_characteristics: Default::default(),
            fanout_sender,
            configuration_manager,
            app_conf,
            span,
            connection_lock: Default::default(),
            adapter_info: adapter_info.into(),
        }
    }
}

impl PeripheralManager {
    pub(crate) async fn get_peripheral_characteristic(
        &self,
        fqcn: &Fqcn,
    ) -> CollectorResult<(Arc<Peripheral>, Characteristic)> {
        let peripheral = self
            .get_peripheral(&fqcn.peripheral)
            .await?
            .context("Failed to get peripheral".to_string())?;

        self.connect(&peripheral).await?;

        let service = peripheral
            .services()
            .into_iter()
            .find(|service| service.uuid == fqcn.service)
            .context("Failed to find service".to_string())?;

        let characteristic = service
            .characteristics
            .into_iter()
            .find(|characteristic| characteristic.uuid == fqcn.characteristic)
            .context("Failed to find characteristic".to_string())?;

        Ok((peripheral, characteristic))
    }

    pub(crate) async fn disconnect_if_has_no_tasks(&self, peripheral: Arc<Peripheral>) -> CollectorResult<()> {
        let poll_handle_map = self.poll_handle_map.lock().await;
        let subscription_map = self.subscription_map.lock().await;
        let peripheral_address = peripheral.address();
        if subscription_map.contains_key(&peripheral_address) {
            return Ok(());
        }
        if poll_handle_map.keys().any(|fqcn| fqcn.peripheral == peripheral_address) {
            return Ok(());
        }

        info!("Disconnecting from {}", peripheral_address);

        peripheral.disconnect().await?;

        Ok(())
    }
}
