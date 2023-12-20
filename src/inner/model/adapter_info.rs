use anyhow::Context;
use serde::{Deserialize, Serialize};
use std::fmt::{Display, Formatter};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub(crate) struct AdapterInfo {
    pub(crate) id: String,
    pub(crate) modalias: String,
}

impl Display for AdapterInfo {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}[{}]", self.id, self.modalias)
    }
}

impl TryFrom<String> for AdapterInfo {
    type Error = anyhow::Error;

    fn try_from(value: String) -> Result<Self, Self::Error> {
        let mut pair = value.split_whitespace();
        let id = pair.next().context("No id")?.to_string();
        let modalias = pair.next().context("No modalias")?.trim();
        let modalias = modalias.strip_prefix('(').unwrap_or(modalias);
        let modalias = modalias.strip_suffix(')').unwrap_or(modalias);
        let modalias = modalias.to_string();
        Ok(Self { id, modalias })
    }
}
