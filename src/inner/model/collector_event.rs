use crate::inner::conf::model::characteristic_config::CharacteristicConfig;
use crate::inner::model::characteristic_payload::CharacteristicPayload;
use crate::inner::model::connect_peripheral_request::ConnectPeripheralRequest;
use crate::inner::model::fqcn::Fqcn;
use std::sync::Arc;

#[derive(Debug, Clone)]
pub(crate) enum CollectorEvent {
    Payload(Arc<CharacteristicPayload>),
    Connect(ConnectPeripheralRequest),
    Disconnect(Arc<Fqcn>, Arc<CharacteristicConfig>),
}
