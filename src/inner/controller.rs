use rocket::get;

use crate::inner::adapter_manager::ADAPTER_MANAGER;
use crate::inner::dto::AdapterDto;
use crate::inner::error::CollectorError;
use crate::inner::http_error::JsonResult;

#[get("/sample")]
pub(crate) async fn index() -> &'static str {
    "Hello, world!"
}

#[get("/adapters")]
pub(crate) async fn adapters() -> JsonResult<Vec<AdapterDto>, CollectorError> {
    Ok(ADAPTER_MANAGER.describe_adapter().await?.into())
}
