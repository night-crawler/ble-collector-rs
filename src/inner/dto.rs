use std::collections::HashSet;
use std::fmt::Debug;

use crate::inner::model::adapter_info::AdapterInfo;
use crate::inner::model::fqcn::Fqcn;
use bounded_integer::BoundedUsize;
use btleplug::api::{
    BDAddr, Characteristic, Descriptor, Peripheral as _, PeripheralProperties, Service, WriteType,
};
use btleplug::platform::Peripheral;
use serde::{Deserialize, Serialize};
use serde_with::{serde_as, DurationMilliSeconds};
use tracing::{error, info};
use uuid::Uuid;

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
pub(crate) struct AdapterDto {
    pub(crate) adapter_info: AdapterInfo,
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

        let props = peripheral.properties().await.unwrap_or_else(|err| {
            error!(
                "Error getting properties for peripheral {:?}: {:?}",
                peripheral.id(),
                err
            );
            None
        });
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
        let adapter_info = AdapterInfo::try_from(value)?;
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

#[derive(Debug, Serialize)]
pub(crate) struct PeripheralIoResponseDto {
    pub(crate) batch_responses: Vec<PeripheralIoBatchResponseDto>,
}

#[derive(Debug, Serialize)]
pub(crate) struct PeripheralIoBatchResponseDto {
    pub(crate) command_responses: Vec<Option<ResultDto<Vec<u8>>>>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub(crate) struct PeripheralIoRequestDto {
    pub(crate) batches: Vec<PeripheralIoBatchRequestDto>,
    pub(crate) parallelism: Option<BoundedUsize<1, 64>>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub(crate) struct PeripheralIoBatchRequestDto {
    pub(crate) commands: Vec<IoCommand>,
    pub(crate) parallelism: Option<BoundedUsize<1, 64>>,
}

#[serde_as]
#[derive(Debug, Clone, Serialize, Deserialize)]
pub(crate) enum IoCommand {
    Write {
        fqcn: Fqcn,
        value: Vec<u8>,
        wait_response: bool,
        #[serde_as(as = "Option<DurationMilliSeconds>")]
        timeout_ms: Option<std::time::Duration>,
    },
    Read {
        fqcn: Fqcn,
        wait_notification: bool,
        #[serde_as(as = "Option<DurationMilliSeconds>")]
        timeout_ms: Option<std::time::Duration>,
    },
}

impl IoCommand {
    pub(crate) fn get_timeout(&self) -> Option<std::time::Duration> {
        match self {
            IoCommand::Write { timeout_ms, .. } => *timeout_ms,
            IoCommand::Read { timeout_ms, .. } => *timeout_ms,
        }
    }
    pub(crate) fn get_write_type(&self) -> WriteType {
        match self {
            IoCommand::Write { wait_response, .. } => {
                if *wait_response {
                    WriteType::WithResponse
                } else {
                    WriteType::WithoutResponse
                }
            }
            IoCommand::Read { .. } => WriteType::WithoutResponse,
        }
    }

    pub(crate) fn get_fqcn(&self) -> &Fqcn {
        match self {
            IoCommand::Write { fqcn, .. } => fqcn,
            IoCommand::Read { fqcn, .. } => fqcn,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_serialize_response() {
        let response = PeripheralIoResponseDto {
            batch_responses: vec![PeripheralIoBatchResponseDto {
                command_responses: vec![
                    Some(ResultDto::Ok(vec![1, 2, 3])),
                    Some(ResultDto::Error {
                        message: "Error".to_string(),
                    }),
                ],
            }],
        };

        let serialized = serde_json::to_string(&response).unwrap();

        println!("{}", serialized);
    }
}
