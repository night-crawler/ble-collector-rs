use std::sync::Arc;

use crate::inner::conf::cmd_args::AppConf;
use crate::inner::conf::manager::ConfigurationManager;
use btleplug::api::{Central, Manager as _};
use btleplug::platform::{Adapter, Manager, Peripheral};
use futures_util::stream;
use futures_util::StreamExt;
use tokio::sync::Mutex;
use tokio::task::JoinSet;
use tracing::{info, info_span, warn};

use crate::inner::dto::{AdapterDto, PeripheralDto};
use crate::inner::error::{CollectorError, CollectorResult};
use crate::inner::model::adapter_info::AdapterInfo;
use crate::inner::model::characteristic_payload::CharacteristicPayload;
use crate::inner::peripheral_manager::PeripheralManager;
use crate::inner::process::FanOutSender;

pub(crate) struct AdapterManager {
    peripheral_managers: Mutex<Vec<Arc<PeripheralManager>>>,
    payload_sender: Arc<FanOutSender<Arc<CharacteristicPayload>>>,
    configuration_manager: Arc<ConfigurationManager>,
    app_conf: Arc<AppConf>,
}

impl AdapterManager {
    pub(crate) fn new(
        configuration_manager: Arc<ConfigurationManager>,
        payload_sender: FanOutSender<Arc<CharacteristicPayload>>,
        app_conf: Arc<AppConf>,
    ) -> Self {
        Self {
            peripheral_managers: Default::default(),
            payload_sender: Arc::new(payload_sender),
            configuration_manager,
            app_conf,
        }
    }
    pub(crate) async fn init(&self) -> CollectorResult<()> {
        async move {
            let manager = Manager::new().await?;
            let adapters = manager.adapters().await?;
            info!(adapters = ?adapters, "Discovered {} adapter(s)", adapters.len());
            for adapter in adapters {
                let info = AdapterInfo::try_from(adapter.adapter_info().await?)?;
                info!(adapter_info = %info, "Discovered adapter");
                self.init_peripheral_manager(adapter).await?;
            }
            Ok(())
        }
        .await
    }

    async fn init_peripheral_manager(&self, adapter: Adapter) -> CollectorResult<()> {
        let adapter_info = AdapterInfo::try_from(adapter.adapter_info().await?)?;
        let span = info_span!("PeripheralManager", adapter = adapter_info.id);
        self.peripheral_managers
            .lock()
            .await
            .push(Arc::new(PeripheralManager::new(
                adapter,
                self.payload_sender.clone(),
                self.configuration_manager.clone(),
                Arc::clone(&self.app_conf),
                span,
            )));
        Ok(())
    }

    pub(crate) async fn get_peripheral_manager(
        &self,
        adapter_id: &str,
    ) -> CollectorResult<Option<Arc<PeripheralManager>>> {
        let managers = self.peripheral_managers.lock().await;

        for manager in managers.iter() {
            let adapter_info = manager.adapter.adapter_info().await?;
            let adapter_info = AdapterInfo::try_from(adapter_info)?;
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
            warn!(result = ?result, "Peripheral discovery has ended");
        }
        // TODO: restart discovery and recreate everything if we've reached this point
        Ok(())
    }

    pub(crate) async fn list_adapters(&self) -> CollectorResult<Vec<AdapterInfo>> {
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
            let adapter_info = AdapterInfo::try_from(info)?;
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
