use rocket::serde::{Deserialize, Serialize};

use crate::inner::conf::dto::peripheral::PeripheralConfigDto;

#[derive(Debug, Clone, Serialize, Deserialize, Eq, PartialEq)]
pub(crate) struct CollectorConfigurationDto {
    pub(crate) peripherals: Vec<PeripheralConfigDto>,
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::inner::conf::dto::characteristic::{
        CharacteristicConfigDto, PublishMetricConfigDto,
    };
    use crate::inner::conf::dto::service::ServiceConfigDto;
    use crate::inner::conf::parse::Filter;
    use crate::inner::metrics::MetricType;
    use std::sync::Arc;
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
                    default_delay: Duration::from_secs(5),
                    default_history_size: 100,
                    characteristics: vec![
                        CharacteristicConfigDto::Subscribe {
                            history_size: Some(2),
                            name: Some("test".to_string().into()),
                            uuid: Uuid::nil(),
                            converter: Default::default(),
                            publish_metrics: Some(PublishMetricConfigDto {
                                metric_type: MetricType::Counter,
                                name: Arc::new("test".to_string()),
                                description: Some(Arc::new("test".to_string())),
                                unit: Arc::new("test".to_string()),
                                labels: Some(Arc::new(vec![(
                                    "test".to_string(),
                                    "test".to_string(),
                                )])),
                            }),
                        },
                        CharacteristicConfigDto::Poll {
                            history_size: None,
                            name: Some("test".to_string().into()),
                            uuid: Uuid::nil(),
                            delay: Some(Duration::from_secs(1)),
                            converter: Default::default(),
                            publish_metrics: None,
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
