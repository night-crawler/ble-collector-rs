use std::sync::Arc;
use std::time::Duration;

use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::inner::conv::converter::Converter;

#[derive(Debug, Clone, Serialize, Deserialize, Eq, PartialEq)]
pub(crate) enum CharacteristicConfigDto {
    Subscribe {
        name: Option<Arc<String>>,
        uuid: Uuid,
        history_size: Option<usize>,
        #[serde(default)]
        converter: Converter,
    },
    Poll {
        name: Option<Arc<String>>,
        uuid: Uuid,
        #[serde(default)]
        #[serde(with = "humantime_serde")]
        delay: Option<Duration>,
        history_size: Option<usize>,
        #[serde(default)]
        converter: Converter,
    },
}

impl CharacteristicConfigDto {
    pub(crate) fn uuid(&self) -> &Uuid {
        match self {
            CharacteristicConfigDto::Subscribe { uuid, .. } => uuid,
            CharacteristicConfigDto::Poll { uuid, .. } => uuid,
        }
    }
}
