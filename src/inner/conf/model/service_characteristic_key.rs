use btleplug::api::Characteristic;
use serde::{Deserialize, Serialize};
use std::fmt::{Display, Formatter};

use uuid::Uuid;

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
