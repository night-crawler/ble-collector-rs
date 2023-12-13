use std::collections::HashMap;
use std::fmt::{Display, Formatter};
use std::sync::Arc;
use std::time::Duration;

use crate::inner::conf::cmd_args::AppConf;
use anyhow::Context;
use btleplug::api::{
    BDAddr, Central, CentralEvent, Characteristic, Peripheral as _, ScanFilter, ValueNotification,
};
use btleplug::platform::{Adapter, Peripheral};
use chrono::Utc;
use futures_util::StreamExt;
use kanal::AsyncSender;
use log::{debug, error, info, warn};
use retainer::Cache;
use rocket::serde::Serialize;
use serde::Deserialize;
use tokio::sync::Mutex;
use tokio::task::{JoinHandle, JoinSet};
use uuid::Uuid;

use crate::inner::conf::flat::{FlatPeripheralConfig, ServiceCharacteristicKey};
use crate::inner::conf::manager::ConfigurationManager;
use crate::inner::conf::parse::CharacteristicConfig;
use crate::inner::conv::converter::CharacteristicValue;
use crate::inner::error::{CollectorError, CollectorResult};
use crate::inner::model::peripheral_key::PeripheralKey;

#[derive(Ord, PartialOrd, Eq, PartialEq, Debug, Clone, Hash, Serialize, Deserialize)]
pub(crate) struct Fqcn {
    pub(crate) peripheral_address: BDAddr,
    pub(crate) service_uuid: Uuid,
    pub(crate) characteristic_uuid: Uuid,
}

impl Display for Fqcn {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "{}::{}:{}",
            self.peripheral_address, self.service_uuid, self.characteristic_uuid
        )
    }
}

#[derive(Debug, Clone)]
pub(crate) struct CharacteristicPayload {
    pub(crate) created_at: chrono::DateTime<Utc>,
    pub(crate) value: CharacteristicValue,
    pub(crate) fqcn: Arc<Fqcn>,
    pub(crate) conf: Arc<CharacteristicConfig>,
}

impl Display for CharacteristicPayload {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        let parts = vec![
            self.fqcn.peripheral_address.to_string(),
            if let Some(name) = self.conf.service_name() {
                name.to_string()
            } else {
                self.fqcn.service_uuid.to_string()
            },
            if let Some(name) = self.conf.name() {
                name.to_string()
            } else {
                self.fqcn.characteristic_uuid.to_string()
            },
        ];

        write!(f, "{}={}", parts.join(":"), self.value)
    }
}

pub(crate) struct PeripheralManager {
    pub(crate) adapter: Arc<Adapter>,
    peripheral_cache: Arc<Cache<BDAddr, Arc<Peripheral>>>,
    cache_monitor: JoinHandle<()>,
    poll_handle_map: Arc<Mutex<HashMap<Arc<Fqcn>, JoinHandle<()>>>>,
    subscription_map: Arc<Mutex<HashMap<BDAddr, JoinHandle<()>>>>,
    subscribed_characteristics: Arc<Mutex<HashMap<Arc<Fqcn>, Arc<CharacteristicConfig>>>>,
    payload_sender: AsyncSender<CharacteristicPayload>,
    configuration_manager: Arc<ConfigurationManager>,
    pub(crate) app_conf: Arc<AppConf>,
}

struct ConnectionContext {
    peripheral: Arc<Peripheral>,
    characteristic: Characteristic,
    characteristic_config: Arc<CharacteristicConfig>,
    fqcn: Arc<Fqcn>,
    peripheral_config: Arc<FlatPeripheralConfig>,
}

impl Fqcn {
    fn with_characteristic(&self, service_uuid: Uuid, characteristic_uuid: Uuid) -> Self {
        Fqcn {
            service_uuid,
            characteristic_uuid,
            ..*self
        }
    }

    pub(crate) fn matches(&self, value_notification: &ValueNotification) -> bool {
        self.service_uuid == value_notification.service_uuid
            && self.characteristic_uuid == value_notification.uuid
    }
}

