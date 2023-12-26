use std::sync::Arc;

use rhai::{Dynamic, Scope};
use rumqttc::v5::mqttbytes::QoS;
use serde::Serialize;

use crate::inner::error::{CollectorError, CollectorResult};
use crate::inner::model::characteristic_payload::CharacteristicPayload;
use crate::inner::model::connect_peripheral_request::ConnectPeripheralRequest;
use crate::inner::model::fqcn::Fqcn;
use crate::inner::publish::mqtt_discovery_payload::MqttDiscoveryPayload;

#[derive(Debug, Default)]
pub(crate) struct MqttInterpolator {
    engine: rhai::Engine,
}

#[derive(Debug, Serialize)]
struct CleanFqcn {
    peripheral: String,
    service: String,
    characteristic: String,
}

/// (alphanumerics, underscore and hyphen)
fn clean_str(s: &str) -> String {
    s.chars()
        .map(|c| match c {
            'a'..='z' | 'A'..='Z' | '0'..='9' => c,
            '-' | '_' => c,
            _ => '_',
        })
        .collect()
}

impl From<&Fqcn> for CleanFqcn {
    fn from(value: &Fqcn) -> Self {
        CleanFqcn {
            peripheral: clean_str(&value.peripheral.to_string()),
            service: clean_str(&value.service.to_string()),
            characteristic: clean_str(&value.characteristic.to_string()),
        }
    }
}

#[derive(Debug, Serialize)]
struct Context {
    fqcn: Arc<Fqcn>,
    clean_fqcn: CleanFqcn,
    service_name: Option<Arc<String>>,
    clean_service_name: Option<String>,
    characteristic_name: Option<Arc<String>>,
    clean_characteristic_name: Option<String>,
    peripheral_name: Option<String>,
    clean_peripheral_name: Option<String>,
}

impl TryFrom<Context> for Dynamic {
    type Error = CollectorError;

    fn try_from(value: Context) -> Result<Self, Self::Error> {
        Ok(rhai::serde::to_dynamic(value)?)
    }
}

impl TryFrom<Context> for Scope<'_> {
    type Error = CollectorError;

    fn try_from(value: Context) -> Result<Self, Self::Error> {
        let mut scope = Scope::new();
        scope.push("ctx", Dynamic::try_from(value)?);
        Ok(scope)
    }
}

impl From<&CharacteristicPayload> for Context {
    fn from(value: &CharacteristicPayload) -> Self {
        Self {
            fqcn: value.fqcn.clone(),
            clean_fqcn: CleanFqcn::from(value.fqcn.as_ref()),
            service_name: value.conf.service_name().clone(),
            clean_service_name: value.conf.service_name().map(|s| clean_str(s.as_str())),
            characteristic_name: value.conf.name().clone(),
            clean_characteristic_name: value.conf.name().map(|s| clean_str(s.as_str())),
            peripheral_name: None, // TODO: pass through peripheral key as well?
            clean_peripheral_name: None,
        }
    }
}

impl From<&ConnectPeripheralRequest> for Context {
    fn from(value: &ConnectPeripheralRequest) -> Self {
        Context {
            fqcn: value.fqcn.clone(),
            clean_fqcn: CleanFqcn::from(value.fqcn.as_ref()),
            service_name: value.conf.service_name().clone(),
            clean_service_name: value.conf.service_name().map(|s| clean_str(s.as_str())),
            characteristic_name: value.conf.name().clone(),
            clean_characteristic_name: value.conf.name().map(|s| clean_str(s.as_str())),
            peripheral_name: value.peripheral_key.name.clone(),
            clean_peripheral_name: value.peripheral_key.name.as_ref().map(|s| clean_str(s.as_str())),
        }
    }
}

impl MqttInterpolator {
    pub(crate) fn interpolate_state_topic(
        &self,
        topic: &str,
        value: &CharacteristicPayload,
    ) -> CollectorResult<String> {
        let mut scope = Scope::try_from(Context::from(value))?;
        let result: String = self.eval(&mut scope, topic)?;
        Ok(result)
    }

