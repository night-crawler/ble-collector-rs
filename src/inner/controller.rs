use std::sync::Arc;

use crate::CollectorState;
use rocket::{get, post};

use crate::inner::conf::flat::FlatPeripheralConfig;
use crate::inner::dto::{AdapterDto, WritePeripheralCommandsRequestDto};
use crate::inner::error::CollectorError;
use crate::inner::http_error::JsonResult;
use crate::inner::storage::Storage;

#[get("/adapters")]
pub(crate) async fn adapters(
    collector_state: &rocket::State<CollectorState>,
) -> JsonResult<Vec<AdapterDto>, CollectorError> {
    Ok(collector_state
        .adapter_manager
        .describe_adapters()
        .await?
        .into())
}

#[get("/configurations")]
pub(crate) async fn configurations(
    collector_state: &rocket::State<CollectorState>,
) -> JsonResult<Vec<Arc<FlatPeripheralConfig>>, CollectorError> {
    Ok(collector_state
        .configuration_manager
        .list_peripheral_configs()
        .await
        .into())
}

#[get("/data")]
pub(crate) async fn data(
    collector_state: &rocket::State<CollectorState>,
) -> JsonResult<Arc<Storage>, CollectorError> {
    Ok(collector_state.storage.clone().into())
}

#[post("/rw", format = "json", data = "<request>")]
pub(crate) async fn rw(
    collector_state: &rocket::State<CollectorState>,
    request: rocket::serde::json::Json<WritePeripheralCommandsRequestDto>,
) -> JsonResult<(), CollectorError> {
    // collector_state
    //     .per
    //     .write_peripheral_commands(request.into_inner())
    //     .await?;
    todo!()
}
