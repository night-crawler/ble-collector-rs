use std::collections::HashMap;
use std::fmt::{Display, Formatter};
use std::sync::Arc;
use std::time::{Duration, Instant};

use anyhow::Context;
use btleplug::api::{BDAddr, Central, CentralEvent, Characteristic, Peripheral as _, ScanFilter};
use btleplug::platform::{Adapter, Peripheral};
use futures_util::StreamExt;
use kanal::{AsyncReceiver, AsyncSender};
use log::{info, warn};
use retainer::Cache;
use tokio::sync::Mutex;
use tokio::task::{JoinHandle, JoinSet};
use uuid::Uuid;

use crate::inner::conf::flat::{FlatPeripheralConfig, ServiceCharacteristicKey};
use crate::inner::conf::manager::CONFIGURATION_MANAGER;
use crate::inner::conf::parse::CharacteristicConfigDto;
use crate::inner::conv::converter::CharacteristicValue;
use crate::inner::dto::PeripheralKey;
use crate::inner::error::{CollectorError, CollectorResult};

#[derive(Ord, PartialOrd, Eq, PartialEq, Debug, Clone, Hash)]
pub(crate) struct TaskKey {
    address: BDAddr,
    service_uuid: Uuid,
    characteristic_uuid: Uuid,
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
    subscribed_characteristics: Arc<Mutex<HashMap<Arc<TaskKey>, Arc<CharacteristicConfigDto>>>>,
    payload_sender: AsyncSender<CharacteristicPayload>,
    payload_receiver: AsyncReceiver<CharacteristicPayload>,
}

struct ConnectionBundle {
    peripheral: Arc<Peripheral>,
    characteristic: Characteristic,
    characteristic_config: Arc<CharacteristicConfigDto>,
    task_key: Arc<TaskKey>,
    subscribed_characteristics: Arc<Mutex<HashMap<Arc<TaskKey>, Arc<CharacteristicConfigDto>>>>,
    peripheral_config: Arc<FlatPeripheralConfig>,
}

impl ConnectionBundle {
    async fn get_conf(&self, characteristic_uuid: Uuid) -> Option<Arc<CharacteristicConfigDto>> {
        let task_key = TaskKey {
            address: self.task_key.address,
            service_uuid: self.task_key.service_uuid,
            characteristic_uuid,
        };
        self.subscribed_characteristics
            .lock()
            .await
            .get(&task_key)
            .cloned()
    }
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

        let (sender, receiver) = kanal::unbounded_async();

