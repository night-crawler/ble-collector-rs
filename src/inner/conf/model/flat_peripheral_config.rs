use crate::inner::conf::dto::peripheral::PeripheralConfigDto;
use crate::inner::conf::dto::service::ServiceConfigDto;
use crate::inner::conf::model::characteristic_config::CharacteristicConfig;
use crate::inner::conf::model::filter::Filter;
use crate::inner::conf::model::service_characteristic_key::ServiceCharacteristicKey;
use crate::inner::conf::traits::Evaluate;
use crate::inner::error::{CollectorError, CollectorResult};
use crate::inner::model::peripheral_key::PeripheralKey;
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};
use std::sync::Arc;

#[derive(Debug, Clone, Serialize, Deserialize, Eq, PartialEq)]
pub(crate) struct FlatPeripheralConfig {
    pub(crate) name: Arc<String>,
    pub(crate) adapter: Option<Filter>,
    pub(crate) device_id: Option<Filter>,
    pub(crate) device_name: Option<Filter>,

    pub(crate) service_map: HashMap<ServiceCharacteristicKey, Arc<CharacteristicConfig>>,
}

impl FlatPeripheralConfig {
    fn add_service(&mut self, service: ServiceConfigDto) -> CollectorResult<()> {
        let service_uuid = service.uuid;
        let mut unique_keys = HashSet::new();
        for char_conf_dto in service.characteristics.iter() {
            let key = ServiceCharacteristicKey {
                service_uuid,
                characteristic_uuid: *char_conf_dto.uuid(),
            };
            if !unique_keys.insert(key.clone()) {
                return Err(CollectorError::DuplicateCharacteristicConfiguration(key));
            }

            if self.service_map.contains_key(&key) {
                return Err(CollectorError::DuplicateServiceConfiguration(service_uuid));
            }
        }

        for char_conf_dto in &service.characteristics {
            let flat_char_conf = CharacteristicConfig::try_from((char_conf_dto, &service))?;
            let key = ServiceCharacteristicKey {
                service_uuid,
                characteristic_uuid: *char_conf_dto.uuid(),
            };
            self.service_map.insert(key, Arc::new(flat_char_conf));
        }

        Ok(())
    }
}

impl TryFrom<PeripheralConfigDto> for FlatPeripheralConfig {
    type Error = CollectorError;

    fn try_from(value: PeripheralConfigDto) -> Result<Self, Self::Error> {
        let mut flat_conf = Self {
            name: Arc::new(value.name),
            adapter: value.adapter,
            device_id: value.device_id,
            device_name: value.device_name,
            service_map: Default::default(),
        };

        for service in value.services {
            flat_conf.add_service(service)?;
        }

        Ok(flat_conf)
    }
}

impl Evaluate<&PeripheralKey, bool> for FlatPeripheralConfig {
    fn evaluate(&self, source: &PeripheralKey) -> bool {
        let adapter_matches = self
            .adapter
            .as_ref()
            .map(|filter| filter.evaluate(&source.adapter_id))
            .unwrap_or(true);
        let device_id_matches = self
            .device_id
            .as_ref()
            .map(|filter| filter.evaluate(&source.peripheral_address.to_string()))
            .unwrap_or(true);

        let name_matches = match (self.device_name.as_ref(), &source.name) {
            (Some(filter), Some(name)) => filter.evaluate(name),
            (None, Some(_)) => true,
            (Some(_), None) => false,
            (None, None) => true,
        };

        adapter_matches && device_id_matches && name_matches
    }
}
