use std::path::PathBuf;

use clap::Parser;

use crate::inner::conf::parse::CollectorConfigurationDto;
use crate::inner::error::CollectorError;

#[derive(Parser, Debug)]
#[command(
    author,
    version,
    about,
    long_about = r###"BLE collectoor
"###
)]
pub(crate) struct Args {
    /// A directory where configs is located.
    #[arg(long)]
    config: PathBuf,
}

impl TryFrom<Args> for CollectorConfigurationDto {
    type Error = CollectorError;

    fn try_from(value: Args) -> Result<Self, Self::Error> {
        let config = std::fs::read_to_string(value.config)?;
        let config: CollectorConfigurationDto = serde_yaml::from_str(&config)?;
        Ok(config)
    }
}
