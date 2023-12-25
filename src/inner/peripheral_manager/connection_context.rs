use crate::inner::conf::model::characteristic_config::CharacteristicConfig;
use crate::inner::conf::model::flat_peripheral_config::FlatPeripheralConfig;
use crate::inner::model::fqcn::Fqcn;
use btleplug::api::{Characteristic, Peripheral as _};
use btleplug::platform::Peripheral;
use std::fmt::{Display, Formatter};
use std::sync::Arc;

pub(super) struct ConnectionContext {
    pub(super) peripheral: Arc<Peripheral>,
    pub(super) characteristic: Characteristic,
    pub(super) characteristic_config: Arc<CharacteristicConfig>,
    pub(super) fqcn: Arc<Fqcn>,
    pub(super) peripheral_config: Arc<FlatPeripheralConfig>,
}

impl Display for ConnectionContext {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        let name = self.characteristic_config.name().unwrap_or(Arc::new("".to_string()));
        write!(
            f,
            "{}[{}]::{}/{name}[{}]",
            self.peripheral_config.name,
            self.peripheral.address(),
            self.characteristic.service_uuid,
            self.characteristic.uuid
        )
    }
}
