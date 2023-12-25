use std::collections::HashMap;
use std::fmt::Display;

use anyhow::Context;
use btleplug::api::BDAddr;
use btleplug::platform::PeripheralId;

#[derive(Debug, Eq, PartialEq, Hash, Ord, PartialOrd)]
pub(crate) struct PeripheralKey {
    pub(crate) adapter_id: String,
    pub(crate) peripheral_address: BDAddr,
    pub(crate) name: Option<String>,
}

impl Display for PeripheralKey {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let name = self.name.as_deref().unwrap_or("Unknown");
        write!(f, "{} {name}[{}]", self.adapter_id, self.peripheral_address)
    }
}

impl TryFrom<&PeripheralId> for PeripheralKey {
    type Error = anyhow::Error;

    fn try_from(value: &PeripheralId) -> Result<Self, Self::Error> {
        let serialized = serde_json::to_value(value)?;
        let deserialized: HashMap<String, String> = serde_json::from_value(serialized)?;
        let path = deserialized.into_values().next().context("No values")?;
        let path = path.strip_prefix("/org/bluez/").context("No /org/bluez prefix")?;
        let (adapter, address) = path.rsplit_once('/').context("No / delimiter")?;
        let address = address.strip_prefix("dev_").context("No dev_ prefix")?;
        let address = address.replace('_', ":");

        let address = BDAddr::from_str_delim(&address)?;

        Ok(Self {
            adapter_id: adapter.to_string(),
            peripheral_address: address,
            name: None,
        })
    }
}
