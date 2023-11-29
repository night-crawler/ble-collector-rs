use std::collections::HashMap;
use std::fmt::{Display, Formatter};
use std::sync::Arc;
use std::time::{Duration, Instant};

use anyhow::Context;
use btleplug::api::{BDAddr, Central, CentralEvent, Characteristic, Peripheral as _, ScanFilter};
use btleplug::platform::{Adapter, Peripheral};
use futures_util::StreamExt;
use kanal::AsyncSender;
use log::{info, warn};
use retainer::Cache;
use tokio::sync::Mutex;
use tokio::task::{JoinHandle, JoinSet};
use uuid::Uuid;

use crate::inner::conf::flat::{FlatPeripheralConfig, ServiceCharacteristicKey};
use crate::inner::conf::manager::ConfigurationManager;
use crate::inner::conf::parse::CharacteristicConfig;
use crate::inner::conv::converter::CharacteristicValue;
use crate::inner::dto::PeripheralKey;
use crate::inner::error::{CollectorError, CollectorResult};

#[derive(Ord, PartialOrd, Eq, PartialEq, Debug, Clone, Hash)]
pub(crate) struct TaskKey {
    pub(crate) address: BDAddr,
    pub(crate) service_uuid: Uuid,
    pub(crate) characteristic_uuid: Uuid,
}

impl Display for TaskKey {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "{}::{}:{}",
            self.address, self.service_uuid, self.characteristic_uuid
        )
    }
}

#[derive(Debug, Clone)]
pub(crate) struct CharacteristicPayload {
    pub(crate) created_at: Instant,
    pub(crate) value: CharacteristicValue,
    pub(crate) task_key: Arc<TaskKey>,
    pub(crate) conf: Arc<CharacteristicConfig>,
}

impl Display for CharacteristicPayload {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}: {}", self.task_key, self.value)
    }
}

pub(crate) struct PeripheralManager {
    pub(crate) adapter: Arc<Adapter>,
    peripheral_cache: Arc<Cache<BDAddr, Arc<Peripheral>>>,
    cache_monitor: JoinHandle<()>,
    handle_map: Arc<Mutex<HashMap<Arc<TaskKey>, JoinHandle<()>>>>,
    subscription_map: Arc<Mutex<HashMap<BDAddr, JoinHandle<()>>>>,
    subscribed_characteristics: Arc<Mutex<HashMap<Arc<TaskKey>, Arc<CharacteristicConfig>>>>,
    payload_sender: AsyncSender<CharacteristicPayload>,
    configuration_manager: Arc<ConfigurationManager>,
}

