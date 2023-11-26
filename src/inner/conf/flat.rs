use std::collections::{HashMap, HashSet};
use std::fmt::{Display, Formatter};

use btleplug::api::Characteristic;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::inner::conf::parse::{
    CharacteristicConfigDto, Filter, PeripheralConfigDto, ServiceConfigDto,
};
use crate::inner::error::{CollectorError, CollectorResult};

#[derive(Debug, Clone, Serialize, Deserialize, Eq, PartialEq, Hash)]
pub(crate) struct ServiceCharacteristicKey {
    pub(crate) service_uuid: Uuid,
    pub(crate) characteristic_uuid: Uuid,
}

impl From<&Characteristic> for ServiceCharacteristicKey {
    fn from(value: &Characteristic) -> Self {
        Self {
            service_uuid: value.service_uuid,
            characteristic_uuid: value.uuid,
        }
    }
}

impl Display for ServiceCharacteristicKey {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}:{}", self.service_uuid, self.characteristic_uuid)
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, Eq, PartialEq)]
pub(crate) struct FlatPeripheralConfig {
    pub(crate) name: String,
    pub(crate) adapter: Option<Filter>,
    pub(crate) device_id: Option<Filter>,
    pub(crate) device_name: Option<Filter>,

    pub(crate) service_map: HashMap<ServiceCharacteristicKey, CharacteristicConfigDto>,
}

impl From<(Uuid, &CharacteristicConfigDto)> for ServiceCharacteristicKey {
    fn from((service_uuid, char_conf): (Uuid, &CharacteristicConfigDto)) -> Self {
        Self {
            service_uuid,
            characteristic_uuid: *char_conf.uuid(),
        }
    }
}

impl FlatPeripheralConfig {
    fn add_service(&mut self, service: ServiceConfigDto) -> CollectorResult<()> {
        let service_uuid = service.uuid;
        let mut unique_keys = HashSet::new();
        for char_conf_dto in service.characteristics.iter() {
            let key = ServiceCharacteristicKey::from((service_uuid, char_conf_dto));
            if !unique_keys.insert(key.clone()) {
                return Err(CollectorError::DuplicateCharacteristicConfiguration(key));
            }

            if self.service_map.contains_key(&key) {
                return Err(CollectorError::DuplicateCharacteristicConfiguration(key));
            }
        }

        for char_conf_dto in service.characteristics {
            let key = ServiceCharacteristicKey::from((service_uuid, &char_conf_dto));
            self.service_map.insert(key, char_conf_dto);
        }

        Ok(())
    }
}

impl TryFrom<PeripheralConfigDto> for FlatPeripheralConfig {
    type Error = CollectorError;

    fn try_from(value: PeripheralConfigDto) -> Result<Self, Self::Error> {
        let mut flat_conf = Self {
            name: value.name,
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