impl Display for ConnectionContext {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        let name = self
            .characteristic_config
            .name()
            .unwrap_or(Arc::new("".to_string()));
        write!(
            f,
            "{}[{}]::{}/{name}[{}]",
            self.peripheral_config.name,
            self.peripheral.address(),
            self.characteristic.service_uuid,
            self.characteristic.uuid
        )
    }
}

impl Drop for PeripheralManager {
    fn drop(&mut self) {
        self.cache_monitor.abort();
    }
}

impl PeripheralManager {
    pub(crate) fn new(
        adapter: Adapter,
        sender: AsyncSender<CharacteristicPayload>,
        configuration_manager: Arc<ConfigurationManager>,
        app_conf: Arc<AppConf>,
    ) -> Self {
        let cache = Arc::new(Cache::new());
        let clone = cache.clone();

        let monitor =
            tokio::spawn(async move { clone.monitor(10, 0.25, Duration::from_secs(10)).await });

        Self {
            adapter: Arc::new(adapter),
            peripheral_cache: cache,
            cache_monitor: monitor,
            poll_handle_map: Default::default(),
            subscription_map: Default::default(),
            subscribed_characteristics: Default::default(),
            payload_sender: sender,
            configuration_manager,
            app_conf,
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
    ) -> CollectorResult<Option<Arc<Peripheral>>> {
        match self.get_cached_peripheral(address).await {
            None => self.populate_cache().await?,
            existing => return Ok(existing),
        }
        Ok(self.get_cached_peripheral(address).await)
    }

    pub(crate) async fn populate_cache(&self) -> CollectorResult<()> {
        for peripheral in self.adapter.peripherals().await? {
            peripheral.discover_services().await?;
            self.peripheral_cache
                .insert(
                    peripheral.address(),
                    Arc::new(peripheral),
                    self.app_conf.peripheral_cache_ttl,
                )
                .await;
        }

        Ok(())
    }

    pub(crate) async fn start_discovery(self: Arc<Self>) -> CollectorResult<()> {
        self.adapter.start_scan(ScanFilter::default()).await?;

        let mut join_set = JoinSet::new();
        {
            let self_clone = Arc::clone(&self);
            join_set.spawn(async move { self_clone.discover_task().await });
        }

        if let Some(result) = join_set.join_next().await {
            info!(
                "Discovery task for adapter {:?} has ended: {result:?}",
                self.adapter
            );
        }

        Ok(())
    }

    pub(crate) async fn connect_all_matching(
        self: Arc<Self>,
        peripheral_key: &PeripheralKey,
        peripheral_config: Arc<FlatPeripheralConfig>,
    ) -> CollectorResult<()> {
        let peripheral = self
            .get_peripheral(&peripheral_key.peripheral_address)
            .await?
            .with_context(|| format!("Failed to get peripheral: {:?}", peripheral_key))?;

        if !peripheral.is_connected().await? {
            peripheral.connect().await?;
        }
        peripheral.discover_services().await?;

        for characteristic in peripheral
            .services()
            .into_iter()
            .flat_map(|service| service.characteristics.into_iter())
        {
            let char_key = ServiceCharacteristicKey::from(&characteristic);
            let Some(conf) = peripheral_config.service_map.get(&char_key).cloned() else {
                continue;
            };

            let fqcn = Fqcn {
                peripheral_address: peripheral_key.peripheral_address,
                service_uuid: characteristic.service_uuid,
                characteristic_uuid: characteristic.uuid,
            };

            if self.check_characteristic_is_handled(&fqcn).await {
                continue;
            };

            let ctx = ConnectionContext {
                peripheral: Arc::clone(&peripheral),
                characteristic,
                characteristic_config: conf,
                fqcn: Arc::new(fqcn),
                peripheral_config: Arc::clone(&peripheral_config),
            };

            self.clone().handle_connect(ctx).await?;
        }

        Ok(())
    }

    async fn check_characteristic_is_handled(&self, fqcn: &Fqcn) -> bool {
        self.poll_handle_map.lock().await.get(fqcn).is_some()
            || self
                .subscribed_characteristics
                .lock()
                .await
                .get(fqcn)
                .is_some()
    }

    async fn handle_connect(self: Arc<Self>, ctx: ConnectionContext) -> CollectorResult<()> {
        let tk = ctx.fqcn.clone();
        let self_clone = Arc::clone(&self);

        match ctx.characteristic_config.as_ref() {
            CharacteristicConfig::Subscribe { .. } => {
                self.subscribe(&ctx).await?;
                self.subscription_map
                    .lock()
                    .await
                    .entry(ctx.fqcn.peripheral_address)
                    .or_insert_with(|| {
                        tokio::spawn(async move {
                            let msg = format!("Ended subscription {ctx}");
                            let res = self_clone.clone().block_on_notifying(ctx).await;
                            warn!("{msg}: {res:?}");
                            self_clone.abort_subscription(tk.clone()).await;
                        })
                    });
            }
            CharacteristicConfig::Poll { .. } => {
                self.poll_handle_map
                    .lock()
                    .await
                    .entry(tk.clone())
                    .or_insert_with(|| {
                        tokio::spawn(async move {
                            let msg = format!("Ended polling {ctx}");
                            let res = self_clone.clone().block_on_polling(ctx).await;
                            warn!("{msg}: {res:?}");
                            self_clone.abort_polling(tk.clone()).await;
                        })
                    });
            }
        }
        Ok(())
    }

    async fn abort_subscription(&self, tk: Arc<Fqcn>) {
        let mut subscribed_characteristics = self.subscribed_characteristics.lock().await;
        subscribed_characteristics
            .retain(|present_tk, _| present_tk.peripheral_address != tk.peripheral_address);

        if let Some(handle) = self
            .subscription_map
            .lock()
            .await
            .remove(&tk.peripheral_address)
        {
            handle.abort();
        } else {
            warn!("Can't abort for device {}: no handle found", tk);
        }
    }

    async fn abort_polling(&self, tk: Arc<Fqcn>) {
        if let Some(handle) = self.poll_handle_map.lock().await.remove(&tk) {
            handle.abort();
        } else {
            warn!("Can't abort for device {}: no handle found", tk);
        }
    }

    async fn get_characteristic_conf(&self, fqcn: &Fqcn) -> Option<Arc<CharacteristicConfig>> {
        self.subscribed_characteristics
            .lock()
            .await
            .get(fqcn)
            .cloned()
    }

    async fn subscribe(&self, ctx: &ConnectionContext) -> CollectorResult<()> {
        let mut subscribed_characteristics = self.subscribed_characteristics.lock().await;

        match subscribed_characteristics.entry(ctx.fqcn.clone()) {
            std::collections::hash_map::Entry::Occupied(_) => {
                warn!("Already subscribed to {ctx}");
                return Ok(());
            }
            std::collections::hash_map::Entry::Vacant(entry) => {
                entry.insert(ctx.characteristic_config.clone());
            }
        }

        ctx.peripheral.subscribe(&ctx.characteristic).await?;

        info!("Subscribed to {ctx}");

        Ok(())
    }
}

impl PeripheralManager {
    async fn block_on_polling(self: Arc<Self>, ctx: ConnectionContext) -> CollectorResult<()> {
        info!("Polling {ctx}");

        let CharacteristicConfig::Poll {
            delay_sec,
            ref converter,
            ..
        } = ctx.characteristic_config.as_ref()
        else {
            return Err(CollectorError::UnexpectedCharacteristicConfiguration(
                ctx.characteristic_config.clone(),
            ));
        };

        loop {
            let value = ctx.peripheral.read(&ctx.characteristic).await?;
            let value = converter.convert(value)?;
            let value = CharacteristicPayload {
                created_at: chrono::offset::Utc::now(),
                value,
                fqcn: ctx.fqcn.clone(),
                conf: Arc::clone(&ctx.characteristic_config),
            };
            self.payload_sender.send(value).await?;
            tokio::time::sleep(*delay_sec).await;
        }
    }

