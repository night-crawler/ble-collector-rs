use std::sync::Arc;

use handlebars::Handlebars;
use serde::Serialize;

use crate::inner::conf::dto::publish::PublishMqttConfigDto;
use crate::inner::error::{CollectorError, CollectorResult};
use crate::inner::model::characteristic_payload::CharacteristicPayload;
use crate::inner::model::fqcn::Fqcn;
use crate::inner::publish::mqtt_discovery_payload::MqttDiscoveryPayload;

#[derive(Debug, Default)]
pub(crate) struct MqttInterpolator {
    hbs: Handlebars<'static>,
}

#[derive(Debug, Serialize)]
struct CleanFqcn {
    peripheral: String,
    service: String,
    characteristic: String,
}

#[derive(Debug, Serialize)]
struct Context {
    fqcn: Arc<Fqcn>,
    clean_fqcn: CleanFqcn,
}

impl From<&CharacteristicPayload> for Context {
    fn from(value: &CharacteristicPayload) -> Self {
        Self {
            fqcn: value.fqcn.clone(),
            clean_fqcn: CleanFqcn {
                peripheral: value.fqcn.peripheral.to_string().replace(':', "_"),
                service: value.fqcn.service.to_string().replace('-', "_"),
                characteristic: value.fqcn.characteristic.to_string().replace('-', "_"),
            },
        }
    }
}

impl MqttInterpolator {
    pub(crate) fn interpolate_state_topic(
        &self,
        topic: &str,
        value: &CharacteristicPayload,
    ) -> CollectorResult<String> {
        let ctx = serde_json::to_value(Context::from(value))?;
        let topic = self.hbs.render_template(topic, &ctx)?;
        Ok(topic)
    }

    pub(crate) fn interpolate_discovery(
        &self,
        fqcn: Arc<Fqcn>,
        mqtt_conf: &PublishMqttConfigDto,
    ) -> CollectorResult<MqttDiscoveryPayload> {
        let Some(discovery) = mqtt_conf.discovery.as_ref() else {
            return Err(CollectorError::NoMqttDiscoveryConfig);
        };
        let mut ctx = serde_json::to_value(Context {
            fqcn: fqcn.clone(),
            clean_fqcn: CleanFqcn {
                peripheral: fqcn.peripheral.to_string().replace(':', "_"),
                service: fqcn.service.to_string().replace('-', "_"),
                characteristic: fqcn.characteristic.to_string().replace('-', "_"),
            },
        })?;
        let state_topic = self.hbs.render_template(mqtt_conf.state_topic.as_str(), &ctx)?;
        let config_topic = self.hbs.render_template(discovery.config_topic.as_str(), &ctx)?;

        // add payload_topic to the context
        if let serde_json::Value::Object(ctx) = &mut ctx {
            ctx.insert(
                "state_topic".to_string(),
                serde_json::Value::String(state_topic.clone()),
            );
            ctx.insert(
                "config_topic".to_string(),
                serde_json::Value::String(config_topic.clone()),
            );
        } else {
            return Err(CollectorError::InvalidMqttConfig(ctx.to_string()));
        }

        let mut interpolated_mqtt_conf = serde_json::to_value(&discovery.remainder)?;
        self.interpolate_value(&mut interpolated_mqtt_conf, &ctx)?;

        Ok(MqttDiscoveryPayload {
            config_topic,
            discovery_config: Some(interpolated_mqtt_conf),
        })
    }

