use std::collections::hash_map::Entry::{Occupied, Vacant};
use std::sync::Arc;

use anyhow::Context;
use btleplug::api::Peripheral as _;
use btleplug::platform::Peripheral;
use futures_util::StreamExt;
use tokio::time::timeout;
use tracing::{debug, info, info_span, warn, Span};

use crate::inner::conf::model::characteristic_config::CharacteristicConfig;
use crate::inner::conf::model::flat_peripheral_config::FlatPeripheralConfig;
use crate::inner::error::{CollectorError, CollectorResult};
use crate::inner::metrics::measure_execution_time::Measure;
use crate::inner::metrics::{
    CONNECTED_PERIPHERALS, CONNECTING_DURATION, CONNECTIONS_DROPPED, CONNECTIONS_HANDLED, CONNECTION_DURATION,
    TOTAL_CONNECTING_DURATION,
};
use crate::inner::model::characteristic_payload::CharacteristicPayload;
use crate::inner::model::collector_event::CollectorEvent;
use crate::inner::model::connect_peripheral_request::ConnectPeripheralRequest;
use crate::inner::model::fqcn::Fqcn;
use crate::inner::model::peripheral_key::PeripheralKey;
use crate::inner::peripheral_manager::connection_context::ConnectionContext;
use crate::inner::peripheral_manager::PeripheralManager;

impl PeripheralManager {
    #[tracing::instrument(level = "info", skip_all, parent = & _parent_span, err)]
    pub(super) async fn connect_all(
        self: Arc<Self>,
        peripheral_key: Arc<PeripheralKey>,
        peripheral_config: Arc<FlatPeripheralConfig>,
        _parent_span: Span,
    ) -> CollectorResult<()> {
        info!("Connecting to all available peripheral characteristics");
        CONNECTIONS_HANDLED.increment();

        let peripheral = self
            .get_peripheral(&peripheral_key.peripheral_address)
            .await?
            .with_context(|| format!("Failed to get peripheral: {:?}", peripheral_key))?;

        self.connect(&peripheral).await?;

        for characteristic in peripheral
            .services()
            .into_iter()
            .flat_map(|service| service.characteristics.into_iter())
        {
            let Some(characteristic_config) = peripheral_config.get_conf(&characteristic) else {
                continue;
            };

            let fqcn = Arc::new(Fqcn {
                peripheral: peripheral_key.peripheral_address,
                service: characteristic.service_uuid,
                characteristic: characteristic.uuid,
            });

            if self.check_characteristic_is_handled(fqcn.as_ref()).await {
                continue;
            };

            self.fanout_sender
                .send(CollectorEvent::Connect(ConnectPeripheralRequest {
                    peripheral_key: peripheral_key.clone(),
                    fqcn: fqcn.clone(),
                    conf: characteristic_config.clone(),
                }))
                .await?;

            let ctx = ConnectionContext {
                peripheral: Arc::clone(&peripheral),
                characteristic,
                characteristic_config,
                fqcn,
                peripheral_config: Arc::clone(&peripheral_config),
            };

            self.clone()
                .spawn(ctx)
                .measure_execution_time(TOTAL_CONNECTING_DURATION, Span::current())
                .await?;
        }

        let num_connected = self.get_all_connected_peripherals().await.get_all().len() as f64;
        info_span!(
            parent: None, "CONNECTED_PERIPHERALS", adapter = peripheral_key.adapter_id
        )
        .in_scope(|| CONNECTED_PERIPHERALS.gauge(num_connected));

        Ok(())
    }