    async fn block_on_notifying(self: Arc<Self>, ctx: ConnectionContext) -> CollectorResult<()> {
        let mut notification_stream = ctx.peripheral.notifications().await?;

        while let Some(event) = notification_stream.next().await {
            let fqcn = Arc::new(ctx.fqcn.with_characteristic(event.service_uuid, event.uuid));
            let Some(conf) = self.get_characteristic_conf(&fqcn).await else {
                // warn!("No conf found for characteristic: {fqcn}; {:?}", ctx.peripheral);
                continue;
            };
            let CharacteristicConfig::Subscribe { converter, .. } = conf.as_ref() else {
                return Err(CollectorError::UnexpectedCharacteristicConfiguration(conf));
            };

            let value = converter.convert(event.value)?;
            let value = CharacteristicPayload {
                created_at: chrono::offset::Utc::now(),
                value,
                fqcn,
                conf,
            };
            self.payload_sender.send(value).await?;
        }

        Err(CollectorError::EndOfStream)
    }
}

impl PeripheralManager {
    pub(crate) async fn get_peripheral_characteristic(
        &self,
        fqcn: &Fqcn,
    ) -> CollectorResult<(Arc<Peripheral>, Characteristic)> {
        let peripheral = self
            .get_peripheral(&fqcn.peripheral_address)
            .await?
            .with_context(|| format!("Failed to get peripheral: {fqcn}"))?;

        if !peripheral.is_connected().await? {
            info!("Connecting to {fqcn}");
            peripheral.connect().await?;
        }

        peripheral.discover_services().await?;
        let service = peripheral
            .services()
            .into_iter()
            .find(|service| service.uuid == fqcn.service_uuid)
            .with_context(|| format!("Failed to find service {fqcn}"))?;

        let characteristic = service
            .characteristics
            .into_iter()
            .find(|characteristic| characteristic.uuid == fqcn.characteristic_uuid)
            .with_context(|| format!("Failed to find characteristic {fqcn}",))?;

        Ok((peripheral, characteristic))
    }

