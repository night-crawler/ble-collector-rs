use rocket::serde::Serialize;

#[derive(Debug, Eq, PartialEq, Serialize)]
pub(crate) struct MqttDiscoveryPayload {
    pub(crate) config_topic: String,
    pub(crate) discovery_config: Option<serde_json::Value>,
}
