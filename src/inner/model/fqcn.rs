use btleplug::api::{BDAddr, ValueNotification};
use metrics::Label;
use serde::{Deserialize, Serialize};
use std::fmt::{Display, Formatter};
use uuid::Uuid;

#[derive(Ord, PartialOrd, Eq, PartialEq, Debug, Clone, Hash, Serialize, Deserialize)]
pub(crate) struct Fqcn {
    pub(crate) peripheral: BDAddr,
    pub(crate) service: Uuid,
    pub(crate) characteristic: Uuid,
}

impl Display for Fqcn {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}::{}:{}", self.peripheral, self.service, self.characteristic)
    }
}

impl Fqcn {
    pub(crate) fn with_characteristic(&self, service_uuid: Uuid, characteristic_uuid: Uuid) -> Self {
        Fqcn {
            service: service_uuid,
            characteristic: characteristic_uuid,
            ..*self
        }
    }

    pub(crate) fn matches(&self, value_notification: &ValueNotification) -> bool {
        self.service == value_notification.service_uuid && self.characteristic == value_notification.uuid
    }

    pub(crate) fn peripheral_label(&self) -> Label {
        Label::new("peripheral", self.peripheral.to_string())
    }

    pub(crate) fn service_label(&self) -> Label {
        Label::new("service", self.service.to_string())
    }

    pub(crate) fn characteristic_label(&self) -> Label {
        Label::new("characteristic", self.characteristic.to_string())
    }
}
