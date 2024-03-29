use std::sync::Arc;

use metrics_exporter_prometheus::PrometheusHandle;
use rocket::http::Status;
use rocket::{get, post};

use crate::inner::adapter_manager::AdapterManager;
use crate::inner::batch_executor::execute_batches;
use crate::inner::conf::manager::ConfigurationManager;
use crate::inner::conf::model::flat_peripheral_config::FlatPeripheralConfig;
use crate::inner::dto::{AdapterDto, Envelope, PeripheralIoRequestDto, PeripheralIoResponseDto, ResultDto};
use crate::inner::error::{CollectorError, CollectorResult};
use crate::inner::http_error::{ApiResult, HttpError};
use crate::inner::model::adapter_info::AdapterInfo;
use crate::inner::model::connected_peripherals::ConnectedPeripherals;
use crate::inner::publish::api_publisher::ApiPublisher;

#[get("/adapters/describe")]
pub(crate) async fn describe_adapters(
    adapter_manager: &rocket::State<Arc<AdapterManager>>,
) -> ApiResult<Vec<AdapterDto>> {
    let wrapped = Envelope::from(adapter_manager.describe_adapters().await?);
    Ok(wrapped.into())
}

#[get("/adapters")]
pub(crate) async fn list_adapters(adapter_manager: &rocket::State<Arc<AdapterManager>>) -> ApiResult<Vec<AdapterInfo>> {
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
pub(crate) async fn get_collector_data(storage: &rocket::State<Arc<ApiPublisher>>) -> ApiResult<Arc<ApiPublisher>> {
    Ok(Envelope::from(Arc::clone(storage)).into())
}

#[tracing::instrument(level = "info", skip_all, fields(
adapter_id = % adapter_id,
))]
#[post("/adapters/<adapter_id>/io", format = "json", data = "<request>")]
pub(crate) async fn read_write_characteristic(
    adapter_id: &str,
    request: rocket::serde::json::Json<PeripheralIoRequestDto>,
    adapter_manager: &rocket::State<Arc<AdapterManager>>,
) -> ApiResult<PeripheralIoResponseDto> {
    let Some(peripheral_manager) = adapter_manager.get_peripheral_manager(adapter_id).await? else {
        return Err(
            HttpError::new(CollectorError::AdapterNotFound(adapter_id.to_string())).with_status(Status::NotFound)
        );
    };
    let response = execute_batches(peripheral_manager, request.into_inner()).await;
    let has_errors = response
        .batch_responses
        .iter()
        .flat_map(|batch_response| batch_response.command_responses.iter())
        .flatten()
        .any(|cmd_result| matches!(cmd_result, ResultDto::Error { .. }));

    if has_errors {
        let body: CollectorResult<String> = serde_json::to_string(&response).map_err(|err| err.into());
        return Err(HttpError::new(CollectorError::ApiError(body?)).with_status(Status::BadRequest));
    }

    Ok(Envelope::from(response).into())
}

#[get("/adapters/<adapter_id>/peripherals")]
pub(crate) async fn get_connected_peripherals(
    adapter_id: &str,
    adapter_manager: &rocket::State<Arc<AdapterManager>>,
) -> ApiResult<ConnectedPeripherals> {
    let Some(peripheral_manager) = adapter_manager.get_peripheral_manager(adapter_id).await? else {
        return Err(
            HttpError::new(CollectorError::AdapterNotFound(adapter_id.to_string())).with_status(Status::NotFound)
        );
    };
    let connected_peripherals = peripheral_manager.get_all_connected_peripherals().await;

    Ok(Envelope::from(connected_peripherals).into())
}

#[get("/metrics")]
pub(crate) async fn get_metrics(handle: &rocket::State<PrometheusHandle>) -> String {
    handle.render()
}