    #[tracing::instrument(level = "info", skip_all, fields(
    service = % ctx.fqcn.service,
    characteristic = % ctx.fqcn.characteristic,
    ))]
    async fn spawn(self: Arc<Self>, ctx: ConnectionContext) -> CollectorResult<()> {
        info!("Spawning subscription / polling tasks");
        let fqcn = ctx.fqcn.clone();
        let self_clone = Arc::clone(&self);

        let parent_span = Span::current();

        match ctx.characteristic_config.as_ref() {
            CharacteristicConfig::Subscribe { .. } => {
                self.subscribe(&ctx).await?;
                // we subscribe only once, the remainder is handled by adding elements to the
                // subscribed_characteristics
                self.subscription_map
                    .lock()
                    .await
                    .entry(ctx.fqcn.peripheral)
                    .or_insert_with(|| {
                        let span = info_span!(parent: self.span.clone(), "block_on_notifying", spawn_type = "notify", peripheral = % ctx.fqcn.peripheral);
                        tokio::spawn(async move {
                            let _ = self_clone
                                .clone()
                                .block_on_notifying(ctx, parent_span)
                                .measure_execution_time(CONNECTION_DURATION, span)
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
                            let span = info_span!(parent: parent_span.clone(), "block_on_polling", spawn_type = "poll");
                            let _ = self_clone
                                .clone()
                                .block_on_polling(ctx, parent_span)
                                .measure_execution_time(CONNECTION_DURATION, span)
                                .await;
                            self_clone.abort_polling(fqcn.clone()).await;
                        })
                    });
            }
        }
        Ok(())
    }

    async fn subscribe(&self, ctx: &ConnectionContext) -> CollectorResult<()> {
        let mut subscribed_characteristics = self.subscribed_characteristics.lock().await;

        match subscribed_characteristics.entry(ctx.fqcn.clone()) {
            Occupied(_) => {
                warn!("Already subscribed");
                return Ok(());
            }
            Vacant(entry) => {
                entry.insert(ctx.characteristic_config.clone());
            }
        }
        drop(subscribed_characteristics);

        ctx.peripheral.subscribe(&ctx.characteristic).await?;
        let existing_connections = self.get_all_connected_peripherals().await;
        info!(%existing_connections, "Subscribed on characteristic");

        Ok(())
    }
    #[tracing::instrument(level = "info", skip_all, err)]
    pub(super) async fn connect(&self, peripheral: &Peripheral) -> CollectorResult<()> {
        let _connect_permit = self.connection_lock.lock_for(peripheral.address()).await?;
        if peripheral.is_connected().await? {
            debug!("Already connected");
            return Ok(());
        }

        info!("Connecting to peripheral");
        timeout(self.app_conf.peripheral_connect_timeout, peripheral.connect())
            .measure_execution_time(CONNECTING_DURATION, Span::current())
            .await??;
        info!("Connected to peripheral");

        if peripheral.services().is_empty() {
            info!("Forcing service discovery for peripheral");
            self.discover_services(peripheral).await?;
            info!("Forced service discovery for peripheral completed");
        }

        Ok(())
    }

    async fn check_characteristic_is_handled(&self, fqcn: &Fqcn) -> bool {
        self.poll_handle_map.lock().await.get(fqcn).is_some()
            || self.subscribed_characteristics.lock().await.get(fqcn).is_some()
    }

    async fn abort_subscription(&self, fqcn: Arc<Fqcn>) {
        let mut subscribed_characteristics = self.subscribed_characteristics.lock().await;
        subscribed_characteristics.retain(|present_tk, _| present_tk.peripheral != fqcn.peripheral);

        if let Some(handle) = self.subscription_map.lock().await.remove(&fqcn.peripheral) {
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
}

impl PeripheralManager {
    #[tracing::instrument(level = "info", skip_all, parent = & _parent_span, err)]
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
                adapter_info: self.adapter_info.clone(),
                created_at: chrono::offset::Utc::now(),
                value,
                fqcn: ctx.fqcn.clone(),
                conf: Arc::clone(&ctx.characteristic_config),
            };
            self.fanout_sender.send(CollectorEvent::Payload(value.into())).await?;
            tokio::time::sleep(*delay_sec).await;
        }
    }

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
                adapter_info: self.adapter_info.clone(),
                created_at: chrono::offset::Utc::now(),
                value,
                fqcn,
                conf,
            };
            self.fanout_sender.send(CollectorEvent::Payload(value.into())).await?;
        }

        Err(CollectorError::EndOfStream)
    }

    #[tracing::instrument(level = "info", skip_all, parent = & _parent_span)]
    pub(crate) async fn handle_disconnect(
        &self,
        peripheral_key: &PeripheralKey,
        _parent_span: Span,
    ) -> CollectorResult<()> {
        CONNECTIONS_DROPPED.increment();
        {
            let mut poll_handle_map = self.poll_handle_map.lock().await;
            let mut subscription_map = self.subscription_map.lock().await;
            let mut subscribed_characteristics = self.subscribed_characteristics.lock().await;

            // self.peripheral_cache.remove(&peripheral_key.peripheral_address).await;

            subscribed_characteristics.retain(|fqcn, _| fqcn.peripheral != peripheral_key.peripheral_address);

            poll_handle_map.retain(|fqcn, handle| {
                if fqcn.peripheral == peripheral_key.peripheral_address {
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

        // we assume that this configuration still exists; it might not be the case in the future
        if let Some(conf) = self.configuration_manager.get_matching_config(peripheral_key).await {
            for (char_key, char_conf) in conf.service_map.iter() {
                let fqcn = Arc::new(Fqcn {
                    peripheral: peripheral_key.peripheral_address,
                    service: char_key.service_uuid,
                    characteristic: char_key.characteristic_uuid,
                });
                self.fanout_sender
                    .send(CollectorEvent::Disconnect(fqcn, char_conf.clone()))
                    .await?;
            }
        }

        let existing_peripherals = self.get_all_connected_peripherals().await;
        warn!(%existing_peripherals,"Peripheral disconnected");

        Ok(())
    }
}
