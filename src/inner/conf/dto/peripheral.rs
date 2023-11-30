use serde::{Deserialize, Serialize};
use serde_with::serde_as;

use crate::inner::conf::dto::service::ServiceConfigDto;
use crate::inner::conf::parse::Filter;

#[serde_as]
#[derive(Debug, Clone, Serialize, Deserialize, Eq, PartialEq)]
pub(crate) struct PeripheralConfigDto {
    pub(crate) name: String,
    pub(crate) adapter: Option<Filter>,
    pub(crate) device_id: Option<Filter>,
    pub(crate) device_name: Option<Filter>,
    pub(crate) services: Vec<ServiceConfigDto>,
}
