use std::collections::HashSet;

use anyhow::Context;
use btleplug::api::{BDAddr, Characteristic, Descriptor, Peripheral as _, PeripheralProperties, Service};
use btleplug::platform::Peripheral;
use log::{error, info};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub(crate) struct AdapterDto {
    pub(crate) id: String,
    pub(crate) modalias: String,
    pub(crate) peripherals: Vec<PeripheralDto>,
}


impl AdapterDto {
    pub(crate) fn add_peripheral(&mut self, peripheral_dto: PeripheralDto) {
        self.peripherals.push(peripheral_dto)
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub(crate) struct PeripheralDto {
    pub(crate) id: String,
    pub(crate) address: BDAddr,
    pub(crate) props: Option<PeripheralProperties>,
    pub(crate) services: Vec<ServiceDto>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub(crate) struct ServiceDto {
    pub uuid: Uuid,
    pub primary: bool,
    pub characteristics: Vec<CharacteristicDto>,
}

impl From<Service> for ServiceDto {
    fn from(value: Service) -> Self {
        Self {
            uuid: value.uuid,
            primary: value.primary,
            characteristics: value.characteristics.into_iter().map(CharacteristicDto::from).collect(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, Eq, PartialEq, Hash)]
pub(crate) enum CharPropDto {
    Broadcast,
    Read,
    WriteWithoutResponse,
    Write,
    Notify,
    Indicate,
    AuthenticatedSignedWrites,
    ExtendedProperties,
    Unknown,
}

impl From<&str> for CharPropDto {
    fn from(value: &str) -> Self {
        match value {
            "BROADCAST" => Self::Broadcast,
            "READ" => Self::Read,
            "WRITE_WITHOUT_RESPONSE" => Self::WriteWithoutResponse,
            "WRITE" => Self::Write,
            "NOTIFY" => Self::Notify,
            "INDICATE" => Self::Indicate,
            "AUTHENTICATED_SIGNED_WRITES" => Self::AuthenticatedSignedWrites,
            "EXTENDED_PROPERTIES" => Self::ExtendedProperties,
            _ => Self::Unknown
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub(crate) struct CharacteristicDto {
    pub uuid: Uuid,
    pub service_uuid: Uuid,
    pub properties: HashSet<CharPropDto>,
    pub descriptors: Vec<DescriptorDto>,
}

impl From<Characteristic> for CharacteristicDto {
    fn from(value: Characteristic) -> Self {
        Self {
            uuid: value.uuid,
            service_uuid: value.service_uuid,
            properties: value.properties.iter_names().map(|(name, _)| name).map(CharPropDto::from).collect(),
            descriptors: value.descriptors.into_iter().map(DescriptorDto::from).collect(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub(crate) struct DescriptorDto {
    pub uuid: Uuid,
    pub service_uuid: Uuid,
    pub characteristic_uuid: Uuid,
}

impl From<Descriptor> for DescriptorDto {
    fn from(value: Descriptor) -> Self {
        Self {
            uuid: value.uuid,
            service_uuid: value.service_uuid,
            characteristic_uuid: value.characteristic_uuid,
        }
    }
}

impl PeripheralDto {
    pub(crate) async fn from_platform(
        peripheral: Peripheral
    ) -> btleplug::Result<Self> {
        // info!("Connecting to peripheral: {:?}", peripheral.id());
        // if let Err(err) = tokio::time::timeout(Duration::from_secs(5), peripheral.connect()).await {
        //     warn!("Timeout connecting to peripheral {:?}: {:?}", peripheral.id(), err);
        // } else {
        //     info!("Connected to peripheral: {:?}", peripheral.id());
        // }
        if let Err(err) = peripheral.discover_services().await {
            error!("Error discovering services for peripheral {:?}: {:?}", peripheral.id(), err);
        } else {
            info!("Discovered services for peripheral: {:?}", peripheral.id());
        }
        // if let Err(err) = tokio::time::timeout(Duration::from_secs(5), peripheral.disconnect()).await {
        //     warn!("Timeout disconnecting from peripheral {:?}: {:?}", peripheral.id(), err);
        // } else {
        //     info!("Disconnected from peripheral: {:?}", peripheral.id());
        // }

        let props = match peripheral.properties().await {
            Ok(props) => props,
            Err(err) => {
                error!("Error getting properties for peripheral {:?}: {:?}", peripheral.id(), err);
                None
            }
        };
        let services = peripheral.services();

        Ok(Self {
            id: peripheral.id().to_string(),
            address: peripheral.address(),
            props,
            services: services.into_iter().map(ServiceDto::from).collect(),
        })
    }
}

impl TryFrom<String> for AdapterDto {
    type Error = anyhow::Error;

    fn try_from(value: String) -> Result<Self, Self::Error> {
        let mut pair = value.split_whitespace();
        let id = pair.next().context("No id")?.to_string();
        let modalias = pair.next().context("No modalias")?.trim();
        let modalias = modalias.strip_prefix('(').unwrap_or(modalias);
        let modalias = modalias.strip_suffix(')').unwrap_or(modalias);
        let modalias = modalias.to_string();
        Ok(Self {
            id,
            modalias,
            peripherals: vec![],
        })
    }
}
