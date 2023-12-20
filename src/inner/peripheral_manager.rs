use std::collections::HashMap;
use std::fmt::{Display, Formatter};
use std::sync::Arc;
use std::time::Duration;

use anyhow::Context;
use btleplug::api::{BDAddr, Central, CentralEvent, Characteristic, Peripheral as _, ScanFilter};
use btleplug::platform::{Adapter, Peripheral, PeripheralId};
use futures_util::{stream, StreamExt};
use kanal::AsyncSender;
use retainer::Cache;
use tokio::sync::Mutex;
use tokio::task::JoinHandle;
use tokio::time::timeout;
use tracing::{info, info_span, warn, Span};

use crate::inner::conf::cmd_args::AppConf;
use crate::inner::conf::manager::ConfigurationManager;
use crate::inner::conf::model::characteristic_config::CharacteristicConfig;
use crate::inner::conf::model::flat_peripheral_config::FlatPeripheralConfig;
use crate::inner::conf::model::service_characteristic_key::ServiceCharacteristicKey;
use crate::inner::debounce_limiter::DebounceLimiter;
use crate::inner::error::{CollectorError, CollectorResult};
use crate::inner::metrics::{
    CONNECTED_PERIPHERALS, CONNECTING_DURATION, CONNECTING_ERRORS, CONNECTIONS_DROPPED,
    CONNECTIONS_HANDLED, CONNECTION_DURATION, EVENT_COUNT, EVENT_THROTTLED_COUNT,
    SERVICE_DISCOVERY_DURATION, TOTAL_CONNECTING_DURATION,
};
use crate::inner::model::characteristic_payload::CharacteristicPayload;
use crate::inner::model::connected_peripherals::ConnectedPeripherals;
use crate::inner::model::fqcn::Fqcn;
use crate::inner::model::peripheral_key::PeripheralKey;

pub(crate) struct PeripheralManager {
    pub(crate) adapter: Arc<Adapter>,
    peripheral_cache: Arc<Cache<BDAddr, Arc<Peripheral>>>,
    peripheral_cache_updated_at: Mutex<std::time::Instant>,
    cache_monitor: JoinHandle<()>,
    poll_handle_map: Arc<Mutex<HashMap<Arc<Fqcn>, JoinHandle<()>>>>,
    subscription_map: Arc<Mutex<HashMap<BDAddr, JoinHandle<()>>>>,
    subscribed_characteristics: Arc<Mutex<HashMap<Arc<Fqcn>, Arc<CharacteristicConfig>>>>,
    payload_sender: AsyncSender<CharacteristicPayload>,
    configuration_manager: Arc<ConfigurationManager>,
    pub(crate) app_conf: Arc<AppConf>,
    span: Span,
}

struct ConnectionContext {
    peripheral: Arc<Peripheral>,
    characteristic: Characteristic,
    characteristic_config: Arc<CharacteristicConfig>,
    fqcn: Arc<Fqcn>,
    peripheral_config: Arc<FlatPeripheralConfig>,
}

impl Display for ConnectionContext {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        let name = self
            .characteristic_config
            .name()
            .unwrap_or(Arc::new("".to_string()));
        write!(
            f,
            "{}[{}]::{}/{name}[{}]",
            self.peripheral_config.name,
            self.peripheral.address(),
            self.characteristic.service_uuid,
            self.characteristic.uuid
        )
    }
}

impl Drop for PeripheralManager {
    fn drop(&mut self) {
        self.cache_monitor.abort();
    }
}

impl PeripheralManager {
    pub(crate) fn new(
        adapter: Adapter,
        sender: AsyncSender<CharacteristicPayload>,
        configuration_manager: Arc<ConfigurationManager>,
        app_conf: Arc<AppConf>,
        span: Span,
    ) -> Self {
        let cache = Arc::new(Cache::new());
        let clone = cache.clone();

        let monitor =
            tokio::spawn(async move { clone.monitor(10, 0.25, Duration::from_secs(10)).await });

        Self {
            adapter: Arc::new(adapter),
            peripheral_cache_updated_at: Mutex::new(
                std::time::Instant::now() - app_conf.peripheral_cache_ttl,
            ),
            peripheral_cache: cache,
            cache_monitor: monitor,
            poll_handle_map: Default::default(),
            subscription_map: Default::default(),
            subscribed_characteristics: Default::default(),
            payload_sender: sender,
            configuration_manager,
            app_conf,
            span,
        }
    }