        Self {
            adapter: Arc::new(adapter),
            peripheral_cache: cache,
            cache_monitor: monitor,
            handle_map: Default::default(),
            subscription_map: Default::default(),
            subscribed_characteristics: Default::default(),
            payload_sender: sender,
            payload_receiver: receiver,
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
        Ok(self.get_cached_peripheral(address).await)
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

    pub(crate) async fn is_connected(&self, peripheral_key: &PeripheralKey) -> bool {
        let Ok(Some(peripheral)) = self
            .get_peripheral(&peripheral_key.peripheral_address)
            .await
        else {
            return false;
        };
        let Ok(is_connected) = peripheral.is_connected().await else {
            return false;
        };
        is_connected
    }
    pub(crate) async fn start_discovery(self: Arc<Self>) -> btleplug::Result<()> {
        self.adapter.start_scan(ScanFilter::default()).await?;

        let mut join_set = JoinSet::new();
        {
            let self_clone = Arc::clone(&self);
            join_set.spawn(async move { discover_task(self_clone).await });
        }
        {
            let self_clone = Arc::clone(&self);
            join_set.spawn(async move { process_characteristic_payload(self_clone).await });
        }

        if let Some(result) = join_set.join_next().await {
            info!("Ending everything: {result:?}");
        }

        Ok(())
    }

    pub(crate) async fn connect_all_matching(
        &self,
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

            let connection_bundle = ConnectionBundle {
                peripheral: Arc::clone(&peripheral),
                characteristic,
                characteristic_config: conf,
                task_key: Arc::new(task_key),
                subscribed_characteristics: Arc::clone(&self.subscribed_characteristics),
                peripheral_config: Arc::clone(&peripheral_config),
            };

            self.handle_connect(connection_bundle).await?;
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

    async fn handle_connect(&self, connection_bundle: ConnectionBundle) -> CollectorResult<()> {
        let sender = self.payload_sender.clone();
        let tk = connection_bundle.task_key.clone();

        match connection_bundle.characteristic_config.as_ref() {
            CharacteristicConfigDto::Subscribe { .. } => {
                self.subscribed_characteristics
                    .lock()
                    .await
                    .entry(tk.clone())
                    .or_insert(connection_bundle.characteristic_config.clone());
                connection_bundle
                    .peripheral
                    .subscribe(&connection_bundle.characteristic)
                    .await?;

                let subscribed_characteristics = Arc::clone(&self.subscribed_characteristics);

                let mut notification_handles_map = self.subscription_map.lock().await;

                notification_handles_map
                    .entry(connection_bundle.task_key.address)
                    .or_insert_with(|| {
                        let subscription_map_clone = self.subscription_map.clone();
                        tokio::spawn(async move {
                            let result = handle_notification_based(connection_bundle, sender).await;
                            warn!("Ended subscription {}: {:?}", tk, result);
                            subscribed_characteristics.lock().await.remove(&tk);

                            if let Some(handle) =
                                subscription_map_clone.lock().await.remove(&tk.address)
                            {
                                handle.abort();
                            }
                        })
                    });
            }
            CharacteristicConfigDto::Poll { .. } => {
                let mut handle_map = self.handle_map.lock().await;

                handle_map.entry(tk.clone()).or_insert_with(|| {
                    let handle_map_clone = Arc::clone(&self.handle_map);

                    tokio::spawn(async move {
                        let res = handle_poll_based(connection_bundle, sender).await;
                        warn!("Ended polling {}: {:?}", tk, res);
                        if let Some(handle) = handle_map_clone.lock().await.remove(&tk) {
                            handle.abort()
                        }
                    })
                });
            }
        }
        Ok(())
    }
}

async fn handle_notification_based(
    connection_bundle: ConnectionBundle,
    sender: AsyncSender<CharacteristicPayload>,
) -> CollectorResult<()> {
    info!("Subscribing to `{}` / {}", connection_bundle.peripheral_config.name, connection_bundle.task_key);
    let mut notification_stream = connection_bundle.peripheral.notifications().await?;

    while let Some(event) = notification_stream.next().await {
        let Some(conf) = connection_bundle.get_conf(event.uuid).await else {
            continue;
        };
        let CharacteristicConfigDto::Subscribe { converter, .. } = conf.as_ref() else {
            return Err(CollectorError::UnexpectedCharacteristicConfiguration(conf));
        };

        let value = converter.convert(event.value)?;
        let value = CharacteristicPayload {
            created_at: Instant::now(),
            value,
            task_key: connection_bundle.task_key.clone(),
        };
        sender.send(value).await?;
    }

    Err(CollectorError::EndOfStream)
}

async fn handle_poll_based(
    connection_bundle: ConnectionBundle,
    sender: AsyncSender<CharacteristicPayload>,
) -> CollectorResult<()> {
    let CharacteristicConfigDto::Poll {
        delay_sec,
        ref converter,
        ..
    } = connection_bundle.characteristic_config.as_ref()
    else {
        return Err(CollectorError::UnexpectedCharacteristicConfiguration(
            connection_bundle.characteristic_config.clone(),
        ));
    };

    let delay_sec = delay_sec.with_context(|| {
        format!(
            "Delay was not updated for characteristic conf {:?}",
            connection_bundle.characteristic_config
        )
    })?;

    loop {
        let value = connection_bundle
            .peripheral
            .read(&connection_bundle.characteristic)
            .await?;
        let value = converter.convert(value)?;

        let value = CharacteristicPayload {
            created_at: Instant::now(),
            value,
            task_key: connection_bundle.task_key.clone(),
        };
        sender.send(value).await?;
        tokio::time::sleep(delay_sec).await;
    }
}

async fn process_characteristic_payload(
    peripheral_manager: Arc<PeripheralManager>,
) -> CollectorResult<()> {
    let receiver = peripheral_manager.payload_receiver.clone();
    let mut stream = receiver.stream();
    while let Some(characteristic_payload) = stream.next().await {
        // CONFIGURATION_MANAGER.get_matching_config()
        info!("Handled {}", characteristic_payload);
    }

    Err(CollectorError::EndOfStream)
}

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
                if let Some(config) = CONFIGURATION_MANAGER
                    .get_matching_config(&peripheral_key)
                    .await
                {
                    // info!("Found matching configuration: {:?}", config);
                    peripheral_manager
                        .connect_all_matching(peripheral_key, config)
                        .await?;
                }
            }
            CentralEvent::DeviceDisconnected(peripheral_id) => {}
        }
    }
    Err(CollectorError::EndOfStream)
}
