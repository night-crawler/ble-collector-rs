use std::sync::Arc;
use std::time::Duration;

use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::inner::conf::dto::characteristic::CharacteristicConfigDto;

#[derive(Debug, Clone, Serialize, Deserialize, Eq, PartialEq)]
pub(crate) struct ServiceConfigDto {
    pub(crate) name: Option<Arc<String>>,
    pub(crate) uuid: Uuid,
    #[serde(with = "humantime_serde")]
    pub(crate) default_delay: Duration,
    pub(crate) default_history_size: usize,
    pub(crate) characteristics: Vec<CharacteristicConfigDto>,
}
