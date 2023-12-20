use std::sync::Arc;

use metrics_exporter_prometheus::PrometheusHandle;
use rocket::http::Status;
use rocket::{get, post};
use tracing::info_span;

use crate::inner::adapter_manager::AdapterManager;
use crate::inner::batch_executor::execute_batches;
use crate::inner::conf::manager::ConfigurationManager;
use crate::inner::conf::model::flat_peripheral_config::FlatPeripheralConfig;
use crate::inner::dto::{AdapterDto, Envelope, PeripheralIoRequestDto, PeripheralIoResponseDto};
use crate::inner::error::CollectorError;
use crate::inner::http_error::{ApiResult, HttpError};
use crate::inner::model::adapter_info::AdapterInfo;
use crate::inner::model::connected_peripherals::ConnectedPeripherals;
use crate::inner::process::api_publisher::ApiPublisher;

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
) -> ApiResult<Vec<AdapterInfo>> {
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
    storage: &rocket::State<Arc<ApiPublisher>>,
) -> ApiResult<Arc<ApiPublisher>> {
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
    let span = info_span!("read_write_characteristic", adapter_id = adapter_id);
    let response = execute_batches(peripheral_manager, request.into_inner(), span).await;
    Ok(Envelope::from(response).into())
}

#[get("/adapters/<adapter_id>/peripherals")]
pub(crate) async fn get_connected_peripherals(
    adapter_id: &str,
    adapter_manager: &rocket::State<Arc<AdapterManager>>,
) -> ApiResult<ConnectedPeripherals> {
    let Some(peripheral_manager) = adapter_manager.get_peripheral_manager(adapter_id).await? else {
        return Err(
            HttpError::new(CollectorError::AdapterNotFound(adapter_id.to_string()))
                .with_status(Status::NotFound),
        );
    };
    let connected_peripherals = peripheral_manager.get_all_connected_peripherals().await;

    Ok(Envelope::from(connected_peripherals).into())
}

#[get("/metrics")]
pub(crate) async fn get_metrics(handle: &rocket::State<PrometheusHandle>) -> String {
    handle.render()
}
