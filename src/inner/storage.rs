use std::collections::VecDeque;
use std::sync::Arc;

use btleplug::api::BDAddr;
use chrono::{DateTime, Utc};
use dashmap::DashMap;
use log::debug;
use rocket::serde::Serialize;
use uuid::Uuid;

use crate::inner::conv::converter::CharacteristicValue;
use crate::inner::peripheral_manager::CharacteristicPayload;

#[derive(Debug, Serialize)]
pub(crate) struct DataPoint {
    pub(crate) ts: DateTime<Utc>,
    pub(crate) value: CharacteristicValue,
}

impl From<CharacteristicPayload> for DataPoint {
    fn from(value: CharacteristicPayload) -> Self {
        Self {
            ts: value.created_at,
            value: value.value,
        }
    }
}

#[derive(Debug, Default, Serialize)]
pub(crate) struct CharacteristicStorage {
    pub(crate) name: Option<Arc<String>>,
    pub(crate) values: VecDeque<DataPoint>,
}

#[derive(Debug, Default, Serialize)]
pub(crate) struct ServiceStorage {
    pub(crate) characteristics: DashMap<Uuid, CharacteristicStorage>,
    pub(crate) updated_at: DateTime<Utc>,
}

#[derive(Debug, Default, Serialize)]
pub(crate) struct PeripheralStorage {
    pub(crate) services: DashMap<Uuid, ServiceStorage>,
    pub(crate) updated_at: DateTime<Utc>,
}

#[derive(Debug, Default, Serialize)]
pub(crate) struct Storage {
    pub(crate) peripherals: DashMap<BDAddr, PeripheralStorage>,
}

impl Storage {
    pub(crate) fn new() -> Self {
        Self {
            peripherals: DashMap::new(),
        }
    }
    pub(crate) fn process(&self, payload: CharacteristicPayload) {
        let mut peripheral = self
            .peripherals
            .entry(payload.task_key.address)
            .or_default();

        peripheral.updated_at = payload.created_at;

        let mut service = peripheral
            .services
            .entry(payload.task_key.service_uuid)
            .or_default();

        service.updated_at = payload.created_at;

        let mut char_storage = service
            .characteristics
            .entry(payload.task_key.characteristic_uuid)
            .or_default();

        char_storage.name = payload.conf.name();
        while char_storage.values.len() > payload.conf.history_size() {
            char_storage.values.pop_front();
        }

        char_storage.values.push_back(payload.into());
    }

    pub(crate) fn block_on_receiving(
        self: Arc<Self>,
        receiver: kanal::Receiver<CharacteristicPayload>,
    ) {
        for payload in receiver {
            debug!("Processing payload: {payload}");
            self.process(payload);
        }
    }
}
