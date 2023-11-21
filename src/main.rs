mod inner;

use btleplug::api::{bleuuid::uuid_from_u16, Central, CentralEvent, Manager as _, Peripheral as _, ScanFilter, WriteType};
use btleplug::platform::{Adapter, Manager, Peripheral};
use rand::{Rng, thread_rng};
use std::error::Error;
use std::sync::Arc;
use std::thread;
use std::time::Duration;
use clap::Parser;
use futures_util::{select, StreamExt};
use lazy_static::lazy_static;
use tokio::task::{JoinError, JoinHandle};
use tokio::time;
use uuid::Uuid;
use crate::inner::error::CollectorError;
use tokio::task::JoinSet;
use log::{error, info};
use rocket::{get, routes};
use crate::inner::adapter_manager::{ADAPTER_MANAGER, AdapterManager};
use crate::inner::adapter_service_manager::AdapterServiceManager;
use crate::inner::args::Args;
use crate::inner::configuration::{CollectorConfiguration, CONFIGURATION_MANAGER};

use crate::inner::controller::{adapters, index};


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
            .mount("/hello", routes![index])
            .mount("/lol", routes![adapters])
            .launch()
            .await?;
        Ok(())
    });

    if let Some(result) = join_set.join_next().await {
        info!("Ending everything: {result:?}");
    }

    Ok(())
}
