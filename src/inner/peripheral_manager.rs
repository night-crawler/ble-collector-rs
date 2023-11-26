use std::collections::HashMap;
use std::fmt::{Display, Formatter};
use std::sync::Arc;
use std::time::Duration;

use anyhow::Context;
use btleplug::api::{BDAddr, Central, CentralEvent, Characteristic, Peripheral as _, ScanFilter};
use btleplug::platform::{Adapter, Peripheral};
use futures_util::StreamExt;
use log::{error, info, warn};
use retainer::Cache;
use tokio::sync::Mutex;
use tokio::task::{JoinHandle, JoinSet};
use uuid::Uuid;

use crate::inner::conf::flat::{FlatPeripheralConfig, ServiceCharacteristicKey};
use crate::inner::conf::manager::CONFIGURATION_MANAGER;
use crate::inner::conf::parse::CharacteristicConfigDto;
use crate::inner::dto::PeripheralKey;
use crate::inner::error::{CollectorError, CollectorResult};

#[derive(Ord, PartialOrd, Eq, PartialEq, Debug, Clone, Hash)]
struct TaskKey {
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

pub(crate) struct PeripheralManager {
    pub(crate) adapter: Arc<Adapter>,
    pub(crate) peripheral_cache: Arc<Cache<BDAddr, Arc<Peripheral>>>,
    pub(crate) cache_monitor: JoinHandle<()>,
    pub(crate) handle_map: Arc<Mutex<HashMap<TaskKey, JoinHandle<()>>>>,
    pub(crate) subscription_map: Arc<Mutex<HashMap<BDAddr, JoinHandle<()>>>>,
    pub(crate) subscribe_characteristics: Arc<Mutex<HashMap<TaskKey, CharacteristicConfigDto>>>,
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
            peripheral_cache: cache,
            cache_monitor: monitor,
            handle_map: Default::default(),
            subscription_map: Default::default(),
            subscribe_characteristics: Default::default(),
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
        join_set.spawn(async move { discover_task(self).await });

        if let Some(result) = join_set.join_next().await {
            info!("Ending everything: {result:?}");
        }

        Ok(())
    }

    pub(crate) async fn connect(
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
                service_uuid: characteristic.uuid,
                characteristic_uuid: characteristic.uuid,
            };

            if self.handle_map.lock().await.get(&task_key).is_some()
                || self
                    .subscribe_characteristics
                    .lock()
                    .await
                    .get(&task_key)
                    .is_some()
            {
                continue;
            }

            match conf {
                CharacteristicConfigDto::Subscribe { .. } => {
                    self.subscribe_characteristics
                        .lock()
                        .await
                        .entry(task_key.clone())
                        .or_insert(conf.clone());
                    peripheral.subscribe(&characteristic).await?;
                    error!("Subscribed!");

                    let mut subscription_map = self.subscription_map.lock().await;

                    subscription_map
                        .entry(peripheral_key.peripheral_address)
                        .or_insert_with(|| {
                            let peripheral = Arc::clone(&peripheral);
                            let subscription_map_clone = self.subscription_map.clone();
                            let tk = task_key.clone();
                            tokio::spawn(async move {
                                let result = handle_notification_based(peripheral).await;
                                warn!("Ended subscription {}: {:?}", tk, result);
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

                    handle_map.entry(task_key.clone()).or_insert_with(|| {
                        let peripheral = Arc::clone(&peripheral);
                        let handle_map_clone = Arc::clone(&self.handle_map);
                        let tk = task_key.clone();

                        tokio::spawn(async move {
                            let res =
                                handle_poll_based(peripheral, characteristic, conf.clone()).await;
                            warn!("Ended polling {}: {:?}", tk, res);
                            if let Some(handle) = handle_map_clone.lock().await.remove(&tk) {
                                handle.abort()
                            }
                        })
                    });
                }
            }
        }

        Ok(())
    }
}

async fn handle_notification_based(peripheral: Arc<Peripheral>) -> CollectorResult<()> {
    let mut s = peripheral.notifications().await?;

    while let Some(event) = s.next().await {
        info!("Subscribed: {:?}", event);
    }

    Err(CollectorError::EndOfStream)
}

async fn handle_poll_based(
    peripheral: Arc<Peripheral>,
    characteristic: Characteristic,
    config: CharacteristicConfigDto,
) -> CollectorResult<()> {
    let CharacteristicConfigDto::Poll {
        history_size,
        name,
        uuid,
        converter,
        delay_sec
    } = config
    else {
        return Err(CollectorError::UnexpectedCharacteristicConfiguration(
            config,
        ));
    };
    loop {
        let result = peripheral.read(&characteristic).await?;
        info!("Read: {:?} from char {:?}", result, characteristic);
        tokio::time::sleep(Duration::from_secs(20)).await;
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
                // debug!("Peripheral: {:?}", peripheral_key);

                // if peripheral_manager.is_connected(&peripheral_key).await {
                //     continue;
                // }
                if let Some(config) = CONFIGURATION_MANAGER
                    .get_matching_config(&peripheral_key)
                    .await
                {
                    // info!("Found matching configuration: {:?}", config);
                    peripheral_manager.connect(peripheral_key, config).await?;
                }
            }
            CentralEvent::DeviceDisconnected(peripheral_id) => {}
        }
    }
    Err(CollectorError::EndOfStream)
}
