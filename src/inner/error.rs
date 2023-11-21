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

    #[error("Arc unwrap error")]
    ArcUnwrapError,
}

pub(crate) type CollectorResult<T> = Result<T, CollectorError>;
