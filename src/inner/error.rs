use crate::inner::conf::model::characteristic_config::CharacteristicConfig;
use crate::inner::conf::model::service_characteristic_key::ServiceCharacteristicKey;
use std::sync::Arc;
use uuid::Uuid;

use crate::inner::conv::converter::ConversionError;

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

    #[error("Duplicate service configuration {0}")]
    DuplicateServiceConfiguration(Uuid),

    #[error("Duplicate service configuration {0}")]
    DuplicateCharacteristicConfiguration(ServiceCharacteristicKey),

    #[error("Unexpected characteristic configuration type {0:?}")]
    UnexpectedCharacteristicConfiguration(Arc<CharacteristicConfig>),

    #[error("Conversion error: {0:?}")]
    ConversionError(#[from] ConversionError),

    #[error("Rocket error: {0:?}")]
    RocketError(#[from] rocket::Error),

    #[error("Timeout: {0:?}")]
    TimeoutError(#[from] tokio::time::error::Elapsed),

    #[error("Join Error")]
    JoinError(#[from] tokio::task::JoinError),

    #[error("Adapter `{0}` not found")]
    AdapterNotFound(String),

    #[error("Unexpected IO command")]
    UnexpectedIoCommand,

    #[error("Tracing filter parse error: {0}")]
    TracingFilterParseError(#[from] tracing_subscriber::filter::ParseError),

    #[error("Tracing filter parse error: {0}")]
    AcquireError(#[from] tokio::sync::AcquireError),
}

pub(crate) type CollectorResult<T> = Result<T, CollectorError>;
