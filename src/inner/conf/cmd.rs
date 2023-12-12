use std::path::PathBuf;

use crate::inner::conf::dto::collector_configuration::CollectorConfigurationDto;
use clap::Parser;

use crate::inner::error::CollectorError;

#[derive(Parser, Debug)]
#[command(
    author,
    version,
    about,
    long_about = r###"BLE collectoor
"###
)]
pub(crate) struct CmdArgs {
    /// A directory where configs is located.
    #[arg(long)]
    pub(crate) config: PathBuf,

    /// Server listen address
    #[arg(long, default_value = "127.0.0.1")]
    pub(crate) listen_address: String,

    /// Server listen port
    #[arg(long, default_value_t = 8000)]
    pub(crate) port: u16,
}

impl TryFrom<&CmdArgs> for CollectorConfigurationDto {
    type Error = CollectorError;

    fn try_from(value: &CmdArgs) -> Result<Self, Self::Error> {
        let config = std::fs::read_to_string(value.config.clone())?;
        let config: CollectorConfigurationDto = serde_yaml::from_str(&config)?;
        Ok(config)
    }
}
