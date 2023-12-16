use btleplug::api::{BDAddr, ValueNotification};
use metrics::Label;
use rocket::serde::{Deserialize, Serialize};
use std::fmt::{Display, Formatter};
use uuid::Uuid;

#[derive(Ord, PartialOrd, Eq, PartialEq, Debug, Clone, Hash, Serialize, Deserialize)]
pub(crate) struct Fqcn {
    pub(crate) peripheral_address: BDAddr,
    pub(crate) service_uuid: Uuid,
    pub(crate) characteristic_uuid: Uuid,
}

impl Display for Fqcn {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "{}::{}:{}",
            self.peripheral_address, self.service_uuid, self.characteristic_uuid
        )
    }
}

impl Fqcn {
    pub(crate) fn with_characteristic(
        &self,
        service_uuid: Uuid,
        characteristic_uuid: Uuid,
    ) -> Self {
        Fqcn {
            service_uuid,
            characteristic_uuid,
            ..*self
        }
    }

    pub(crate) fn matches(&self, value_notification: &ValueNotification) -> bool {
        self.service_uuid == value_notification.service_uuid
            && self.characteristic_uuid == value_notification.uuid
    }

    pub(crate) fn peripheral_label(&self) -> Label {
        Label::new("peripheral", self.peripheral_address.to_string())
    }
}
