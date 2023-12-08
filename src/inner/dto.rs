use std::collections::{HashMap, HashSet};
use std::fmt::Debug;

use anyhow::Context;
use bounded_integer::BoundedUsize;
use btleplug::api::{
    BDAddr, Characteristic, Descriptor, Peripheral as _, PeripheralProperties, Service, WriteType,
};
use btleplug::platform::Peripheral;
use log::{error, info};
use serde::{Deserialize, Serialize};
use serde_with::{DurationMilliSeconds, serde_as};
use serde_with::DurationSeconds;
use uuid::Uuid;

use crate::inner::peripheral_manager::TaskKey;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub(crate) struct Envelope<T> {
    pub(crate) data: T,
}

impl<T> From<T> for Envelope<T> {
    fn from(data: T) -> Self {
        Self { data }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub(crate) struct AdapterInfoDto {
    pub(crate) id: String,
    pub(crate) modalias: String,
}

impl TryFrom<String> for AdapterInfoDto {
    type Error = anyhow::Error;

    fn try_from(value: String) -> Result<Self, Self::Error> {
        let mut pair = value.split_whitespace();
        let id = pair.next().context("No id")?.to_string();
        let modalias = pair.next().context("No modalias")?.trim();
        let modalias = modalias.strip_prefix('(').unwrap_or(modalias);
        let modalias = modalias.strip_suffix(')').unwrap_or(modalias);
        let modalias = modalias.to_string();
        Ok(Self { id, modalias })
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub(crate) struct AdapterDto {
    pub(crate) adapter_info: AdapterInfoDto,
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
            characteristics: value
                .characteristics
                .into_iter()
                .map(CharacteristicDto::from)
                .collect(),
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
            _ => Self::Unknown,
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
            properties: value
                .properties
                .iter_names()
                .map(|(name, _)| name)
                .map(CharPropDto::from)
                .collect(),
            descriptors: value
                .descriptors
                .into_iter()
                .map(DescriptorDto::from)
                .collect(),
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
    pub(crate) async fn from_platform(peripheral: Peripheral) -> btleplug::Result<Self> {
        if let Err(err) = peripheral.discover_services().await {
            error!(
                "Error discovering services for peripheral {:?}: {:?}",
                peripheral.id(),
                err
            );
        } else {
            info!("Discovered services for peripheral: {:?}", peripheral.id());
        }

        let props = match peripheral.properties().await {
            Ok(props) => props,
            Err(err) => {
                error!(
                    "Error getting properties for peripheral {:?}: {:?}",
                    peripheral.id(),
                    err
                );
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
        let adapter_info = AdapterInfoDto::try_from(value)?;
        Ok(Self {
            adapter_info,
            peripherals: vec![],
        })
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub(crate) enum ResultDto<T> {
    Ok(T),
    Error { message: String },
}

impl<T, E> From<Result<T, E>> for ResultDto<T>
where
    E: Debug,
{
    fn from(value: Result<T, E>) -> Self {
        match value {
            Ok(value) => Self::Ok(value),
            Err(err) => Self::Error {
                message: format!("{:?}", err),
            },
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub(crate) struct PeripheralIoResponseDto {
    pub(crate) write_commands: Vec<ResultDto<()>>,
    pub(crate) read_commands: Vec<ResultDto<Vec<u8>>>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub(crate) struct PeripheralIoRequestDto {
    pub(crate) write_commands: Vec<WritePeripheralValueCommandDto>,
    pub(crate) read_commands: Vec<ReadPeripheralValueCommandDto>,
    pub(crate) parallelism: Option<BoundedUsize<1, 64>>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub(crate) struct WritePeripheralValueCommandDto {
    pub(crate) peripheral_address: BDAddr,
    pub(crate) service_uuid: Uuid,
    pub(crate) characteristic_uuid: Uuid,
    pub(crate) value: Vec<u8>,
    pub(crate) wait_response: bool,
}

impl From<&WritePeripheralValueCommandDto> for TaskKey {
    fn from(value: &WritePeripheralValueCommandDto) -> Self {
        Self {
            address: value.peripheral_address,
            service_uuid: value.service_uuid,
            characteristic_uuid: value.characteristic_uuid,
        }
    }
}

impl WritePeripheralValueCommandDto {
    pub(crate) fn get_write_type(&self) -> WriteType {
        if self.wait_response {
            WriteType::WithResponse
        } else {
            WriteType::WithoutResponse
        }
    }
}

#[serde_as]
#[derive(Debug, Clone, Serialize, Deserialize)]
pub(crate) struct ReadPeripheralValueCommandDto {
    pub(crate) peripheral_address: BDAddr,
    pub(crate) service_uuid: Uuid,
    pub(crate) characteristic_uuid: Uuid,
    pub(crate) wait_notification: bool,
    #[serde_as(as = "Option<DurationMilliSeconds>")]
    pub(crate) timeout_ms: Option<std::time::Duration>,
}

impl From<&ReadPeripheralValueCommandDto> for TaskKey {
    fn from(value: &ReadPeripheralValueCommandDto) -> Self {
        Self {
            address: value.peripheral_address,
            service_uuid: value.service_uuid,
            characteristic_uuid: value.characteristic_uuid,
        }
    }
}


#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_serialize() {
        let write_command = WritePeripheralValueCommandDto {
            peripheral_address: Default::default(),
            service_uuid: Default::default(),
            characteristic_uuid: Default::default(),
            value: vec![1, 117, 255],
            wait_response: false,
        };
        let q = serde_json::ser::to_string(&write_command).unwrap();
        println!("{}", q);
    }
}