    pub(crate) async fn disconnect_if_has_no_tasks(
        &self,
        peripheral: Arc<Peripheral>,
    ) -> CollectorResult<()> {
        let poll_handle_map = self.poll_handle_map.lock().await;
        let subscription_map = self.subscription_map.lock().await;
        let peripheral_address = peripheral.address();
        if subscription_map.contains_key(&peripheral_address) {
            return Ok(());
        }
        if poll_handle_map
            .keys()
            .any(|fqcn| fqcn.peripheral_address == peripheral_address)
        {
            return Ok(());
        }

        peripheral.disconnect().await?;

        Ok(())
    }
}

impl PeripheralManager {
    async fn discover_task(self: Arc<Self>) -> CollectorResult<()> {
        let throttle_map = Cache::new();
        let mut num_throttles = 0usize;

        let mut stream = self.adapter.events().await?;
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
                    if let Some(peripheral) = self
                        .clone()
                        .get_peripheral(&peripheral_key.peripheral_address)
                        .await?
                    {
                        if let Some(props) = peripheral.properties().await? {
                            peripheral_key.name = props.local_name;
                        }
                    }

                    let peripheral_key = Arc::new(peripheral_key);
                    throttle_map
                        .purge(
                            self.app_conf.event_throttling_purge_samples,
                            self.app_conf.event_throttling_purge_threshold,
                        )
                        .await;
                    if throttle_map
                        .insert(peripheral_key.clone(), true, self.app_conf.event_throttling)
                        .await
                        .is_some()
                    {
                        num_throttles += 1;
                        if num_throttles % 1000 == 0 {
                            debug!("Throttled {num_throttles} events");
                        }
                        continue;
                    };

                    if let Some(config) = self
                        .configuration_manager
                        .get_matching_config(&peripheral_key)
                        .await
                    {
                        let peripheral_manager = Arc::clone(&self);
                        tokio::spawn(async move {
                            if let Err(err) = peripheral_manager
                                .clone()
                                .connect_all_matching(&peripheral_key, config)
                                .await
                            {
                                error!("Error connecting to peripheral {peripheral_key}: {err:?}");
                            }
                        });
                    }
                }
                CentralEvent::DeviceDisconnected(_) => {}
            }
        }
        Err(CollectorError::EndOfStream)
    }
}
