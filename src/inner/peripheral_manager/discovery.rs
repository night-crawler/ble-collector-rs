use crate::inner::debounce_limiter::DebounceLimiter;
use crate::inner::error::{CollectorError, CollectorResult};
use crate::inner::metrics::{CONNECTING_ERRORS, EVENT_COUNT, EVENT_THROTTLED_COUNT};
use crate::inner::model::peripheral_key::PeripheralKey;
use crate::inner::peripheral_manager::ext::CentralEventExt;
use crate::inner::peripheral_manager::PeripheralManager;
use btleplug::api::{Central, CentralEvent, ScanFilter};
use futures_util::StreamExt;
use std::sync::Arc;
use tokio::time::timeout;
use tracing::{debug, info, Span};

impl PeripheralManager {
    #[tracing::instrument(level="info", skip_all, parent = &self.span)]
    pub(crate) async fn start_discovery(self: Arc<Self>) -> CollectorResult<()> {
        self.adapter.start_scan(ScanFilter::default()).await?;

        let self_clone = Arc::clone(&self);
        let result = self_clone.discover_task().await;
        info!("Discovery task has ended: {result:?}");

        Err(CollectorError::EndOfStream)
    }

    async fn discover_task(self: Arc<Self>) -> CollectorResult<()> {
        loop {
            match self.clone().discover_task_internal().await {
                Ok(_) => break Ok(()),
                Err(CollectorError::TimeoutError(_)) => {
                    continue;
                }
                err => break err,
            }
        }
    }

    #[tracing::instrument(level="info", parent = &self.span, skip(self), err)]
    async fn discover_task_internal(self: Arc<Self>) -> CollectorResult<()> {
        let limiter = DebounceLimiter::new(
            self.app_conf.event_throttling_purge_samples,
            self.app_conf.event_throttling_purge_threshold,
            self.app_conf.event_throttling,
        );

        let mut stream = self.adapter.events().await?;
        while let Some(event) = timeout(self.app_conf.notification_stream_read_timeout, stream.next()).await? {
            debug!(?event, "Received CentralEvent");
            let peripheral_id = event.get_peripheral_id();
            let peripheral_key = Arc::new(self.build_peripheral_key(peripheral_id).await?);
            self.clone()
                .handle_single_event(event, &limiter, peripheral_key)
                .await?;
        }

        Err(CollectorError::EndOfStream)
    }

    #[tracing::instrument(level = "info", skip_all, fields(
    peripheral = % peripheral_key.peripheral_address,
    peripheral_name = ? peripheral_key.name,
    ))]
    async fn handle_single_event(
        self: Arc<Self>,
        event: CentralEvent,
        limiter: &DebounceLimiter<Arc<PeripheralKey>>,
        peripheral_key: Arc<PeripheralKey>,
    ) -> CollectorResult<()> {
        EVENT_COUNT.increment();
        let span = Span::current();

        match event {
            CentralEvent::DeviceDisconnected(_) => {
                let peripheral_manager = Arc::clone(&self);
                tokio::spawn(async move {
                    peripheral_manager.handle_disconnect(&peripheral_key, span).await?;
                    Ok::<_, anyhow::Error>(())
                });
            }
            _ => {
                if limiter.throttle(peripheral_key.clone()).await {
                    debug!("Throttled CentralEvent");
                    EVENT_THROTTLED_COUNT.increment();
                    return Ok(());
                };

                let Some(config) = self.configuration_manager.get_matching_config(&peripheral_key).await else {
                    return Ok(());
                };
                let peripheral_manager = Arc::clone(&self);
                tokio::spawn(async move {
                    if peripheral_manager
                        .clone()
                        .connect_all(peripheral_key, config, span.clone())
                        .await
                        .is_err()
                    {
                        CONNECTING_ERRORS.increment();
                    }
                });
            }
        }
        Ok(())
    }
}