    async fn get_cached_peripheral(&self, address: &BDAddr) -> Option<Arc<Peripheral>> {
        if let Some(p) = self.peripheral_cache.get(address).await {
            return Some(Arc::clone(&p));
        }
        None
    }
    #[tracing::instrument(level="info", parent = &self.span, skip(self))]
    pub(crate) async fn get_peripheral(&self, peripheral: &BDAddr, ) -> CollectorResult<Option<Arc<Peripheral>>> {
        match self.get_cached_peripheral(peripheral).await {
            None => self.populate_cache().await?,
            existing => return Ok(existing),
        }
        Ok(self.get_cached_peripheral(peripheral).await)
    }

    #[tracing::instrument(level="info", skip(self), err)]
    async fn discover_services(&self, peripheral: &Peripheral) -> CollectorResult<()> {
        SERVICE_DISCOVERY_DURATION
            .measure(|| peripheral.discover_services())
            .await?;
        Ok(())
    }

    #[tracing::instrument(level="info", skip(self), err)]
    pub(crate) async fn populate_cache(&self) -> CollectorResult<()> {
        let mut update_at = self.peripheral_cache_updated_at.lock().await;
        if update_at.elapsed() < self.app_conf.peripheral_cache_ttl {
            return Ok(());
        }
        *update_at = std::time::Instant::now();

        info!("Cache miss");

        let caching_results = stream::iter(self.adapter.peripherals().await?)
            .map(|peripheral| async {
                self.discover_services(&peripheral).await?;
                self.peripheral_cache
                    .insert(
                        peripheral.address(),
                        Arc::new(peripheral),
                        self.app_conf.peripheral_cache_ttl,
                    )
                    .await;

                Ok::<(), CollectorError>(())
            })
            .buffer_unordered(self.app_conf.service_discovery_parallelism)
            .collect::<Vec<_>>()
            .await;

        for result in caching_results {
            result?;
        }
        Ok(())
    }

    #[tracing::instrument(level="info", skip_all, parent = &self.span)]
    pub(crate) async fn start_discovery(self: Arc<Self>) -> CollectorResult<()> {
        self.adapter.start_scan(ScanFilter::default()).await?;

        let self_clone = Arc::clone(&self);
        let result = self_clone.discover_task().await;
        info!("Discovery task has ended: {result:?}");

        Err(CollectorError::EndOfStream)
    }

    #[tracing::instrument(level="info", skip_all, err)]
    async fn connect(&self, peripheral: &Peripheral) -> CollectorResult<()> {
        CONNECTING_DURATION
            .measure(|| {
                timeout(
                    self.app_conf.peripheral_connect_timeout,
                    peripheral.connect(),
                )
            })
            .await??;
        info!("Connected to peripheral");
        Ok(())
    }

    #[tracing::instrument(level="info", skip_all, parent = &_parent_span, err)]
    pub(crate) async fn connect_all_matching(
        self: Arc<Self>,
        peripheral_key: &PeripheralKey,
        peripheral_config: Arc<FlatPeripheralConfig>,
        _parent_span: Span,
    ) -> CollectorResult<()> {
        let peripheral = self
            .get_peripheral(&peripheral_key.peripheral_address)
            .await?
            .with_context(|| format!("Failed to get peripheral: {:?}", peripheral_key))?;

        if !peripheral.is_connected().await? {
            self.connect(&peripheral).await?;
        }

        if peripheral.services().is_empty() {
            info!("Forcing service discovery for peripheral {peripheral_key}");
            SERVICE_DISCOVERY_DURATION
                .measure(|| peripheral.discover_services())
                .await?;
            info!("Forced service discovery for peripheral {peripheral_key} completed");
        }

        for characteristic in peripheral
            .services()
            .into_iter()
            .flat_map(|service| service.characteristics.into_iter())
        {
            let char_key = ServiceCharacteristicKey::from(&characteristic);
            let Some(conf) = peripheral_config.service_map.get(&char_key).cloned() else {
                continue;
            };

            let fqcn = Fqcn {
                peripheral_address: peripheral_key.peripheral_address,
                service_uuid: characteristic.service_uuid,
                characteristic_uuid: characteristic.uuid,
            };

            if self.check_characteristic_is_handled(&fqcn).await {
                continue;
            };

            let ctx = ConnectionContext {
                peripheral: Arc::clone(&peripheral),
                characteristic,
                characteristic_config: conf,
                fqcn: Arc::new(fqcn),
                peripheral_config: Arc::clone(&peripheral_config),
            };

            TOTAL_CONNECTING_DURATION
                .measure(|| self.clone().handle_connect(ctx))
                .await?;
        }

        let num_connected = self.get_all_connected_peripherals().await.get_all().len() as f64;
        info_span!(
            parent: None, "CONNECTED_PERIPHERALS", adapter = peripheral_key.adapter_id
        ).in_scope(|| CONNECTED_PERIPHERALS.gauge(num_connected));

        Ok(())
    }

