use std::sync::Arc;

use clap::Parser;
use console_subscriber::ConsoleLayer;
use metrics_exporter_prometheus::PrometheusBuilder;
use metrics_tracing_context::{MetricsLayer, TracingContextLayer};
use metrics_util::layers::Stack;
use metrics_util::MetricKindMask;
use rocket::routes;
use tokio::task::JoinSet;
use tracing::warn;
use tracing_subscriber::layer::SubscriberExt;
use tracing_subscriber::util::SubscriberInitExt;
use tracing_subscriber::EnvFilter;

use inner::process::api_publisher::ApiPublisher;

use crate::inner::adapter_manager::AdapterManager;
use crate::inner::api::{
    describe_adapters, get_collector_data, get_connected_peripherals, get_metrics, list_adapters,
    list_configurations, read_write_characteristic,
};
use crate::inner::conf::cmd_args::AppConf;
use crate::inner::conf::dto::collector_configuration::CollectorConfigurationDto;
use crate::inner::conf::manager::ConfigurationManager;
use crate::inner::error::CollectorResult;
use crate::inner::metrics::describe_metrics;
use crate::inner::model::characteristic_payload::CharacteristicPayload;
use crate::inner::process::metric_publisher::MetricPublisher;
use crate::inner::process::processor::PayloadProcessor;
use crate::inner::process::ProcessPayload;

mod inner;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let metrics_layer = MetricsLayer::new();
    let console_layer = ConsoleLayer::builder().with_default_env().spawn();
    let fmt_layer = tracing_subscriber::fmt::layer()
        .compact()
        .with_ansi(atty::is(atty::Stream::Stdout))
        .with_target(false);
    let filter_layer = EnvFilter::try_from_default_env().or_else(|_| EnvFilter::try_new("info"))?;

    tracing_subscriber::registry()
        .with(filter_layer)
        .with(fmt_layer)
        .with(metrics_layer)
        .with(console_layer)
        .init();

    let app_conf = Arc::new(AppConf::parse());
    let builder = PrometheusBuilder::new();
    let (recorder, exporter) = builder
        .idle_timeout(
            MetricKindMask::COUNTER | MetricKindMask::HISTOGRAM | MetricKindMask::GAUGE,
            Some(app_conf.metrics_idle_timeout),
        )
        .build()?;

    let prometheus_handle = recorder.handle();

    Stack::new(recorder)
        .push(TracingContextLayer::only_allow([
            "peripheral",
            "adapter",
            "characteristic",
            "scope",
            "service",
        ]))
        .install()?;

    let handle = tokio::runtime::Handle::try_current()?;
    handle.spawn(exporter);

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
                    get_connected_peripherals
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
        warn!("Main has ended: {result:?}");
        result??;
    }

    Ok(())
}
