use std::net::SocketAddr;
use std::sync::Arc;
use std::time::Duration;

use console_subscriber::ConsoleLayer;
use futures_util::StreamExt;
use kanal::AsyncReceiver;
use metrics_exporter_prometheus::{PrometheusBuilder, PrometheusHandle};
use metrics_tracing_context::{MetricsLayer, TracingContextLayer};
use metrics_util::layers::Stack;
use metrics_util::MetricKindMask;
use rocket::{Build, Rocket, routes};
use rumqttc::v5::MqttOptions;
use tokio::task::JoinSet;
use tracing::debug;
use tracing_subscriber::EnvFilter;
use tracing_subscriber::layer::SubscriberExt;
use tracing_subscriber::util::SubscriberInitExt;

use crate::inner::adapter_manager::AdapterManager;
use crate::inner::api::{
    describe_adapters, get_collector_data, get_connected_peripherals, get_metrics, list_adapters,
    list_configurations, read_write_characteristic,
};
use crate::inner::conf::manager::ConfigurationManager;
use crate::inner::error::CollectorError;
use crate::inner::metrics::describe_metrics;
use crate::inner::model::characteristic_payload::CharacteristicPayload;
use crate::inner::process::api_publisher::ApiPublisher;
use crate::inner::process::metric_publisher::MetricPublisher;
use crate::inner::process::PublishPayload;
use crate::inner::process::multi_publisher::MultiPublisher;

pub(super) fn init_tracing() -> anyhow::Result<()> {
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

    Ok(())
}

pub(super) fn init_prometheus(idle_timeout: Duration) -> anyhow::Result<PrometheusHandle> {
    let builder = PrometheusBuilder::new();
    let (recorder, exporter) = builder
        .idle_timeout(
            MetricKindMask::COUNTER | MetricKindMask::HISTOGRAM | MetricKindMask::GAUGE,
            Some(idle_timeout),
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

    Ok(prometheus_handle)
}

pub(super) fn init_multi_publisher(
    api_publisher: &Arc<ApiPublisher>,
    metric_publisher: &Arc<MetricPublisher>,
    payload_receiver: kanal::Receiver<Arc<CharacteristicPayload>>,
) -> Arc<MultiPublisher> {
    let api_publisher = Arc::clone(api_publisher);
    let payload_storage_processor: Arc<dyn PublishPayload + Sync + Send> = api_publisher;

    let metric_publisher = Arc::clone(metric_publisher);
    let payload_metric_publisher: Arc<dyn PublishPayload + Sync + Send> = metric_publisher;

    Arc::new(MultiPublisher::new(
        payload_receiver,
        vec![payload_storage_processor, payload_metric_publisher],
    ))
}

pub(super) fn init_rocket(
    configuration_manager: Arc<ConfigurationManager>,
    adapter_manager: Arc<AdapterManager>,
    api_publisher: Arc<ApiPublisher>,
    prometheus_handle: PrometheusHandle,
    listen_address: SocketAddr,
) -> Rocket<Build> {
    rocket::build()
        .manage(configuration_manager)
        .manage(adapter_manager)
        .manage(api_publisher)
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
                .merge(("address", Arc::new(listen_address.ip().to_string())))
                .merge(("port", listen_address.port())),
        )
}

pub(super) async fn init_mqtt(
    opts: MqttOptions,
    payload_receiver: AsyncReceiver<Arc<CharacteristicPayload>>,
    cap: usize,
    join_set: &mut JoinSet<anyhow::Result<()>>,
) -> anyhow::Result<()> {
    let (mqtt_client, mut event_loop) = rumqttc::v5::AsyncClient::new(opts, cap);

    join_set.spawn(async move {
        let mut stream = payload_receiver.stream();
        while let Some(payload) = stream.next().await {
            let Some(mqtt_conf) = payload.conf.publish_mqtt() else {
                continue;
            };
            let data = serde_json::to_string(&payload.value)?;
            mqtt_client
                .publish(
                    mqtt_conf.topic.as_str(),
                    mqtt_conf.qos(),
                    mqtt_conf.retain,
                    data,
                )
                .await?;
        }

        Err::<(), anyhow::Error>(CollectorError::EndOfStream.into())
    });

    join_set.spawn(async move {
        loop {
            let event = event_loop.poll().await?;
            debug!(?event, "MQTT event");
        }
    });

    Ok(())
}
