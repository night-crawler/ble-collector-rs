use rocket::serde::{Deserialize, Serialize};

use crate::inner::conf::dto::peripheral::PeripheralConfigDto;

#[derive(Debug, Clone, Serialize, Deserialize, Eq, PartialEq)]
pub(crate) struct CollectorConfigurationDto {
    pub(crate) peripherals: Vec<PeripheralConfigDto>,
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::inner::conf::dto::characteristic::CharacteristicConfigDto;
    use crate::inner::conf::dto::service::ServiceConfigDto;
    use crate::inner::conf::parse::Filter;
    use std::time::Duration;
    use uuid::Uuid;

    /// check serialization / deserialization
    #[test]
    fn test() {
        let config = CollectorConfigurationDto {
            peripherals: vec![PeripheralConfigDto {
                name: "test".to_string(),
                adapter: Some(Filter::Contains("hci0".to_string())),
                device_id: Some(Filter::StartsWith("FA:6F".to_string())),
                device_name: Some(Filter::EndsWith("test".to_string())),
                services: vec![ServiceConfigDto {
                    name: Some("test".to_string().into()),
                    uuid: Uuid::nil(),
                    default_delay_sec: Duration::from_secs(5),
                    default_history_size: 100,
                    characteristics: vec![
                        CharacteristicConfigDto::Subscribe {
                            history_size: Some(2),
                            name: Some("test".to_string().into()),
                            uuid: Uuid::nil(),
                            converter: Default::default(),
                        },
                        CharacteristicConfigDto::Poll {
                            history_size: None,
                            name: Some("test".to_string().into()),
                            uuid: Uuid::nil(),
                            delay_sec: Some(Duration::from_secs(1)),
                            converter: Default::default(),
                        },
                    ],
                }],
            }],
        };

        let serialized = serde_yaml::to_string(&config).unwrap();
        println!("{}", serialized);
        let deserialized: CollectorConfigurationDto = serde_yaml::from_str(&serialized).unwrap();
        assert_eq!(config, deserialized);
    }
}