struct ConnectionContext {
    peripheral: Arc<Peripheral>,
    characteristic: Characteristic,
    characteristic_config: Arc<CharacteristicConfig>,
    task_key: Arc<TaskKey>,
    peripheral_config: Arc<FlatPeripheralConfig>,
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
    ) -> Self {
        let cache = Arc::new(Cache::new());
        let clone = cache.clone();

        let monitor =
            tokio::spawn(async move { clone.monitor(10, 0.25, Duration::from_secs(10)).await });

        Self {
            adapter: Arc::new(adapter),
            peripheral_cache: cache,
            cache_monitor: monitor,
            handle_map: Default::default(),
            subscription_map: Default::default(),
            subscribed_characteristics: Default::default(),
            payload_sender: sender,
            configuration_manager,
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
                    Duration::from_secs(60),
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
            join_set.spawn(async move { discover_task(self_clone).await });
        }
        // {
        //     let self_clone = Arc::clone(&self);
        //     join_set.spawn(async move { process_characteristic_payload(self_clone).await });
        // }

        if let Some(result) = join_set.join_next().await {
            info!("Ending everything: {result:?}");
        }

        Ok(())
    }

    pub(crate) async fn connect_all_matching(
        self: Arc<Self>,
        peripheral_key: PeripheralKey,
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

            let task_key = TaskKey {
                address: peripheral_key.peripheral_address,
                service_uuid: characteristic.service_uuid,
                characteristic_uuid: characteristic.uuid,
            };

            if self.check_characteristic_is_handled(&task_key).await {
                continue;
            };

            let ctx = ConnectionContext {
                peripheral: Arc::clone(&peripheral),
                characteristic,
                characteristic_config: conf,
                task_key: Arc::new(task_key),
                peripheral_config: Arc::clone(&peripheral_config),
            };

            self.clone().handle_connect(ctx).await?;
        }

        Ok(())
    }

    async fn check_characteristic_is_handled(&self, task_key: &TaskKey) -> bool {
        self.handle_map.lock().await.get(task_key).is_some()
            || self
                .subscribed_characteristics
                .lock()
                .await
                .get(task_key)
                .is_some()
    }

    async fn handle_connect(self: Arc<Self>, ctx: ConnectionContext) -> CollectorResult<()> {
        let tk = ctx.task_key.clone();
        let self_clone = Arc::clone(&self);

        match ctx.characteristic_config.as_ref() {
            CharacteristicConfig::Subscribe { .. } => {
                self.subscribe(&ctx).await?;
                self.subscription_map
                    .lock()
                    .await
                    .entry(ctx.task_key.address)
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
                self.handle_map
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

    async fn abort_subscription(&self, tk: Arc<TaskKey>) {
        let mut subscribed_characteristics = self.subscribed_characteristics.lock().await;
        subscribed_characteristics.retain(|present_tk, _| present_tk.address != tk.address);

        if let Some(handle) = self.subscription_map.lock().await.remove(&tk.address) {
            handle.abort();
        } else {
            warn!("Can't abort for device {}: no handle found", tk);
        }
    }

    async fn abort_polling(&self, tk: Arc<TaskKey>) {
        if let Some(handle) = self.handle_map.lock().await.remove(&tk) {
            handle.abort();
        } else {
            warn!("Can't abort for device {}: no handle found", tk);
        }
    }

    async fn get_characteristic_conf(
        &self,
        ctx: &ConnectionContext,
        characteristic_uuid: Uuid,
    ) -> Option<Arc<CharacteristicConfig>> {
        let task_key = TaskKey {
            address: ctx.task_key.address,
            service_uuid: ctx.task_key.service_uuid,
            characteristic_uuid,
        };
        self.subscribed_characteristics
            .lock()
            .await
            .get(&task_key)
            .cloned()
    }

    async fn subscribe(&self, ctx: &ConnectionContext) -> CollectorResult<()> {
        let mut subscribed_characteristics = self.subscribed_characteristics.lock().await;

        subscribed_characteristics
            .entry(ctx.task_key.clone())
            .or_insert(ctx.characteristic_config.clone());

        ctx.peripheral.subscribe(&ctx.characteristic).await?;
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

        let delay_sec = delay_sec.with_context(|| {
            format!(
                "Delay was not updated for characteristic conf {:?}",
                ctx.characteristic_config
            )
        })?;

        loop {
            let value = ctx.peripheral.read(&ctx.characteristic).await?;
            let value = converter.convert(value)?;
            let value = CharacteristicPayload {
                created_at: Instant::now(),
                value,
                task_key: ctx.task_key.clone(),
                conf: Arc::clone(&ctx.characteristic_config),
            };
            self.payload_sender.send(value).await?;
            tokio::time::sleep(delay_sec).await;
        }
    }

    async fn block_on_notifying(self: Arc<Self>, ctx: ConnectionContext) -> CollectorResult<()> {
        info!("Subscribing to {ctx}");
        let mut notification_stream = ctx.peripheral.notifications().await?;

        while let Some(event) = notification_stream.next().await {
            let Some(conf) = self.get_characteristic_conf(&ctx, event.uuid).await else {
                continue;
            };
            let CharacteristicConfig::Subscribe { converter, .. } = conf.as_ref() else {
                return Err(CollectorError::UnexpectedCharacteristicConfiguration(conf));
            };

            let value = converter.convert(event.value)?;
            let value = CharacteristicPayload {
                created_at: Instant::now(),
                value,
                task_key: ctx.task_key.clone(),
                conf: Arc::clone(&ctx.characteristic_config),
            };
            self.payload_sender.send(value).await?;
        }

        Err(CollectorError::EndOfStream)
    }
}

// async fn process_characteristic_payload(
//     peripheral_manager: Arc<PeripheralManager>,
// ) -> CollectorResult<()> {
//     let receiver = peripheral_manager.payload_receiver.clone();
//     let mut stream = receiver.stream();
//     while let Some(characteristic_payload) = stream.next().await {
//         // CONFIGURATION_MANAGER.get_matching_config()
//         info!("Handled {}", characteristic_payload);
//     }
//
//     Err(CollectorError::EndOfStream)
// }

async fn discover_task(peripheral_manager: Arc<PeripheralManager>) -> CollectorResult<()> {
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
                    .clone()
                    .get_peripheral(&peripheral_key.peripheral_address)
                    .await?
                {
                    if let Some(props) = peripheral.properties().await? {
                        peripheral_key.name = props.local_name;
                    }
                }
                // debug!("Peripheral: {:?}", peripheral_key);

                // if peripheral_manager.is_connected(&peripheral_key).await {
                //     continue;
                // }
                if let Some(config) = peripheral_manager
                    .configuration_manager
                    .get_matching_config(&peripheral_key)
                    .await
                {
                    // info!("Found matching configuration: {:?}", config);
                    peripheral_manager
                        .clone()
                        .connect_all_matching(peripheral_key, config)
                        .await?;
                }
            }
            CentralEvent::DeviceDisconnected(_) => {}
        }
    }
    Err(CollectorError::EndOfStream)
}
