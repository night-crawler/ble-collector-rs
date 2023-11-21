use std::error::Error;

use clap::Parser;
use log::info;
use rocket::routes;
use tokio::task::JoinSet;

use crate::inner::adapter_manager::ADAPTER_MANAGER;
use crate::inner::args::Args;
use crate::inner::configuration::{CollectorConfiguration, CONFIGURATION_MANAGER};
use crate::inner::controller::{adapters, configurations};

mod inner;

#[tokio::main]
async fn main() -> Result<(), Box<dyn Error>> {
    env_logger::builder()
        .filter_level(log::LevelFilter::Info)
        // .format_target(false)
        // .format_timestamp(None)
        .init();

    let conf = CollectorConfiguration::try_from(Args::parse())?;
    CONFIGURATION_MANAGER.add_services(conf.services).await;

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