    async fn check_characteristic_is_handled(&self, fqcn: &Fqcn) -> bool {
        self.poll_handle_map.lock().await.get(fqcn).is_some()
            || self
                .subscribed_characteristics
                .lock()
                .await
                .get(fqcn)
                .is_some()
    }

    #[tracing::instrument(level="info", skip_all, fields(
        service = %ctx.fqcn.service_uuid,
        characteristic = %ctx.fqcn.characteristic_uuid,
    ))]
    async fn handle_connect(self: Arc<Self>, ctx: ConnectionContext) -> CollectorResult<()> {
        info!("Connecting");
        let fqcn = ctx.fqcn.clone();
        let self_clone = Arc::clone(&self);

        let span = Span::current();

        match ctx.characteristic_config.as_ref() {
            CharacteristicConfig::Subscribe { .. } => {
                self.subscribe(&ctx).await?;
                self.subscription_map
                    .lock()
                    .await
                    .entry(ctx.fqcn.peripheral_address)
                    .or_insert_with(|| {
                        tokio::spawn(async move {
                            let _ = CONNECTION_DURATION
                                .measure(|| self_clone.clone().block_on_notifying(ctx, span))
                                .await;
                            self_clone.abort_subscription(fqcn.clone()).await;
                        })
                    });
            }
            CharacteristicConfig::Poll { .. } => {
                self.poll_handle_map
                    .lock()
                    .await
                    .entry(fqcn.clone())
                    .or_insert_with(|| {
                        tokio::spawn(async move {
                            let _ = CONNECTION_DURATION
                                .measure(|| self_clone.clone().block_on_polling(ctx, span))
                                .await;
                            self_clone.abort_polling(fqcn.clone()).await;
                        })
                    });
            }
        }
        Ok(())
    }

    async fn abort_subscription(&self, fqcn: Arc<Fqcn>) {
        let mut subscribed_characteristics = self.subscribed_characteristics.lock().await;
        subscribed_characteristics
            .retain(|present_tk, _| present_tk.peripheral_address != fqcn.peripheral_address);

        if let Some(handle) = self
            .subscription_map
            .lock()
            .await
            .remove(&fqcn.peripheral_address)
        {
            handle.abort();
            warn!("Aborted subscription");
        } else {
            warn!("Can't abort subscription: no handle found");
        }
    }

    async fn abort_polling(&self, fqcn: Arc<Fqcn>) {
        if let Some(handle) = self.poll_handle_map.lock().await.remove(&fqcn) {
            handle.abort();
            warn!("Aborted polling");
        } else {
            warn!("Can't abort polling: no handle found");
        }
    }

    async fn get_characteristic_conf(&self, fqcn: &Fqcn) -> Option<Arc<CharacteristicConfig>> {
        self.subscribed_characteristics
            .lock()
            .await
            .get(fqcn)
            .cloned()
    }

    async fn subscribe(&self, ctx: &ConnectionContext) -> CollectorResult<()> {
        let mut subscribed_characteristics = self.subscribed_characteristics.lock().await;

        match subscribed_characteristics.entry(ctx.fqcn.clone()) {
            std::collections::hash_map::Entry::Occupied(_) => {
                warn!("Already subscribed");
                return Ok(());
            }
            std::collections::hash_map::Entry::Vacant(entry) => {
                entry.insert(ctx.characteristic_config.clone());
            }
        }
        drop(subscribed_characteristics);

        ctx.peripheral.subscribe(&ctx.characteristic).await?;
        let existing_connections = self.get_all_connected_peripherals().await;
        info!(%existing_connections, "Subscribed on characteristic");

        Ok(())
    }
}

