use std::str::FromStr;

use asic_rs_core::{
    data::device::HashAlgorithm, errors::ModelSelectionError, traits::model::MinerModel,
};
use asic_rs_macros::ModelAlgorithm;
use serde::{Deserialize, Serialize};
use strum::Display;
use ts_rs::TS;

#[derive(
    Debug, PartialEq, Eq, Clone, Hash, Serialize, Deserialize, Display, ModelAlgorithm, TS,
)]
pub enum IceRiverModel {
    #[algorithm(HashAlgorithm::Blake3)]
    #[serde(alias = "10306", alias = "ICERIVER AL3")]
    AL3,
    #[algorithm(HashAlgorithm::KHeavyHash)]
    KS0,
    #[algorithm(HashAlgorithm::KHeavyHash)]
    KS1,
    #[algorithm(HashAlgorithm::KHeavyHash)]
    KS2,
    #[algorithm(HashAlgorithm::KHeavyHash)]
    KS3,
    #[algorithm(HashAlgorithm::KHeavyHash)]
    KS3L,
    #[algorithm(HashAlgorithm::KHeavyHash)]
    KS3M,
    #[algorithm(HashAlgorithm::KHeavyHash)]
    KS5,
    #[algorithm(HashAlgorithm::KHeavyHash)]
    KS5L,
    #[algorithm(HashAlgorithm::KHeavyHash)]
    KS5M,
    #[strum(to_string = "{0}")]
    #[algorithm(HashAlgorithm::Unknown)]
    Unknown(String),
}

impl FromStr for IceRiverModel {
    type Err = ModelSelectionError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let normalized = s.trim().to_ascii_uppercase();
        let normalized = normalized.strip_prefix("ICERIVER ").unwrap_or(&normalized);
        serde_json::from_value(serde_json::Value::String(normalized.to_owned()))
            .or_else(|_| Ok(Self::Unknown(s.to_owned())))
    }
}

impl MinerModel for IceRiverModel {
    fn make_name(&self) -> String {
        "IceRiver".to_owned()
    }

    fn is_known(&self) -> bool {
        !matches!(self, Self::Unknown(_))
    }
}

#[cfg(test)]
mod tests {
    use asic_rs_core::traits::model::MinerModelAlgorithm;

    use super::*;

    #[test]
    fn recognizes_software_model_code_without_guessing_unknown_algorithms() {
        for alias in ["AL3", "10306", "IceRiver AL3", " al3 "] {
            let model = IceRiverModel::from_str(alias).unwrap();
            assert_eq!(model, IceRiverModel::AL3);
            assert_eq!(model.hash_algorithm(), HashAlgorithm::Blake3);
        }
        let model = IceRiverModel::from_str("future model").unwrap();
        assert!(!model.is_known());
        assert_eq!(model.hash_algorithm(), HashAlgorithm::Unknown);
    }
}
