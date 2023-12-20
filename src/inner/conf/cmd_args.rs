use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;

use clap::Parser;

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
    /// A directory where configs is located.
    #[arg(long)]
    pub(crate) config: PathBuf,

    /// Server listen address.
    #[arg(long, default_value = "127.0.0.1")]
    pub(crate) listen_address: Arc<String>,

    /// Server listen port.
    #[arg(long, default_value_t = 8000)]
    pub(crate) port: u16,

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
}

impl TryFrom<&AppConf> for CollectorConfigurationDto {
    type Error = CollectorError;

    fn try_from(value: &AppConf) -> Result<Self, Self::Error> {
        let config = std::fs::read_to_string(value.config.clone())?;
        let config: CollectorConfigurationDto = serde_yaml::from_str(&config)?;
        Ok(config)
    }
}
