use rocket::get;

use crate::inner::adapter_manager::ADAPTER_MANAGER;
use crate::inner::configuration::{BleServiceConfig, CONFIGURATION_MANAGER};
use crate::inner::dto::AdapterDto;
use crate::inner::error::CollectorError;
use crate::inner::http_error::JsonResult;

#[get("/adapters")]
pub(crate) async fn adapters() -> JsonResult<Vec<AdapterDto>, CollectorError> {
    Ok(ADAPTER_MANAGER.describe_adapter().await?.into())
}

#[get("/configurations")]
pub(crate) async fn configurations() -> JsonResult<Vec<BleServiceConfig>, CollectorError> {
    Ok(CONFIGURATION_MANAGER.list_services().await.into())
}
