use std::sync::Arc;

use clap::Parser;
use fern::colors::{Color, ColoredLevelConfig};
use log::{info, warn};
use metrics_exporter_prometheus::{PrometheusBuilder, PrometheusHandle};
use metrics_util::MetricKindMask;
use rocket::routes;
use tokio::task::JoinSet;

use crate::inner::adapter_manager::AdapterManager;
use crate::inner::conf::cmd_args::AppConf;
use crate::inner::conf::dto::collector_configuration::CollectorConfigurationDto;
use crate::inner::conf::manager::ConfigurationManager;
use crate::inner::controller::{
    describe_adapters, get_collector_data, get_metrics, list_adapters, list_configurations,
    read_write_characteristic,
};
use crate::inner::error::CollectorResult;
use crate::inner::metrics::describe_metrics;
use crate::inner::model::characteristic_payload::CharacteristicPayload;
use crate::inner::process::metric_publisher::MetricPublisher;
use crate::inner::process::processor::PayloadProcessor;
use crate::inner::process::ProcessPayload;
use inner::process::api_publisher::ApiPublisher;

mod inner;

fn init_logging() -> CollectorResult<()> {
    let colors = ColoredLevelConfig::new()
        .debug(Color::Magenta)
        .error(Color::Red)
        .info(Color::Green)
        .warn(Color::Yellow);
    fern::Dispatch::new()
        .format(move |out, message, record| {
            out.finish(format_args!(
                "[{} {} {}] {}",
                humantime::format_rfc3339_millis(std::time::SystemTime::now()),
                colors.color(record.level()),
                record.target(),
                message
            ))
        })
        .level(log::LevelFilter::Info)
        .level_for("rocket", log::LevelFilter::Info)
        .level_for("ble_collector_rs", log::LevelFilter::Debug)
        .chain(std::io::stdout())
        .apply()?;
    Ok(())
}

#[tokio::main]
async fn main() -> CollectorResult<()> {
    init_logging()?;
    let app_conf = Arc::new(AppConf::parse());
    let builder = PrometheusBuilder::new();
    let prometheus_handle: PrometheusHandle = builder
        .idle_timeout(
            MetricKindMask::COUNTER | MetricKindMask::HISTOGRAM | MetricKindMask::GAUGE,
            Some(app_conf.metrics_idle_timeout),
        )
        .install_recorder()
        .expect("failed to install recorder");
    describe_metrics();

    let collector_conf = CollectorConfigurationDto::try_from(app_conf.as_ref())?;
    let configuration_manager = Arc::new(ConfigurationManager::default());
    configuration_manager
        .add_peripherals(collector_conf.peripherals)
        .await?;

    let (payload_sender, payload_receiver) = kanal::unbounded_async::<CharacteristicPayload>();

    let adapter_manager = Arc::new(AdapterManager::new(
        Arc::clone(&configuration_manager),
        payload_sender,
        Arc::clone(&app_conf),
    ));
    let payload_storage_processor = Arc::new(ApiPublisher::new());
    adapter_manager.init().await?;

    let payload_metric_publisher = Arc::new(MetricPublisher::new());

    let payload_processor = {
        let payload_storage_processor = Arc::clone(&payload_storage_processor);
        let payload_storage_processor: Arc<dyn ProcessPayload + Sync + Send> =
            payload_storage_processor;

        let payload_metric_publisher = Arc::clone(&payload_metric_publisher);
        let payload_metric_publisher: Arc<dyn ProcessPayload + Sync + Send> =
            payload_metric_publisher;

        Arc::new(PayloadProcessor::new(
            payload_receiver.clone_sync(),
            vec![payload_storage_processor, payload_metric_publisher],
        ))
    };

    let mut join_set: JoinSet<CollectorResult<()>> = JoinSet::new();

    {
        let adapter_manager = adapter_manager.clone();
        join_set.spawn(async move {
            adapter_manager.start_discovery().await?;
            Ok(())
        });
    }

    {
        let payload_processor = payload_processor.clone();
        join_set.spawn_blocking(|| {
            let handle = std::thread::spawn(move || {
                payload_processor.block_on_receiving();
            });
            let result = handle.join();
            warn!("Storage receiver has ended: {result:?}");
            Ok(())
        });
    }

    join_set.spawn(async move {
        rocket::build()
            .manage(configuration_manager)
            .manage(adapter_manager)
            .manage(payload_storage_processor)
            .manage(prometheus_handle)
            .mount(
                "/ble",
                routes![
                    describe_adapters,
                    list_configurations,
                    get_collector_data,
                    list_adapters,
                    read_write_characteristic,
                ],
            )
            .mount("/", routes![get_metrics])
            .configure(
                rocket::config::Config::figment()
                    .merge(("address", Arc::clone(&app_conf.listen_address)))
                    .merge(("port", app_conf.port)),
            )
            .launch()
            .await?;
        Ok(())
    });

    if let Some(result) = join_set.join_next().await {
        info!("Main has ended: {result:?}");
    }

    Ok(())
}
