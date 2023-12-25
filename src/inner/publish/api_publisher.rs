use std::collections::VecDeque;
use std::sync::Arc;

use btleplug::api::BDAddr;
use chrono::{DateTime, Utc};
use dashmap::DashMap;
use serde::Serialize;
use uuid::Uuid;

use crate::inner::model::characteristic_payload::CharacteristicPayload;
use crate::inner::publish::dto::ApiDataPoint;
use crate::inner::publish::PublishPayload;

#[derive(Debug, Default, Serialize)]
pub(crate) struct CharacteristicStorage {
    pub(crate) name: Option<Arc<String>>,
    pub(crate) values: VecDeque<ApiDataPoint>,
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
        let mut peripheral = self.peripherals.entry(payload.fqcn.peripheral).or_default();

        peripheral.updated_at = payload.created_at;
        peripheral.num_updates += 1;

        let mut service = peripheral.services.entry(payload.fqcn.service).or_default();

        service.updated_at = payload.created_at;
        service.num_updates += 1;

        let mut char_storage = service.characteristics.entry(payload.fqcn.characteristic).or_default();

        char_storage.num_updates += 1;
        char_storage.name = payload.conf.name();
        while char_storage.values.len() > payload.conf.history_size() {
            char_storage.values.pop_front();
        }

        let data_point = ApiDataPoint::from(payload.as_ref());
        char_storage.values.push_back(data_point);
    }
}

impl PublishPayload for ApiPublisher {
    fn publish(&self, payload: Arc<CharacteristicPayload>) {
        self.process(payload);
    }
}