impl PeripheralManager {
    #[tracing::instrument(level="info", skip_all, parent = &_parent_span, err)]
    async fn block_on_polling(self: Arc<Self>, ctx: ConnectionContext, _parent_span: Span) -> CollectorResult<()> {
        info!("Polling characteristic");

        let CharacteristicConfig::Poll {
            delay_sec,
            ref converter,
            ..
        } = ctx.characteristic_config.as_ref()
        else {
            return Err(CollectorError::UnexpectedCharacteristicConfiguration(
                ctx.characteristic_config.clone(),
            ));
        };

        loop {
            let value = ctx.peripheral.read(&ctx.characteristic).await?;
            let value = converter.convert(value)?;
            let value = CharacteristicPayload {
                created_at: chrono::offset::Utc::now(),
                value,
                fqcn: ctx.fqcn.clone(),
                conf: Arc::clone(&ctx.characteristic_config),
            };
            self.payload_sender.send(value).await?;
            tokio::time::sleep(*delay_sec).await;
        }
    }

    #[tracing::instrument(level="info", skip_all, parent = &_parent_span, err)]
    async fn block_on_notifying(self: Arc<Self>, ctx: ConnectionContext, _parent_span: Span) -> CollectorResult<()> {
        info!("Subscribing to notifications");
        let mut notification_stream = ctx.peripheral.notifications().await?;

        while let Some(event) = notification_stream.next().await {
            let fqcn = Arc::new(ctx.fqcn.with_characteristic(event.service_uuid, event.uuid));
            let Some(conf) = self.get_characteristic_conf(&fqcn).await else {
                // warn!("No conf found for characteristic: {fqcn}; {:?}", ctx.peripheral);
                continue;
            };
            let CharacteristicConfig::Subscribe { converter, .. } = conf.as_ref() else {
                return Err(CollectorError::UnexpectedCharacteristicConfiguration(conf));
            };

            let value = converter.convert(event.value)?;
            let value = CharacteristicPayload {
                created_at: chrono::offset::Utc::now(),
                value,
                fqcn,
                conf,
            };
            self.payload_sender.send(value).await?;
        }

        Err(CollectorError::EndOfStream)
    }
}

impl PeripheralManager {
    pub(crate) async fn get_peripheral_characteristic(
        &self,
        fqcn: &Fqcn,
    ) -> CollectorResult<(Arc<Peripheral>, Characteristic)> {
        let peripheral = self
            .get_peripheral(&fqcn.peripheral_address)
            .await?
            .with_context(|| format!("Failed to get peripheral: {fqcn}"))?;

        if !peripheral.is_connected().await? {
            info!("Connecting to {fqcn}");
            CONNECTING_DURATION
                .measure(|| {
                    timeout(
                        self.app_conf.peripheral_connect_timeout,
                        peripheral.connect(),
                    )
                })
                .await??;
        }

        let service = peripheral
            .services()
            .into_iter()
            .find(|service| service.uuid == fqcn.service_uuid)
            .with_context(|| format!("Failed to find service {fqcn}"))?;

        let characteristic = service
            .characteristics
            .into_iter()
            .find(|characteristic| characteristic.uuid == fqcn.characteristic_uuid)
            .with_context(|| format!("Failed to find characteristic {fqcn}",))?;

        Ok((peripheral, characteristic))
    }

    pub(crate) async fn disconnect_if_has_no_tasks(
        &self,
        peripheral: Arc<Peripheral>,
    ) -> CollectorResult<()> {
        let poll_handle_map = self.poll_handle_map.lock().await;
        let subscription_map = self.subscription_map.lock().await;
        let peripheral_address = peripheral.address();
        if subscription_map.contains_key(&peripheral_address) {
            return Ok(());
        }
        if poll_handle_map
            .keys()
            .any(|fqcn| fqcn.peripheral_address == peripheral_address)
        {
            return Ok(());
        }

        peripheral.disconnect().await?;

        Ok(())
    }