    fn interpolate_value(
        &self,
        value: &mut serde_json::Value,
        ctx: &serde_json::Value,
    ) -> Result<(), handlebars::RenderError> {
        match value {
            serde_json::Value::String(s) => {
                let rendered = self.hbs.render_template(s, ctx)?;
                *value = serde_json::Value::String(rendered);
            }
            serde_json::Value::Array(a) => {
                for v in a {
                    self.interpolate_value(v, ctx)?;
                }
            }
            serde_json::Value::Object(o) => {
                for (_, v) in o {
                    self.interpolate_value(v, ctx)?;
                }
            }
            _ => {}
        }

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use chrono::Utc;
    use serde_json::json;

    use crate::inner::conf::dto::publish::{DiscoverySettings, PublishMqttConfigDto};
    use crate::inner::conf::model::characteristic_config::CharacteristicConfig;
    use crate::inner::conv::converter::{CharacteristicValue, Converter};
    use crate::inner::model::adapter_info::AdapterInfo;
    use crate::inner::model::characteristic_payload::CharacteristicPayload;
    use crate::inner::model::fqcn::Fqcn;

    use super::*;

    #[test]
    fn test_mqtt_payload_interpolation() {
        let config = json! {{
           "device_class":"temperature",
           "state_topic":"{{state_topic}}",
           "unit_of_measurement":"°C",
           "value_template":"\\{{ value_json.temperature }}",
           "unique_id":"temp01ae",
           "device":{
              "identifiers":[
                 "{{~#if (eq fqcn.peripheral '11:22:33:44:55:66')~}}
                    sample
                  {{~else~}}
                    {{~peripheral~}}
                  {{~/if~}}"
              ],
              "name":"{{~#if (eq fqcn.peripheral '11:22:33:44:55:66')~}}
                {{~clean_fqcn.peripheral~}}
              {{~else~}}
                {{~peripheral~}}
              {{~/if~}}",
           }
        }};
        let config: serde_yaml::Value = serde_json::from_value(config).unwrap();

        let fqcn = Arc::new(Fqcn {
            peripheral: "11:22:33:44:55:66".parse().unwrap(),
            service: "0000180f-0000-1000-8000-00805f9b34fb".parse().unwrap(),
            characteristic: "00002a19-0000-1000-8000-00805f9b34fb".parse().unwrap(),
        });

        let mqtt_conf = PublishMqttConfigDto {
            state_topic: Arc::new("test-{{ clean_fqcn.peripheral }}".to_string()),
            unit: Some(Arc::new("test-{{peripheral}}-test".to_string())),
            retain: true,
            qos: Default::default(),
            discovery: Some(Arc::new(DiscoverySettings {
                config_topic: Arc::new("config-test-{{clean_fqcn.peripheral}}".to_string()),
                remainder: config,
            })),
        };

        let char_conf = Arc::new(CharacteristicConfig::Subscribe {
            name: Some("name-test".to_string().into()),
            service_name: Some("service-name-test".to_string().into()),
            service_uuid: "0000180f-0000-1000-8000-00805f9b34fb".parse().unwrap(),
            uuid: "00002a19-0000-1000-8000-00805f9b34fb".parse().unwrap(),
            history_size: 42,
            converter: Converter::F32,
            publish_metrics: None,
            publish_mqtt: Some(mqtt_conf.clone()),
        });

        let payload = CharacteristicPayload {
            fqcn: fqcn.clone(),
            value: CharacteristicValue::F64(42.0),
            created_at: Utc::now(),
            conf: char_conf,
            adapter_info: Arc::new(AdapterInfo {
                id: "hci0".to_string(),
                modalias: "smth".to_string(),
            }),
        };

        let interpolator = MqttInterpolator::default();
        let mqtt_payload = interpolator.interpolate_discovery(fqcn.clone(), &mqtt_conf).unwrap();

        assert_eq!(
            mqtt_payload,
            MqttDiscoveryPayload {
                config_topic: "config-test-11_22_33_44_55_66".to_string(),
                discovery_config: Some(json! {{
                    "device": {
                      "identifiers": [
                        "sample"
                      ],
                      "name": "11_22_33_44_55_66"
                    },
                    "device_class": "temperature",
                    "state_topic": "test-11_22_33_44_55_66",
                    "unique_id": "temp01ae",
                    "unit_of_measurement": "°C",
                    "value_template": "{{ value_json.temperature }}"
                }}),
            }
        );

        let topic = interpolator
            .interpolate_state_topic(mqtt_conf.state_topic.as_str(), &payload)
            .unwrap();
        assert_eq!(topic, "test-11_22_33_44_55_66");
    }
}
