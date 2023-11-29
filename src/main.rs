use std::sync::Arc;

use clap::Parser;
use fern::colors::{Color, ColoredLevelConfig};
use log::{info, warn};
use rocket::routes;
use tokio::task::JoinSet;

use crate::inner::adapter_manager::AdapterManager;
use crate::inner::args::Args;
use crate::inner::conf::manager::ConfigurationManager;
use crate::inner::conf::parse::CollectorConfigurationDto;
use crate::inner::controller::{adapters, configurations, data};
use crate::inner::error::CollectorResult;
use crate::inner::peripheral_manager::CharacteristicPayload;
use crate::inner::storage::Storage;

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
        .level_for("sensor_hub_ble_collector_rs", log::LevelFilter::Debug)
        .chain(std::io::stdout())
        .apply()?;
    Ok(())
}

pub(crate) struct CollectorState {
    pub(crate) configuration_manager: Arc<ConfigurationManager>,
    pub(crate) adapter_manager: Arc<AdapterManager>,
    pub(crate) storage: Arc<Storage>,
}

#[tokio::main]
async fn main() -> CollectorResult<()> {
    init_logging()?;

    let conf = CollectorConfigurationDto::try_from(Args::parse())?;
    let configuration_manager = Arc::new(ConfigurationManager::default());
    configuration_manager
        .add_peripherals(conf.peripherals)
        .await?;

    let (sender, receiver) = kanal::unbounded_async::<CharacteristicPayload>();

    let adapter_manager = Arc::new(AdapterManager::new(
        Arc::clone(&configuration_manager),
        sender,
    ));
    let storage = Arc::new(Storage::new());
    adapter_manager.init().await?;

    let mut join_set: JoinSet<CollectorResult<()>> = JoinSet::new();

    {
        let adapter_manager = adapter_manager.clone();
        join_set.spawn(async move {
            adapter_manager.start_discovery().await?;
            Ok(())
        });
    }

    {
        let storage = storage.clone();
        join_set.spawn_blocking(|| {
            let handle = std::thread::spawn(move || {
                storage.block_on_receiving(receiver.clone_sync());
            });
            let result = handle.join();
            warn!("Storage receiver has ended: {result:?}");
            Ok(())
        });
    }

    let collector_state = CollectorState {
        configuration_manager,
        adapter_manager,
        storage,
    };

    join_set.spawn(async move {
        rocket::build()
            .manage(collector_state)
            .mount("/ble", routes![adapters, configurations, data])
            .launch()
            .await?;
        Ok(())
    });

    if let Some(result) = join_set.join_next().await {
        info!("Ending everything: {result:?}");
    }

    Ok(())
}
