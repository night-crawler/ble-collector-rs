use std::net::SocketAddr;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;

use anyhow::Context;
use clap::Parser;
use rumqttc::v5::MqttOptions;

use crate::inner::conf::dto::collector_configuration::CollectorConfigurationDto;
use crate::inner::error::CollectorError;

#[derive(Parser, Debug)]
#[command(
    author,
    version,
    about,
    long_about = r###"BLE collectoor
"###
)]
pub(crate) struct AppConf {
    /// A directory where config is located.
    #[arg(long)]
    pub(crate) config: PathBuf,

    /// Server listen address.
    #[arg(long, default_value = "127.0.0.1:8000")]
    pub(crate) listen_address: SocketAddr,

    /// Throttle events for the same peripheral for at least this time in milliseconds.
    #[arg(long, value_parser = humantime::parse_duration, default_value = "30s")]
    pub(crate) event_throttling: Duration,

    /// Throttle purge samples
    #[arg(long, default_value = "100")]
    pub(crate) event_throttling_purge_samples: usize,

    /// Purge cache threshold.
    #[arg(long, default_value = "0.25")]
    pub(crate) event_throttling_purge_threshold: f64,

    /// Store retrieved/discovered peripherals for at least this time in milliseconds in the internal peripheral cache.
    #[arg(long, value_parser = humantime::parse_duration, default_value = "60s")]
    pub(crate) peripheral_cache_ttl: Duration,

    /// Default characteristic read timeout.
    #[arg(long, value_parser = humantime::parse_duration, default_value = "5s")]
    pub(crate) default_read_timeout: Duration,

    /// Default characteristic write timeout.
    #[arg(long, value_parser = humantime::parse_duration, default_value = "5s")]
    pub(crate) default_write_timeout: Duration,

    /// Default multi-batch parallelism for characteristic I/O.
    #[arg(long, default_value = "1")]
    pub(crate) default_multi_batch_parallelism: usize,

    /// Default batch parallelism for characteristic I/O.
    #[arg(long, default_value = "1")]
    pub(crate) default_batch_parallelism: usize,

    /// Service discovery parallelism.
    #[arg(long, default_value = "4")]
    pub(crate) service_discovery_parallelism: usize,

    /// Default peripheral connect timeout.
    #[arg(long, value_parser = humantime::parse_duration, default_value = "30s")]
    pub(crate) peripheral_connect_timeout: Duration,

    /// Metrics idle timeout. Metric is removed if no data received for this time.
    #[arg(long, value_parser = humantime::parse_duration, default_value = "5m")]
    pub(crate) metrics_idle_timeout: Duration,

    /// Notification stream read timeout. Restart the stream if no data received for this time.
    #[arg(long, value_parser = humantime::parse_duration, default_value = "5m")]
    pub(crate) notification_stream_read_timeout: Duration,

    /// MQTT broker address, i.e. localhost:1883
    #[clap(long)]
    pub(crate) mqtt_address: Option<SocketAddr>,

    /// MQTT broker username.
    #[arg(long, requires = "mqtt_address", requires = "mqtt_password")]
    pub(crate) mqtt_username: Option<Arc<String>>,

    /// MQTT broker password.
    #[arg(long, requires = "mqtt_address", requires = "mqtt_username")]
    pub(crate) mqtt_password: Option<Arc<String>>,

    /// MQTT client id.
    #[arg(long, requires = "mqtt_address", default_value = "ble-collector")]
    pub(crate) mqtt_id: Arc<String>,

    /// MQTT client id.
    #[arg(long, requires = "mqtt_address", value_parser = humantime::parse_duration, default_value = "10s")]
    pub(crate) mqtt_keepalive: Duration,

    /// MQTT cap is the capacity of the bounded async channel.
    #[arg(long, requires = "mqtt_address", default_value = "1000")]
    pub(crate) mqtt_cap: usize,
}

impl TryFrom<&AppConf> for CollectorConfigurationDto {
    type Error = CollectorError;

    fn try_from(value: &AppConf) -> Result<Self, Self::Error> {
        let config = std::fs::read_to_string(value.config.clone())?;
        let config: CollectorConfigurationDto = serde_yaml::from_str(&config)?;
        Ok(config)
    }
}

impl TryFrom<&AppConf> for MqttOptions {
    type Error = anyhow::Error;

    fn try_from(value: &AppConf) -> Result<Self, Self::Error> {
        let sock_addr = value.mqtt_address.context("No mqtt broker address was specified")?;
        let mut mqtt_options = MqttOptions::new(value.mqtt_id.as_ref(), sock_addr.ip().to_string(), sock_addr.port());

        mqtt_options.set_keep_alive(value.mqtt_keepalive);

        if let (Some(username), Some(password)) = (value.mqtt_username.as_ref(), value.mqtt_password.as_ref()) {
            mqtt_options.set_credentials(username.as_str(), password.as_str());
        }
        Ok(mqtt_options)
    }
}
