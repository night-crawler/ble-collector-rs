use uuid::Uuid;

use crate::inner::conf::flat::ServiceCharacteristicKey;
use crate::inner::conf::parse::CharacteristicConfigDto;

#[derive(Debug, thiserror::Error)]
pub(crate) enum CollectorError {
    #[error("Bluetooth error: {0:?}")]
    BluetoothError(#[from] btleplug::Error),

    #[error("Kanal error: {0:?}")]
    KanalError(#[from] kanal::SendError),

    #[error("End of stream")]
    EndOfStream,

    #[error("IoError: {0:?}")]
    IoError(#[from] std::io::Error),

    #[error("Serialization Error: {0:?}")]
    SerializationError(#[from] serde_yaml::Error),

    #[error("Error: {0:?}")]
    AnyError(#[from] anyhow::Error),

    #[error("Duplicate configuration: {0}")]
    DuplicateConfiguration(String),

    #[error("Duplicate service configuration {service_uuid}")]
    DuplicateServiceConfiguration { service_uuid: Uuid },

    #[error("Duplicate service configuration {0}")]
    DuplicateCharacteristicConfiguration(ServiceCharacteristicKey),

    #[error("Unexpected characteristic configuration type {0:?}")]
    UnexpectedCharacteristicConfiguration(CharacteristicConfigDto),
}

pub(crate) type CollectorResult<T> = Result<T, CollectorError>;
