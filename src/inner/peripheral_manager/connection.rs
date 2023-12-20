use std::collections::hash_map::Entry::{Occupied, Vacant};
use std::sync::Arc;

use anyhow::Context;
use btleplug::api::Peripheral as _;
use btleplug::platform::Peripheral;
use futures_util::StreamExt;
use tokio::time::timeout;
use tracing::{info, info_span, warn, Span};

use crate::inner::conf::model::characteristic_config::CharacteristicConfig;
use crate::inner::conf::model::flat_peripheral_config::FlatPeripheralConfig;
use crate::inner::conf::model::service_characteristic_key::ServiceCharacteristicKey;
use crate::inner::error::{CollectorError, CollectorResult};
use crate::inner::metrics::{
    CONNECTED_PERIPHERALS, CONNECTING_DURATION, CONNECTIONS_DROPPED, CONNECTIONS_HANDLED,
    CONNECTION_DURATION, SERVICE_DISCOVERY_DURATION, TOTAL_CONNECTING_DURATION,
};
use crate::inner::model::characteristic_payload::CharacteristicPayload;
use crate::inner::model::fqcn::Fqcn;
use crate::inner::model::peripheral_key::PeripheralKey;
use crate::inner::peripheral_manager::connection_context::ConnectionContext;
use crate::inner::peripheral_manager::PeripheralManager;

impl PeripheralManager {
    #[tracing::instrument(level = "info", skip_all, parent = & _parent_span, err)]
    pub(super) async fn connect_all(
        self: Arc<Self>,
        peripheral_key: &PeripheralKey,
        peripheral_config: Arc<FlatPeripheralConfig>,
        _parent_span: Span,
    ) -> CollectorResult<()> {
        info!("Connecting to all available peripheral characteristics");
        CONNECTIONS_HANDLED.increment();

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
                .measure(|| self.clone().spawn(ctx))
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
    service = % ctx.fqcn.service_uuid,
    characteristic = % ctx.fqcn.characteristic_uuid,
    ))]
    async fn spawn(self: Arc<Self>, ctx: ConnectionContext) -> CollectorResult<()> {
        info!("Spawning subscription / polling tasks");
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
        info!("Connecting to peripheral");
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

    async fn check_characteristic_is_handled(&self, fqcn: &Fqcn) -> bool {
        self.poll_handle_map.lock().await.get(fqcn).is_some()
            || self
                .subscribed_characteristics
                .lock()
                .await
                .get(fqcn)
                .is_some()
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
}

impl PeripheralManager {
    #[tracing::instrument(level = "info", skip_all, parent = & _parent_span, err)]
    async fn block_on_polling(
        self: Arc<Self>,
        ctx: ConnectionContext,
        _parent_span: Span,
    ) -> CollectorResult<()> {
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

    #[tracing::instrument(level = "info", skip_all, parent = & _parent_span, err)]
    async fn block_on_notifying(
        self: Arc<Self>,
        ctx: ConnectionContext,
        _parent_span: Span,
    ) -> CollectorResult<()> {
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

    #[tracing::instrument(level = "info", skip_all, parent = & _parent_span)]
    pub(crate) async fn handle_disconnect(
        &self,
        peripheral_key: &PeripheralKey,
        _parent_span: Span,
    ) {
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
        warn!(%existing_peripherals,"Peripheral disconnected");
    }
}
