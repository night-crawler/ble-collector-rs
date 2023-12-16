use crate::inner::conf::parse::CharacteristicConfig;
use crate::inner::conv::converter::CharacteristicValue;
use crate::inner::model::fqcn::Fqcn;
use chrono::Utc;
use std::fmt::{Display, Formatter};
use std::sync::Arc;

#[derive(Debug, Clone)]
pub(crate) struct CharacteristicPayload {
    pub(crate) created_at: chrono::DateTime<Utc>,
    pub(crate) value: CharacteristicValue,
    pub(crate) fqcn: Arc<Fqcn>,
    pub(crate) conf: Arc<CharacteristicConfig>,
}

impl Display for CharacteristicPayload {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        let parts = vec![
            self.fqcn.peripheral_address.to_string(),
            if let Some(name) = self.conf.service_name() {
                name.to_string()
            } else {
                self.fqcn.service_uuid.to_string()
            },
            if let Some(name) = self.conf.name() {
                name.to_string()
            } else {
                self.fqcn.characteristic_uuid.to_string()
            },
        ];

        write!(f, "{}={}", parts.join(":"), self.value)
    }
}