    #[tracing::instrument(level="info", skip_all, parent = &_parent_span)]
    pub(crate) async fn handle_disconnect(&self, peripheral_key: &PeripheralKey, _parent_span: Span) {
        CONNECTIONS_DROPPED.increment();

        {
            let mut poll_handle_map = self.poll_handle_map.lock().await;
            let mut subscription_map = self.subscription_map.lock().await;
            let mut subscribed_characteristic = self.subscribed_characteristics.lock().await;

            self.peripheral_cache
                .remove(&peripheral_key.peripheral_address)
                .await;

            subscribed_characteristic
                .retain(|fqcn, _| fqcn.peripheral_address != peripheral_key.peripheral_address);

            poll_handle_map.retain(|fqcn, handle| {
                if fqcn.peripheral_address == peripheral_key.peripheral_address {
                    handle.abort();
                    false
                } else {
                    true
                }
            });
            subscription_map.retain(|address, handle| {
                if *address == peripheral_key.peripheral_address {
                    handle.abort();
                    false
                } else {
                    true
                }
            });
        }

        let existing_peripherals = self.get_all_connected_peripherals().await;
        warn!(%existing_peripherals,"Device disconnected");
    }

    pub(crate) async fn get_all_connected_peripherals(&self) -> ConnectedPeripherals {
        let poll_handle_map = self.poll_handle_map.lock().await;
        let subscription_map = self.subscription_map.lock().await;
        let subscribed_characteristic = self.subscribed_characteristics.lock().await;

        ConnectedPeripherals::new(
            poll_handle_map.keys().map(|fqcn| fqcn.peripheral_address),
            subscription_map.keys().cloned(),
            subscribed_characteristic
                .keys()
                .map(|fqcn| fqcn.peripheral_address),
        )
    }
}

impl PeripheralManager {
    async fn build_peripheral_key(
        &self,
        peripheral_id: &PeripheralId,
    ) -> CollectorResult<PeripheralKey> {
        let mut peripheral_key = PeripheralKey::try_from(peripheral_id)?;
        if let Some(peripheral) = self
            .get_peripheral(&peripheral_key.peripheral_address)
            .await?
        {
            if let Some(props) = peripheral.properties().await? {
                peripheral_key.name = props.local_name;
            }
        }

        Ok(peripheral_key)
    }

    pub(crate) async fn discover_task(self: Arc<Self>) -> CollectorResult<()> {
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
        while let Some(event) = timeout(
            self.app_conf.notification_stream_read_timeout,
            stream.next(),
        )
        .await?
        {
            let peripheral_id = event.get_peripheral_id();
            let peripheral_key = Arc::new(self.build_peripheral_key(peripheral_id).await?);
            self.clone()
                .handle_single_event(event, &limiter, peripheral_key)
                .await?;
        }

        Err(CollectorError::EndOfStream)
    }

    #[tracing::instrument(level="info", skip_all, fields(
        peripheral = %peripheral_key.peripheral_address,
        peripheral_name = ?peripheral_key.name,
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
                    peripheral_manager.handle_disconnect(&peripheral_key, span).await;
                });
            }
            _ => {
                if limiter.throttle(peripheral_key.clone()).await {
                    EVENT_THROTTLED_COUNT.increment();
                    return Ok(());
                };

                let Some(config) = self
                    .configuration_manager
                    .get_matching_config(&peripheral_key)
                    .await
                else {
                    return Ok(());
                };
                let peripheral_manager = Arc::clone(&self);
                tokio::spawn(async move {
                    CONNECTIONS_HANDLED.increment();
                    if peripheral_manager
                        .clone()
                        .connect_all_matching(&peripheral_key, config, span.clone())
                        .await.is_err()
                    {
                        CONNECTING_ERRORS.increment();
                    }
                });
            }
        }
        Ok(())
    }
}

trait CentralEventExt {
    fn get_peripheral_id(&self) -> &PeripheralId;
}

impl CentralEventExt for CentralEvent {
    fn get_peripheral_id(&self) -> &PeripheralId {
        match self {
            CentralEvent::DeviceDiscovered(id)
            | CentralEvent::DeviceUpdated(id)
            | CentralEvent::DeviceConnected(id)
            | CentralEvent::DeviceDisconnected(id)
            | CentralEvent::ManufacturerDataAdvertisement { id, .. }
            | CentralEvent::ServiceDataAdvertisement { id, .. }
            | CentralEvent::ServicesAdvertisement { id, .. } => id,
        }
    }
}
