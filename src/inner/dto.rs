use std::collections::HashSet;
use std::fmt::Debug;

use anyhow::Context;
use bounded_integer::BoundedUsize;
use btleplug::api::{
    BDAddr, Characteristic, Descriptor, Peripheral as _, PeripheralProperties, Service, WriteType,
};
use btleplug::platform::Peripheral;
use log::{error, info};
use serde::{Deserialize, Serialize};
use serde_with::{serde_as, DurationMilliSeconds};
use uuid::Uuid;

use crate::inner::peripheral_manager::Fqcn;

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
            IoCommand::Write { timeout_ms, .. } => timeout_ms.clone(),
            IoCommand::Read { timeout_ms, .. } => timeout_ms.clone(),
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
}

#[cfg(test)]
mod tests {
    use std::str::FromStr;

    use btleplug::api::BDAddr;
    use uuid::Uuid;

    use crate::inner::peripheral_manager::Fqcn;

    use super::*;

    #[test]
    fn test_serialize_io_request() {
        let request = PeripheralIoRequestDto {
            batches: vec![PeripheralIoBatchRequestDto {
                commands: vec![
                    IoCommand::Write {
                        fqcn: Fqcn {
                            peripheral_address: BDAddr::from_str("00:00:00:00:00:00").unwrap(),
                            service_uuid: Uuid::from_str("0000180f-0000-1000-8000-00805f9b34fb")
                                .unwrap(),
                            characteristic_uuid: Uuid::from_str(
                                "00002a19-0000-1000-8000-00805f9b34fb",
                            )
                            .unwrap(),
                        },
                        value: vec![1, 2, 3],
                        wait_response: true,
                    },
                    IoCommand::Read {
                        fqcn: Fqcn {
                            peripheral_address: BDAddr::from_str("00:00:00:00:00:00").unwrap(),
                            service_uuid: Uuid::from_str("0000180f-0000-1000-8000-00805f9b34fb")
                                .unwrap(),
                            characteristic_uuid: Uuid::from_str(
                                "00002a19-0000-1000-8000-00805f9b34fb",
                            )
                            .unwrap(),
                        },
                        wait_notification: true,
                        timeout_ms: Some(std::time::Duration::from_secs(1)),
                    },
                ],
                parallelism: Some(BoundedUsize::new(1).unwrap()),
            }],
            parallelism: Some(BoundedUsize::new(1).unwrap()),
        };

        let serialized = serde_json::to_string_pretty(&request).unwrap();
        // println!("{}", serialized);

        let q = r#"
{"batches": [{"commands": [{"Write": {"fqcn": {"peripheral_address": "FA:6F:EC:EE:4B:36", "service_uuid": "ac866789-aaaa-eeee-a329-969d4bc8621e", "characteristic_uuid": "0000a004-0000-1000-8000-00805f9b34fb"}, "value": [2], "wait_response": true}}, {"Read": {"fqcn": {"peripheral_address": "FA:6F:EC:EE:4B:36", "service_uuid": "ac866789-aaaa-eeee-a329-969d4bc8621e", "characteristic_uuid": "0000a006-0000-1000-8000-00805f9b34fb"}, "wait_notification": true, "timeout_ms": 2000}}], "parallelism": 32}], "parallelism": 4}
        "#;

        let q = "{\"batches\": [{\"commands\": [{\"Write\": {\"fqcn\": {\"peripheral_address\": \"FA:6F:EC:EE:4B:36\", \"service_uuid\": \"ac866789-aaaa-eeee-a329-969d4bc8621e\", \"characteristic_uuid\": \"0000a004-0000-1000-8000-00805f9b34fb\"}, \"value\": [2], \"wait_response\": true}, \"Read\": null}, {\"Write\": null, \"Read\": {\"fqcn\": {\"peripheral_address\": \"FA:6F:EC:EE:4B:36\", \"service_uuid\": \"ac866789-aaaa-eeee-a329-969d4bc8621e\", \"characteristic_uuid\": \"0000a006-0000-1000-8000-00805f9b34fb\"}, \"wait_notification\": true, \"timeout_ms\": 2000}}], \"parallelism\": 32}], \"parallelism\": 4}";

        if let Err(err) = serde_json::from_str::<PeripheralIoRequestDto>(q) {
            println!("{:?}", err);
        } else {
            println!("!!!")
        }
    }

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
