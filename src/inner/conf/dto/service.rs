use std::sync::Arc;
use std::time::Duration;

use serde::{Deserialize, Serialize};
use serde_with::serde_as;
use serde_with::DurationSeconds;
use uuid::Uuid;

use crate::inner::conf::dto::characteristic::CharacteristicConfigDto;

#[serde_as]
#[derive(Debug, Clone, Serialize, Deserialize, Eq, PartialEq)]
pub(crate) struct ServiceConfigDto {
    pub(crate) name: Option<Arc<String>>,
    pub(crate) uuid: Uuid,
    #[serde_as(as = "DurationSeconds")]
    pub(crate) default_delay_sec: Duration,
    pub(crate) default_history_size: usize,
    pub(crate) characteristics: Vec<CharacteristicConfigDto>,
}
