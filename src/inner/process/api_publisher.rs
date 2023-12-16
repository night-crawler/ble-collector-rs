use std::collections::VecDeque;
use std::sync::Arc;

use btleplug::api::BDAddr;
use chrono::{DateTime, Utc};
use dashmap::DashMap;
use rocket::serde::Serialize;
use uuid::Uuid;

use crate::inner::conv::converter::CharacteristicValue;
use crate::inner::model::characteristic_payload::CharacteristicPayload;
use crate::inner::process::ProcessPayload;

#[derive(Debug, Serialize)]
pub(crate) struct DataPoint {
    pub(crate) ts: DateTime<Utc>,
    pub(crate) value: CharacteristicValue,
}

impl From<&CharacteristicPayload> for DataPoint {
    fn from(value: &CharacteristicPayload) -> Self {
        Self {
            ts: value.created_at,
            value: value.value.clone(),
        }
    }
}

#[derive(Debug, Default, Serialize)]
pub(crate) struct CharacteristicStorage {
    pub(crate) name: Option<Arc<String>>,
    pub(crate) values: VecDeque<DataPoint>,
    pub(crate) num_updates: usize,
}

#[derive(Debug, Default, Serialize)]
pub(crate) struct ServiceStorage {
    pub(crate) characteristics: DashMap<Uuid, CharacteristicStorage>,
    pub(crate) updated_at: DateTime<Utc>,
    pub(crate) num_updates: usize,
}

#[derive(Debug, Default, Serialize)]
pub(crate) struct PeripheralStorage {
    pub(crate) services: DashMap<Uuid, ServiceStorage>,
    pub(crate) updated_at: DateTime<Utc>,
    pub(crate) num_updates: usize,
}

#[derive(Debug, Serialize)]
pub(crate) struct ApiPublisher {
    pub(crate) peripherals: DashMap<BDAddr, PeripheralStorage>,
}

impl ApiPublisher {
    pub(crate) fn new() -> Self {
        Self {
            peripherals: DashMap::new(),
        }
    }
    pub(crate) fn process(&self, payload: Arc<CharacteristicPayload>) {
        let mut peripheral = self
            .peripherals
            .entry(payload.fqcn.peripheral_address)
            .or_default();

        peripheral.updated_at = payload.created_at;
        peripheral.num_updates += 1;

        let mut service = peripheral
            .services
            .entry(payload.fqcn.service_uuid)
            .or_default();

        service.updated_at = payload.created_at;
        service.num_updates += 1;

        let mut char_storage = service
            .characteristics
            .entry(payload.fqcn.characteristic_uuid)
            .or_default();

        char_storage.num_updates += 1;
        char_storage.name = payload.conf.name();
        while char_storage.values.len() > payload.conf.history_size() {
            char_storage.values.pop_front();
        }

        let data_point = DataPoint::from(payload.as_ref());
        char_storage.values.push_back(data_point);
    }
}

impl ProcessPayload for ApiPublisher {
    fn process(&self, payload: Arc<CharacteristicPayload>) {
        self.process(payload);
    }
}
