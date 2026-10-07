//! Model names and algorithms documented by pyasic 0.79's Goldshell model classes.
//! Firmware version or vendor identity alone never establishes an algorithm.
use std::str::FromStr;

use asic_rs_core::{
    data::device::{HashAlgorithm, MinerHardware},
    errors::ModelSelectionError,
    traits::model::{MinerModel, MinerModelAlgorithm},
};
use serde::{Deserialize, Serialize};
use strum::Display;
use ts_rs::TS;

#[derive(Debug, PartialEq, Eq, Clone, Hash, Serialize, Deserialize, Display, TS)]
pub enum GoldshellModel {
    CK5,
    HS5,
    KD5,
    KDMax,
    KDBoxII,
    KDBoxPro,
    MiniDoge,
    Byte,
    #[strum(to_string = "{0}")]
    Unknown(String),
}

impl FromStr for GoldshellModel {
    type Err = ModelSelectionError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let raw = s.trim();
        if raw.is_empty() {
            return Err(ModelSelectionError::UnexpectedModelResponse);
        }
        let normalized = raw.to_ascii_uppercase().replace([' ', '-', '_'], "");
        let name = normalized.strip_prefix("GOLDSHELL").unwrap_or(&normalized);
        Ok(match name {
            "CK5" => Self::CK5,
            "HS5" => Self::HS5,
            "KD5" => Self::KD5,
            "KDMAX" => Self::KDMax,
            "KDBOXII" => Self::KDBoxII,
            "KDBOXPRO" => Self::KDBoxPro,
            "MINIDOGE" => Self::MiniDoge,
            "BYTE" => Self::Byte,
            _ => Self::Unknown(raw.to_string()),
        })
    }
}

impl MinerModelAlgorithm for GoldshellModel {
    fn hash_algorithm(&self) -> HashAlgorithm {
        match self {
            Self::CK5 => HashAlgorithm::Eaglesong,
            Self::HS5 => HashAlgorithm::Handshake,
            Self::KD5 | Self::KDMax | Self::KDBoxII | Self::KDBoxPro => HashAlgorithm::Kadena,
            Self::MiniDoge => HashAlgorithm::Scrypt,
            // Byte is a multi-device family; pyasic uses GenericAlgo rather
            // than one algorithm for the whole chassis.
            Self::Byte | Self::Unknown(_) => HashAlgorithm::Unknown,
        }
    }
}

impl MinerModel for GoldshellModel {
    fn make_name(&self) -> String {
        "Goldshell".to_string()
    }
    fn is_known(&self) -> bool {
        !matches!(self, Self::Unknown(_))
    }
}

impl From<GoldshellModel> for MinerHardware {
    fn from(_: GoldshellModel) -> Self {
        // Hardware counts require model-specific authoritative evidence; live
        // working counts are collected separately from the read API.
        Self::default()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn exact_model_names_establish_algorithms_but_generic_vendor_does_not() {
        assert_eq!(
            GoldshellModel::from_str("Goldshell-CK5")
                .unwrap()
                .hash_algorithm(),
            HashAlgorithm::Eaglesong
        );
        assert_eq!(
            GoldshellModel::from_str("Goldshell")
                .unwrap()
                .hash_algorithm(),
            HashAlgorithm::Unknown
        );
        assert_eq!(
            GoldshellModel::from_str("new-model").unwrap(),
            GoldshellModel::Unknown("new-model".into())
        );
        assert_eq!(
            MinerHardware::from(GoldshellModel::CK5),
            MinerHardware::default()
        );
    }
}
