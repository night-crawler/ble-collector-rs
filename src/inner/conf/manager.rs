use std::collections::{HashMap, HashSet};
use std::sync::Arc;

use crate::inner::conf::dto::peripheral::PeripheralConfigDto;
use crate::inner::conf::model::flat_peripheral_config::FlatPeripheralConfig;
use crate::inner::conf::traits::Evaluate;
use tokio::sync::Mutex;

use crate::inner::error::{CollectorError, CollectorResult};
use crate::inner::model::peripheral_key::PeripheralKey;

#[derive(Default)]
pub(crate) struct ConfigurationManager {
    peripheral_map: Arc<Mutex<HashMap<Arc<String>, Arc<FlatPeripheralConfig>>>>,
}

impl ConfigurationManager {
    pub(crate) async fn add_peripherals(&self, peripheral_configs: Vec<PeripheralConfigDto>) -> CollectorResult<()> {
        let mut unique_names = HashSet::new();
        for service in peripheral_configs.iter() {
            let service_name = service.name.clone();
            if !unique_names.insert(service_name.clone()) {
                return Err(CollectorError::DuplicateConfiguration(service_name));
            }
        }

        let mut existing_services = self.peripheral_map.lock().await;
        for peripheral_config in peripheral_configs.iter() {
            if existing_services.contains_key(&peripheral_config.name) {
                return Err(CollectorError::DuplicateConfiguration(peripheral_config.name.clone()));
            }
        }

        for peripheral_config in peripheral_configs {
            let flat_conf = FlatPeripheralConfig::try_from(peripheral_config)?;
            existing_services.insert(flat_conf.name.clone(), Arc::new(flat_conf));
        }

        Ok(())
    }
    pub(crate) async fn add_peripheral_config(&self, peripheral_config: PeripheralConfigDto) -> CollectorResult<()> {
        let existing_services = self.peripheral_map.lock().await;
        if existing_services.contains_key(&peripheral_config.name) {
            return Err(CollectorError::DuplicateConfiguration(peripheral_config.name));
        }
        let flat_conf = FlatPeripheralConfig::try_from(peripheral_config)?;
        self.peripheral_map
            .lock()
            .await
            .insert(flat_conf.name.clone(), Arc::new(flat_conf));
        Ok(())
    }
    pub(crate) async fn list_peripheral_configs(&self) -> Vec<Arc<FlatPeripheralConfig>> {
        let services = self.peripheral_map.lock().await;
        services.values().cloned().collect()
    }
    pub(crate) async fn get_matching_config(
        &self,
        peripheral_key: &PeripheralKey,
    ) -> Option<Arc<FlatPeripheralConfig>> {
        let peripheral_map = self.peripheral_map.lock().await;
        peripheral_map
            .values()
            .find(|conf| conf.evaluate(peripheral_key))
            .cloned()
    }
}
