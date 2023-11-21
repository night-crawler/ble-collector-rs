use std::sync::Arc;

use btleplug::api::{Central, Manager as _, Peripheral as _};
use btleplug::platform::{Adapter, Manager, Peripheral};
use futures_util::stream;
use futures_util::StreamExt;
use lazy_static::lazy_static;
use log::{error, info, warn};
use tokio::sync::Mutex;
use tokio::task::JoinSet;

use crate::inner::adapter_service_manager::AdapterServiceManager;
use crate::inner::dto::{AdapterDto, PeripheralDto};
use crate::inner::error::{CollectorError, CollectorResult};

lazy_static! {
    pub(crate) static ref ADAPTER_MANAGER: AdapterManager = {
        Default::default()
    };
}


#[derive(Default)]
pub(crate) struct AdapterManager {
    adapters: Mutex<Vec<Arc<AdapterServiceManager>>>,
}


impl AdapterManager {
    pub(crate) async fn init(&self) -> btleplug::Result<()> {
        let manager = Manager::new().await?;
        let adapters = manager.adapters().await?;
        for adapter in adapters {
            info!("Found adapter: {:?}", adapter);
            self.add_adapter(adapter).await;
        }
        Ok(())
    }

    async fn add_adapter(&self, adapter: Adapter) {
        self.adapters.lock().await.push(Arc::new(AdapterServiceManager::new(adapter)))
    }

    pub(crate) async fn start_discovery(&self) -> btleplug::Result<()> {
        let mut join_set = JoinSet::new();
        for adapter in self.adapters.lock().await.iter() {
            let adapter = Arc::clone(adapter);
            join_set.spawn(async move {
                adapter.start_discovery().await
            });
        }

        if let Some(result) = join_set.join_next().await {
            info!("Ending discovery: {result:?}");
        }
        // TODO: restart discovery and recreate everything if we've reached this point
        Ok(())
    }

    pub(crate) async fn describe_adapter(&self) -> CollectorResult<Vec<AdapterDto>> {
        let adapters = self.adapters.lock().await;

        let peripherals_per_adapter = stream::iter(adapters.iter())
            .map(Arc::clone)
            .map(|adapter_service_manager| {
                async move {
                    let adapter_info = adapter_service_manager.adapter.adapter_info().await?;
                    let adapter_dto = AdapterDto::try_from(adapter_info)?;
                    let peripherals = adapter_service_manager.adapter.peripherals().await?;

                    Ok::<(Arc<Mutex<AdapterDto>>, Vec<Peripheral>), CollectorError>((
                        Arc::new(Mutex::new(adapter_dto)),
                        peripherals
                    ))
                }
            })
            .buffer_unordered(4)
            .collect::<Vec<_>>()
            .await
            .into_iter()
            .collect::<CollectorResult<Vec<_>>>()?;

        let flatten_iter = peripherals_per_adapter
            .into_iter()
            .flat_map(|(adapter_dto, peripherals)| {
                peripherals.into_iter().map(move |peripheral| (adapter_dto.clone(), peripheral))
            });

        let intermediate_result: Vec<Arc<Mutex<AdapterDto>>> = stream::iter(flatten_iter)
            .map(|(adapter_dto, peripheral)| {
                async move {
                    {
                        let dto = PeripheralDto::from_platform(peripheral).await?;
                        let mut adapter_dto = adapter_dto.lock().await;
                        adapter_dto.add_peripheral(dto);
                    }
                    Ok::<Arc<Mutex<AdapterDto>>, CollectorError>(adapter_dto)
                }
            })
            .buffer_unordered(32)
            .collect::<Vec<_>>()
            .await
            .into_iter()
            .collect::<CollectorResult<Vec<_>>>()?;

        let mut result: Vec<AdapterDto> = vec![];

        for dto in intermediate_result {
            let dto = dto.lock().await;
            if result.iter().any(|existing| existing.id == dto.id) {
                continue;
            }
            let mut dto = dto.clone();
            dto.peripherals.sort_unstable_by_key(|peripheral| peripheral.id.clone());
            result.push(dto);
        }

        Ok(result)
    }
}
