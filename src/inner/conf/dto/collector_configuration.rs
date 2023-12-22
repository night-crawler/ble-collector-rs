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
    use crate::inner::conf::dto::publish::{PublishMetricConfigDto, PublishMqttConfigDto};
    use crate::inner::conf::dto::service::ServiceConfigDto;
    use crate::inner::conf::model::filter::Filter;
    use crate::inner::metrics::MetricType;
    use serde_json::json;
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
                            publish_mqtt: Some(PublishMqttConfigDto {
                                topic: Arc::new("test".to_string()),
                                unit: Some(Arc::new("test".to_string())),
                                retain: true,
                                qos: Default::default(),
                            }),
                        },
                        CharacteristicConfigDto::Poll {
                            history_size: None,
                            name: Some("test".to_string().into()),
                            uuid: Uuid::nil(),
                            delay: Some(Duration::from_secs(1)),
                            converter: Default::default(),
                            publish_metrics: None,
                            publish_mqtt: None,
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

    #[test]
    fn test_load_example() {
        let example = include_str!("../../../../example.yaml");
        let deserialized: CollectorConfigurationDto = serde_yaml::from_str(example).unwrap();
        let mut grafana_rules = vec![];
        for peripheral in deserialized.peripherals {
            for service in peripheral.services {
                for characteristic in service.characteristics {
                    let (char_uuid, name) = match characteristic {
                        CharacteristicConfigDto::Subscribe { uuid, name, .. } => (uuid, name),
                        CharacteristicConfigDto::Poll { uuid, name, .. } => (uuid, name),
                    };

                    let name = name
                        .unwrap()
                        .split_whitespace()
                        .collect::<Vec<_>>()
                        .join("");

                    let regex = format!("(.*)(.*)({char_uuid})");
                    let rename_pattern = format!("$1 $2 {name}");

                    let char_rename = json!({
                        "id": "renameByRegex",
                        "options": {
                            "regex": regex,
                            "renamePattern": rename_pattern
                        }
                    });

                    grafana_rules.push(char_rename);
                }

                let service_name = service
                    .name
                    .unwrap()
                    .split_whitespace()
                    .collect::<Vec<_>>()
                    .join("");
                let service_uuid = service.uuid;
                let regex = format!("(.*)({service_uuid})(.+)");
                let rename_pattern = format!("$1 {service_name} $3");
                let service_rename = json!({
                    "id": "renameByRegex",
                    "options": {
                        "regex": regex,
                        "renamePattern": rename_pattern
                    }
                });

                grafana_rules.push(service_rename);
            }
        }

        println!("{}", serde_json::to_string_pretty(&grafana_rules).unwrap());
    }
}
