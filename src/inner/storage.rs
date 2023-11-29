use std::collections::VecDeque;
use std::sync::Arc;

use btleplug::api::BDAddr;
use chrono::{DateTime, Utc};
use dashmap::DashMap;
use log::info;
use rocket::serde::Serialize;
use uuid::Uuid;

use crate::inner::conv::converter::CharacteristicValue;
use crate::inner::peripheral_manager::CharacteristicPayload;

#[derive(Debug, Default, Serialize)]
pub(crate) struct CharStorage {
    pub(crate) characteristics_map: DashMap<Uuid, VecDeque<CharacteristicValue>>,
    pub(crate) updated_at: DateTime<Utc>,
}

#[derive(Debug, Default, Serialize)]
pub(crate) struct ServiceStorage {
    pub(crate) service_map: DashMap<Uuid, CharStorage>,
    pub(crate) updated_at: DateTime<Utc>,
}

#[derive(Debug, Default, Serialize)]
pub(crate) struct PeripheralStorage {
    pub(crate) peripheral_map: DashMap<BDAddr, ServiceStorage>,
}

impl PeripheralStorage {
    pub(crate) fn new() -> Self {
        Self {
            peripheral_map: DashMap::new(),
        }
    }
    pub(crate) fn process(&self, payload: CharacteristicPayload) {
        let peripheral = self
            .peripheral_map
            .entry(payload.task_key.address)
            .or_default();

        let service = peripheral
            .service_map
            .entry(payload.task_key.service_uuid)
            .or_default();

        let mut char_deque = service
            .characteristics_map
            .entry(payload.task_key.characteristic_uuid)
            .or_default();

        char_deque.push_back(payload.value);
        while char_deque.len() > payload.conf.history_size() {
            char_deque.pop_front();
        }
    }

    pub(crate) fn block_on_receiving(
        self: Arc<Self>,
        receiver: kanal::Receiver<CharacteristicPayload>,
    ) {
        for payload in receiver {
            info!("Processing payload: {payload:?}");
            self.process(payload);
        }
    }
}
