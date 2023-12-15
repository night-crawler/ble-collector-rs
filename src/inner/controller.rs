use metrics_exporter_prometheus::PrometheusHandle;
use std::sync::Arc;

use rocket::http::Status;
use rocket::{get, post};

use crate::inner::adapter_manager::AdapterManager;
use crate::inner::batch_executor::execute_batches;
use crate::inner::conf::flat::FlatPeripheralConfig;
use crate::inner::conf::manager::ConfigurationManager;
use crate::inner::dto::{
    AdapterDto, AdapterInfoDto, Envelope, PeripheralIoRequestDto, PeripheralIoResponseDto,
};
use crate::inner::error::CollectorError;
use crate::inner::http_error::{ApiResult, HttpError};
use crate::inner::storage::Storage;

#[get("/adapters/describe")]
pub(crate) async fn describe_adapters(
    adapter_manager: &rocket::State<Arc<AdapterManager>>,
) -> ApiResult<Vec<AdapterDto>> {
    let wrapped = Envelope::from(adapter_manager.describe_adapters().await?);
    Ok(wrapped.into())
}

#[get("/adapters")]
pub(crate) async fn list_adapters(
    adapter_manager: &rocket::State<Arc<AdapterManager>>,
) -> ApiResult<Vec<AdapterInfoDto>> {
    let wrapped = Envelope::from(adapter_manager.list_adapters().await?);
    Ok(wrapped.into())
}

#[get("/configurations")]
pub(crate) async fn list_configurations(
    configuration_manager: &rocket::State<Arc<ConfigurationManager>>,
) -> ApiResult<Vec<Arc<FlatPeripheralConfig>>> {
    let wrapped = Envelope::from(configuration_manager.list_peripheral_configs().await);
    Ok(wrapped.into())
}

#[get("/data")]
pub(crate) async fn get_collector_data(
    storage: &rocket::State<Arc<Storage>>,
) -> ApiResult<Arc<Storage>> {
    Ok(Envelope::from(Arc::clone(storage)).into())
}

#[post("/adapters/<adapter_id>/io", format = "json", data = "<request>")]
pub(crate) async fn read_write_characteristic(
    adapter_id: &str,
    request: rocket::serde::json::Json<PeripheralIoRequestDto>,
    adapter_manager: &rocket::State<Arc<AdapterManager>>,
) -> ApiResult<PeripheralIoResponseDto> {
    let Some(peripheral_manager) = adapter_manager.get_peripheral_manager(adapter_id).await? else {
        return Err(
            HttpError::new(CollectorError::AdapterNotFound(adapter_id.to_string()))
                .with_status(Status::NotFound),
        );
    };
    let response = execute_batches(peripheral_manager, request.into_inner()).await;
    Ok(Envelope::from(response).into())
}

#[get("/metrics")]
pub(crate) async fn get_metrics(handle: &rocket::State<PrometheusHandle>) -> String {
    handle.render()
}
