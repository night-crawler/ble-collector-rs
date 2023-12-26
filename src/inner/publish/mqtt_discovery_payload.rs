use rumqttc::v5::mqttbytes::QoS;

#[derive(Debug, Eq, PartialEq)]
pub(crate) struct MqttDiscoveryPayload {
    pub(crate) config_topic: String,
    pub(crate) retain: bool,
    pub(crate) qos: QoS,
    pub(crate) discovery_config: Option<serde_json::Value>,
}
