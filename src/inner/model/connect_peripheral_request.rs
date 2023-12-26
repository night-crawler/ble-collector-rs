use crate::inner::conf::model::characteristic_config::CharacteristicConfig;
use crate::inner::model::fqcn::Fqcn;
use crate::inner::model::peripheral_key::PeripheralKey;
use std::sync::Arc;

#[derive(Debug, Clone)]
pub(crate) struct ConnectPeripheralRequest {
    pub(crate) peripheral_key: Arc<PeripheralKey>,
    pub(crate) fqcn: Arc<Fqcn>,
    pub(crate) conf: Arc<CharacteristicConfig>,
}
