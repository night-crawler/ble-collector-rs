use std::sync::Arc;

use crate::inner::conf::manager::ConfigurationManager;
use btleplug::api::{Central, Manager as _, Peripheral as _};
use btleplug::platform::{Adapter, Manager, Peripheral};
use futures_util::stream;
use futures_util::StreamExt;
use kanal::AsyncSender;
use log::{info, warn};
use tokio::sync::Mutex;
use tokio::task::JoinSet;

use crate::inner::dto::{AdapterDto, AdapterInfoDto, PeripheralDto};
use crate::inner::error::{CollectorError, CollectorResult};
use crate::inner::peripheral_manager::{CharacteristicPayload, PeripheralManager};

pub(crate) struct AdapterManager {
    peripheral_managers: Mutex<Vec<Arc<PeripheralManager>>>,
    sender: AsyncSender<CharacteristicPayload>,
    configuration_manager: Arc<ConfigurationManager>,
}

impl AdapterManager {
    pub(crate) fn new(
        configuration_manager: Arc<ConfigurationManager>,
        sender: AsyncSender<CharacteristicPayload>,
    ) -> Self {
        Self {
            peripheral_managers: Default::default(),
            sender,
            configuration_manager,
        }
    }
    pub(crate) async fn init(&self) -> CollectorResult<()> {
        let manager = Manager::new().await?;
        let adapters = manager.adapters().await?;
        for adapter in adapters {
            info!("Discovered adapter: {:?}", adapter);
            self.add_adapter(adapter).await;
        }
        Ok(())
    }

    async fn add_adapter(&self, adapter: Adapter) {
        self.peripheral_managers
            .lock()
            .await
            .push(Arc::new(PeripheralManager::new(
                adapter,
                self.sender.clone(),
                self.configuration_manager.clone(),
            )));
    }

    pub(crate) async fn get_peripheral_manager(
        &self,
        adapter_id: &str,
    ) -> CollectorResult<Option<Arc<PeripheralManager>>> {
        let managers = self.peripheral_managers.lock().await;

        for manager in managers.iter() {
            let adapter_info = manager.adapter.adapter_info().await?;
            let adapter_info = AdapterInfoDto::try_from(adapter_info)?;
            if adapter_info.id == adapter_id {
                return Ok(Some(Arc::clone(manager)));
            }
        }

        Ok(None)
    }

    pub(crate) async fn start_discovery(&self) -> CollectorResult<()> {
        let mut join_set = JoinSet::new();
        for peripheral_manager in self.peripheral_managers.lock().await.iter().cloned() {
            join_set.spawn(async move { peripheral_manager.start_discovery().await });
        }

        if let Some(result) = join_set.join_next().await {
            warn!("Peripheral discovery has ended: {result:?}");
        }
        // TODO: restart discovery and recreate everything if we've reached this point
        Ok(())
    }

    pub(crate) async fn list_adapters(&self) -> CollectorResult<Vec<AdapterInfoDto>> {
        let managers = self.peripheral_managers.lock().await;

        let infos = stream::iter(managers.iter())
            .map(Arc::clone)
            .map(|adapter_service_manager| async move {
                adapter_service_manager.adapter.adapter_info().await
            })
            .buffered(4)
            .collect::<Vec<_>>()
            .await;

        let mut adapters = vec![];

        for info in infos {
            let info = info?;
            let adapter_info = AdapterInfoDto::try_from(info)?;
            adapters.push(adapter_info);
        }

        Ok(adapters)
    }

    pub(crate) async fn describe_adapters(&self) -> CollectorResult<Vec<AdapterDto>> {
        let device_managers = self.peripheral_managers.lock().await;

        let peripherals_per_adapter = stream::iter(device_managers.iter())
            .map(Arc::clone)
            .map(|peripheral_manager| async move {
                let adapter_info = peripheral_manager.adapter.adapter_info().await?;
                let adapter_dto = AdapterDto::try_from(adapter_info)?;
                let peripherals = peripheral_manager.adapter.peripherals().await?;

                Ok::<(Arc<Mutex<AdapterDto>>, Vec<Peripheral>), CollectorError>((
                    Arc::new(Mutex::new(adapter_dto)),
                    peripherals,
                ))
            })
            .buffer_unordered(4)
            .collect::<Vec<_>>()
            .await
            .into_iter()
            .collect::<CollectorResult<Vec<_>>>()?;

        let flatten_iter =
            peripherals_per_adapter
                .into_iter()
                .flat_map(|(adapter_dto, peripherals)| {
                    peripherals
                        .into_iter()
                        .map(move |peripheral| (adapter_dto.clone(), peripheral))
                });

        let intermediate_result: Vec<Arc<Mutex<AdapterDto>>> = stream::iter(flatten_iter)
            .map(|(adapter_dto, peripheral)| async move {
                peripheral.discover_services().await?;
                {
                    let dto = PeripheralDto::from_platform(peripheral).await?;
                    let mut adapter_dto = adapter_dto.lock().await;
                    adapter_dto.add_peripheral(dto);
                }
                Ok::<Arc<Mutex<AdapterDto>>, CollectorError>(adapter_dto)
            })
            .buffer_unordered(32)
            .collect::<Vec<_>>()
            .await
            .into_iter()
            .collect::<CollectorResult<Vec<_>>>()?;

        let mut result: Vec<AdapterDto> = vec![];

        for dto in intermediate_result {
            let dto = dto.lock().await;
            if result
                .iter()
                .any(|existing| existing.adapter_info.id == dto.adapter_info.id)
            {
                continue;
            }
            let mut dto = dto.clone();
            dto.peripherals
                .sort_unstable_by_key(|peripheral| peripheral.id.clone());
            result.push(dto);
        }

        Ok(result)
    }
}
