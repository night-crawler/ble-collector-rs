use std::sync::Arc;

use rocket::get;

use crate::inner::adapter_manager::ADAPTER_MANAGER;
use crate::inner::conf::flat::FlatPeripheralConfig;
use crate::inner::conf::manager::CONFIGURATION_MANAGER;
use crate::inner::dto::AdapterDto;
use crate::inner::error::CollectorError;
use crate::inner::http_error::JsonResult;

#[get("/adapters")]
pub(crate) async fn adapters() -> JsonResult<Vec<AdapterDto>, CollectorError> {
    Ok(ADAPTER_MANAGER.describe_adapters().await?.into())
}

#[get("/configurations")]
pub(crate) async fn configurations() -> JsonResult<Vec<Arc<FlatPeripheralConfig>>, CollectorError> {
    Ok(CONFIGURATION_MANAGER.list_peripheral_configs().await.into())
}
