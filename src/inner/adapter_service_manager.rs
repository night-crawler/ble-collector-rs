use btleplug::api::{bleuuid::uuid_from_u16, Central, CentralEvent, Manager as _, Peripheral as _, ScanFilter, WriteType};
use btleplug::platform::{Adapter, Manager, Peripheral};
use rand::{Rng, thread_rng};
use std::error::Error;
use std::sync::Arc;
use std::thread;
use std::time::Duration;
use clap::Parser;
use futures_util::{select, StreamExt};
use tokio::task::{JoinError, JoinHandle};
use tokio::time;
use uuid::Uuid;
use crate::inner::error::CollectorError;
use tokio::task::JoinSet;
use log::{error, info};
use rocket::{get, routes};
use crate::inner::args::Args;
use crate::inner::configuration::CollectorConfiguration;


pub(crate) struct AdapterServiceManager {
    pub(crate) adapter: Arc<Adapter>,
    // discovery_events: kanal::AsyncReceiver<CentralEvent>
}


impl AdapterServiceManager {
    pub(crate) fn new(adapter: Adapter) -> Self {
        Self {
            adapter: Arc::new(adapter),
        }
    }

    pub(crate) async fn start_discovery(&self) -> btleplug::Result<()> {
        self.adapter.start_scan(ScanFilter::default()).await?;
        let adapter = Arc::clone(&self.adapter);

        let (sender, receiver) = kanal::unbounded_async::<CentralEvent>();

        let mut join_set = JoinSet::new();
        join_set.spawn(async move {
            discover_task(adapter, sender).await
        });

        join_set.spawn(async move {
            process_events(receiver).await
        });

        if let Some(result) = join_set.join_next().await {
            info!("Ending everything: {result:?}");
        }

        Ok(())
    }
}


async fn discover_task(adapter: Arc<Adapter>, sender: kanal::AsyncSender<CentralEvent>) -> Result<(), CollectorError> {
    let mut stream =  adapter.events().await?;
    while let Some(event) = stream.next().await {
        sender.send(event).await?;
    }
    Err(CollectorError::EndOfStream)
}

async fn process_events(receiver: kanal::AsyncReceiver<CentralEvent>) -> Result<(), CollectorError> {
    while let Some(event) = receiver.stream().next().await {
        // info!("Event: {event:?}");
    }

    Err(CollectorError::EndOfStream)
}
