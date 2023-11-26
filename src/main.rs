use std::error::Error;

use clap::Parser;
use fern::colors::{Color, ColoredLevelConfig};
use log::info;
use rocket::routes;
use tokio::task::JoinSet;

use crate::inner::adapter_manager::ADAPTER_MANAGER;
use crate::inner::args::Args;
use crate::inner::conf::manager::CONFIGURATION_MANAGER;
use crate::inner::conf::parse::CollectorConfigurationDto;
use crate::inner::controller::{adapters, configurations};

mod inner;

#[tokio::main]
async fn main() -> Result<(), Box<dyn Error>> {
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

    let conf = CollectorConfigurationDto::try_from(Args::parse())?;
    CONFIGURATION_MANAGER
        .add_peripherals(conf.peripherals)
        .await?;

    ADAPTER_MANAGER.init().await?;

    let mut join_set: JoinSet<anyhow::Result<()>> = JoinSet::new();
    join_set.spawn(async move {
        ADAPTER_MANAGER.start_discovery().await?;
        Ok(())
    });
    join_set.spawn(async move {
        rocket::build()
            .mount("/ble", routes![adapters, configurations])
            .launch()
            .await?;
        Ok(())
    });

    if let Some(result) = join_set.join_next().await {
        info!("Ending everything: {result:?}");
    }

    Ok(())
}
