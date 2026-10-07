//! Exact algorithms from pyasic 0.79's Innosilicon model classes.
//! The fleet's `a9-1.2.0` firmware is not sufficient to identify an A9 variant.
use asic_rs_core::{
    data::device::{HashAlgorithm, MinerHardware},
    errors::ModelSelectionError,
    traits::model::{MinerModel, MinerModelAlgorithm},
};
use serde::{Deserialize, Serialize};
use std::str::FromStr;
use strum::Display;
use ts_rs::TS;

#[derive(Debug, PartialEq, Eq, Clone, Hash, Serialize, Deserialize, Display, TS)]
pub enum InnosiliconModel {
    #[strum(to_string = "T3H+")]
    T3HPlus,
    A10X,
    A11,
    A11MX,
    #[strum(to_string = "{0}")]
    Unknown(String),
}
impl FromStr for InnosiliconModel {
    type Err = ModelSelectionError;
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let raw = s.trim();
        if raw.is_empty() {
            return Err(ModelSelectionError::UnexpectedModelResponse);
        }
        let normalized = raw.to_ascii_uppercase().replace([' ', '-'], "");
        let name = normalized
            .strip_prefix("INNOSILICON")
            .unwrap_or(&normalized);
        Ok(match name {
            "T3H+" => Self::T3HPlus,
            "A10X" => Self::A10X,
            "A11" => Self::A11,
            "A11MX" => Self::A11MX,
            _ => Self::Unknown(raw.to_string()),
        })
    }
}
impl MinerModelAlgorithm for InnosiliconModel {
    fn hash_algorithm(&self) -> HashAlgorithm {
        match self {
            Self::T3HPlus => HashAlgorithm::SHA256,
            Self::A10X | Self::A11 | Self::A11MX => HashAlgorithm::EtHash,
            Self::Unknown(_) => HashAlgorithm::Unknown,
        }
    }
}
impl MinerModel for InnosiliconModel {
    fn make_name(&self) -> String {
        "Innosilicon".to_string()
    }
    fn is_known(&self) -> bool {
        !matches!(self, Self::Unknown(_))
    }
}
impl From<InnosiliconModel> for MinerHardware {
    fn from(_: InnosiliconModel) -> Self {
        Self::default()
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn firmware_and_generic_vendor_never_imply_algorithm_or_hardware() {
        for raw in ["Innosilicon", "a9-1.2.0", "A9", "A9++"] {
            let model = InnosiliconModel::from_str(raw).unwrap();
            assert_eq!(model.hash_algorithm(), HashAlgorithm::Unknown);
            assert_eq!(MinerHardware::from(model), MinerHardware::default());
        }
    }
}
