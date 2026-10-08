//! Supported Goldshell model identities.
//! Firmware version or vendor identity alone never establishes an algorithm.
use std::str::FromStr;

use asic_rs_core::{
    data::device::HashAlgorithm,
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
    SC5Pro,
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
            "SC5PRO" => Self::SC5Pro,
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
            Self::SC5Pro => HashAlgorithm::Blake2b,
            // Byte is a multi-device family rather than one algorithm.
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn retained_model_identities_keep_algorithms_without_expected_hardware() {
        for (name, expected, algorithm) in [
            (
                "Goldshell-CK5",
                GoldshellModel::CK5,
                HashAlgorithm::Eaglesong,
            ),
            ("HS5", GoldshellModel::HS5, HashAlgorithm::Handshake),
            ("KD5", GoldshellModel::KD5, HashAlgorithm::Kadena),
            ("KD-Max", GoldshellModel::KDMax, HashAlgorithm::Kadena),
            ("KD Box II", GoldshellModel::KDBoxII, HashAlgorithm::Kadena),
            ("KDBoxPro", GoldshellModel::KDBoxPro, HashAlgorithm::Kadena),
            ("Mini-Doge", GoldshellModel::MiniDoge, HashAlgorithm::Scrypt),
            ("Byte", GoldshellModel::Byte, HashAlgorithm::Unknown),
            ("SC5Pro", GoldshellModel::SC5Pro, HashAlgorithm::Blake2b),
        ] {
            let model = GoldshellModel::from_str(name).unwrap();
            assert_eq!(model, expected);
            assert_eq!(model.hash_algorithm(), algorithm);
            assert_eq!(
                asic_rs_core::data::device::MinerHardware::from(model),
                asic_rs_core::data::device::MinerHardware::default()
            );
        }
    }

    #[test]
    fn unverified_sc5_variants_and_ari31_keep_unknown_algorithms() {
        for name in ["SC5", "SC5ProX", "Goldshell-ARI31", "Goldshell"] {
            assert_eq!(
                GoldshellModel::from_str(name).unwrap().hash_algorithm(),
                HashAlgorithm::Unknown
            );
        }
    }
}
