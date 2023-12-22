use std::sync::Arc;

use clap::Parser;
use rumqttc::v5::MqttOptions;
use tokio::task::JoinSet;
use tracing::warn;

use inner::process::api_publisher::ApiPublisher;

use crate::init::{init_mqtt, init_multi_publisher, init_prometheus, init_rocket, init_tracing};
use crate::inner::adapter_manager::AdapterManager;
use crate::inner::conf::cmd_args::AppConf;
use crate::inner::conf::dto::collector_configuration::CollectorConfigurationDto;
use crate::inner::conf::manager::ConfigurationManager;
use crate::inner::model::characteristic_payload::CharacteristicPayload;
use crate::inner::process::metric_publisher::MetricPublisher;
use crate::inner::process::FanOutSender;

mod init;
mod inner;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let mut join_set: JoinSet<anyhow::Result<()>> = JoinSet::new();
    init_tracing()?;

    let app_conf = Arc::new(AppConf::parse());
    let prometheus_handle = init_prometheus(app_conf.metrics_idle_timeout)?;

    let collector_conf = CollectorConfigurationDto::try_from(app_conf.as_ref())?;
    let configuration_manager = Arc::new(ConfigurationManager::default());
    configuration_manager
        .add_peripherals(collector_conf.peripherals)
        .await?;

    let (payload_sender, payload_receiver) = kanal::unbounded_async::<Arc<CharacteristicPayload>>();
    let mut fanout_sender = FanOutSender::new(vec![payload_sender]);

    match MqttOptions::try_from(app_conf.as_ref()) {
        Ok(opts) => {
            let (mqtt_sender, mqtt_receiver) =
                kanal::unbounded_async::<Arc<CharacteristicPayload>>();
            fanout_sender.add(mqtt_sender);
            init_mqtt(opts, mqtt_receiver, app_conf.mqtt_cap, &mut join_set).await?;
        }
        Err(error) => {
            warn!(%error, "Failed to create an MQTT client");
        }
    }

    let adapter_manager = Arc::new(AdapterManager::new(
        Arc::clone(&configuration_manager),
        fanout_sender,
        Arc::clone(&app_conf),
    ));
    adapter_manager.init().await?;

    let api_publisher = Arc::new(ApiPublisher::new());
    let metric_publisher = Arc::new(MetricPublisher::new());
    let multi_publisher = init_multi_publisher(
        &api_publisher,
        &metric_publisher,
        payload_receiver.clone_sync(),
    );

    {
        let adapter_manager = adapter_manager.clone();
        join_set.spawn(async move {
            adapter_manager.start_discovery().await?;
            Ok(())
        });
    }
    {
        let sync_multi_publisher = multi_publisher.clone();
        join_set.spawn_blocking(|| {
            let handle = std::thread::spawn(move || {
                sync_multi_publisher.block_on_receiving();
            });
            let result = handle.join();
            warn!("Storage receiver has ended: {result:?}");
            Ok(())
        });
    }



    join_set.spawn(async move {
        init_rocket(
            configuration_manager,
            adapter_manager,
            api_publisher,
            prometheus_handle,
            app_conf.listen_address,
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
