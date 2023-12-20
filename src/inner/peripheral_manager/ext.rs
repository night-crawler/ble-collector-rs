use btleplug::api::CentralEvent;
use btleplug::platform::PeripheralId;

pub(super) trait CentralEventExt {
    fn get_peripheral_id(&self) -> &PeripheralId;
}

impl CentralEventExt for CentralEvent {
    fn get_peripheral_id(&self) -> &PeripheralId {
        match self {
            CentralEvent::DeviceDiscovered(id)
            | CentralEvent::DeviceUpdated(id)
            | CentralEvent::DeviceConnected(id)
            | CentralEvent::DeviceDisconnected(id)
            | CentralEvent::ManufacturerDataAdvertisement { id, .. }
            | CentralEvent::ServiceDataAdvertisement { id, .. }
            | CentralEvent::ServicesAdvertisement { id, .. } => id,
        }
    }
}