    pub(crate) fn interpolate_discovery(
        &self,
        request: ConnectPeripheralRequest,
    ) -> CollectorResult<MqttDiscoveryPayload> {
        let Some(mqtt_conf) = request.conf.publish_mqtt() else {
            return Err(CollectorError::NoMqttConfig);
        };
        let Some(discovery) = mqtt_conf.discovery.as_ref() else {
            return Err(CollectorError::NoMqttDiscoveryConfig);
        };

        let ctx = Context::from(&request);
        let mut scope = Scope::try_from(ctx)?;

        let state_topic: String = self.eval(&mut scope, mqtt_conf.state_topic.as_str())?;
        let config_topic: String = self.eval(&mut scope, discovery.config_topic.as_str())?;

        scope // add topics to the context
            .push("state_topic", state_topic)
            .push("config_topic", config_topic.clone());

        let mut interpolated_mqtt_conf = serde_json::to_value(&discovery.remainder)?;
        self.interpolate_value(&mut interpolated_mqtt_conf, &mut scope)?;

        Ok(MqttDiscoveryPayload {
            config_topic,
            discovery_config: Some(interpolated_mqtt_conf),
            retain: discovery.retain.unwrap_or(mqtt_conf.retain),
            qos: discovery.qos.map(QoS::from).unwrap_or(mqtt_conf.qos()),
        })
    }

    #[tracing::instrument(skip(self, scope), err)]
    fn eval(&self, scope: &mut Scope, expr: &str) -> CollectorResult<String> {
        let result = self.engine.eval_with_scope(scope, expr);
        match result {
            Ok(result) => Ok(result),
            Err(e) => Err(CollectorError::EvalError(expr.to_string(), e)),
        }
    }

    #[tracing::instrument(skip(self, scope), err)]
    fn interpolate_value(&self, value: &mut serde_json::Value, scope: &mut Scope) -> CollectorResult<()> {
        match value {
            serde_json::Value::String(s) => {
                let rendered = self.eval(scope, s)?;
                *value = serde_json::Value::String(rendered);
            }
            serde_json::Value::Array(a) => {
                for v in a {
                    self.interpolate_value(v, scope)?;
                }
            }
            serde_json::Value::Object(o) => {
                for (_, v) in o {
                    self.interpolate_value(v, scope)?;
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
    use crate::inner::model::peripheral_key::PeripheralKey;

    use super::*;

    #[test]
    fn test_mqtt_payload_interpolation() {
        let config = json! {{
           "device_class":"`temperature`",
           "state_topic":"`${state_topic}`",
           "unit_of_measurement":"`°C`",
           "value_template":"`{{ value_json.temperature }}`",
           "unique_id":"`temp01ae`",
           "device":{
              "identifiers":[r###"
                  switch ctx.fqcn.peripheral {
                    "11:22:33:44:55:66" => `${ctx.clean_service_name.to_lower()}-${ctx.clean_characteristic_name.to_lower()}-${ctx.clean_peripheral_name.to_lower()}`,
                    _ => "unknown"
                  }
              "###,
                ],
              "name":"`name`",
           }
        }};
        let config: serde_yaml::Value = serde_json::from_value(config).unwrap();

        let fqcn = Arc::new(Fqcn {
            peripheral: "11:22:33:44:55:66".parse().unwrap(),
            service: "0000180f-0000-1000-8000-00805f9b34fb".parse().unwrap(),
            characteristic: "00002a19-0000-1000-8000-00805f9b34fb".parse().unwrap(),
        });

        let mqtt_conf = PublishMqttConfigDto {
            state_topic: Arc::new("`test-${ctx.clean_fqcn.peripheral}`".to_string()),
            unit: Some(Arc::new("`test-${ctx.peripheral}-test`".to_string())),
            retain: true,
            qos: Default::default(),
            discovery: Some(Arc::new(DiscoverySettings {
                config_topic: Arc::new("`config-test-${ctx.clean_fqcn.peripheral}`".to_string()),
                retain: Default::default(),
                qos: Default::default(),
                remainder: config,
            })),
        };

        let char_conf = Arc::new(CharacteristicConfig::Subscribe {
            name: Some("name test".to_string().into()),
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
            conf: char_conf.clone(),
            adapter_info: Arc::new(AdapterInfo {
                id: "hci0".to_string(),
                modalias: "smth".to_string(),
            }),
        };

        let peripheral_key = Arc::new(PeripheralKey {
            adapter_id: "hci0".to_string(),
            peripheral_address: "11:22:33:44:55:66".parse().unwrap(),
            name: Some("Name Different Case".to_string()),
        });

        let interpolator = MqttInterpolator::default();
        let request = ConnectPeripheralRequest {
            peripheral_key: peripheral_key.clone(),
            fqcn: fqcn.clone(),
            conf: char_conf.clone(),
        };
        let mqtt_payload = interpolator.interpolate_discovery(request).unwrap();

        assert_eq!(
            mqtt_payload,
            MqttDiscoveryPayload {
                config_topic: "config-test-11_22_33_44_55_66".to_string(),
                retain: true,
                qos: QoS::AtLeastOnce,
                discovery_config: Some(json! {{
                    "device": {
                      "identifiers": [
                        "service-name-test-name_test-name_different_case"
                      ],
                      "name": "name"
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
