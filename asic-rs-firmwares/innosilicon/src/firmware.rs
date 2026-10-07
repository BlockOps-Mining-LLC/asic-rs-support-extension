use crate::{backend::InnosiliconV1, web::InnosiliconWebAPI};
use asic_rs_core::{
    data::command::DiscoveryCommand,
    discovery::{HTTP_WEB_ROOT, RPC_VERSION},
    errors::ModelSelectionError,
    traits::{
        discovery::DiscoveryCommands,
        entry::FirmwareEntry,
        firmware::MinerFirmware,
        identification::{FirmwareIdentification, WebResponse},
        miner::{HasAuth, HasDefaultAuth, Miner, MinerAuth},
    },
    util::send_rpc_command,
};
use asic_rs_makes_innosilicon::models::InnosiliconModel;
use async_trait::async_trait;
use serde_json::Value;
use std::{fmt::Display, net::IpAddr, str::FromStr};

#[derive(Default, Debug)]
pub struct InnosiliconFirmware;
impl Display for InnosiliconFirmware {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "Innosilicon Stock (read-only)")
    }
}
impl DiscoveryCommands for InnosiliconFirmware {
    fn get_discovery_commands(&self) -> Vec<DiscoveryCommand> {
        vec![HTTP_WEB_ROOT, RPC_VERSION]
    }
}
pub fn model_from_type(response: &Value) -> Result<InnosiliconModel, ModelSelectionError> {
    let raw = response
        .get("type")
        .and_then(Value::as_str)
        .filter(|s| !s.trim().is_empty())
        .ok_or(ModelSelectionError::UnexpectedModelResponse)?;
    InnosiliconModel::from_str(raw)
}
#[async_trait]
impl MinerFirmware for InnosiliconFirmware {
    type Model = InnosiliconModel;
    async fn get_model(ip: IpAddr) -> Result<Self::Model, ModelSelectionError> {
        let value = InnosiliconWebAPI::new(ip, InnosiliconV1::default_auth())
            .read("type")
            .await
            .map_err(|_| ModelSelectionError::NoModelResponse)?;
        model_from_type(&value)
    }
    async fn get_version(ip: IpAddr) -> Option<semver::Version> {
        let value = send_rpc_command(&ip, "version").await?;
        let raw = value.pointer("/VERSION/0/CGMiner")?.as_str()?;
        let candidate = raw.rsplit('-').next()?.trim_start_matches('v');
        semver::Version::parse(candidate).ok()
    }
}
impl FirmwareIdentification for InnosiliconFirmware {
    fn identify_rpc(&self, response: &str) -> bool {
        let response = response.to_ascii_lowercase();
        response.contains("innosilicon") || response.contains("innominer")
    }
    fn identify_web(&self, response: &WebResponse<'_>) -> bool {
        let body = response.body.to_ascii_lowercase();
        matches!(response.status, 200 | 401 | 403)
            && (body.contains("innosilicon")
                || body.contains("dragonmint")
                || response
                    .auth_header
                    .to_ascii_lowercase()
                    .contains("innosilicon"))
    }
    fn is_stock(&self) -> bool {
        true
    }
}
#[async_trait]
impl FirmwareEntry for InnosiliconFirmware {
    async fn build_miner(
        &self,
        ip: IpAddr,
        auth: Option<&MinerAuth>,
    ) -> Result<Box<dyn Miner>, ModelSelectionError> {
        let resolved = auth.cloned().unwrap_or_else(InnosiliconV1::default_auth);
        let value = InnosiliconWebAPI::new(ip, resolved.clone())
            .read("type")
            .await;
        let model = value
            .as_ref()
            .ok()
            .and_then(|v| model_from_type(v).ok())
            .unwrap_or_else(|| InnosiliconModel::Unknown("Innosilicon".into()));
        let mut miner = InnosiliconV1::new(ip, model);
        miner.set_auth(resolved);
        Ok(Box::new(miner))
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    use asic_rs_core::{data::device::HashAlgorithm, traits::model::MinerModelAlgorithm};
    #[test]
    fn raw_type_is_preserved_and_firmware_cannot_establish_a9_variant() {
        let value: Value =
            serde_json::from_str(include_str!("../tests/fixtures/type_contract.json")).unwrap();
        assert_eq!(
            model_from_type(&value).unwrap(),
            InnosiliconModel::Unknown("Innosilicon".into())
        );
        assert_eq!(
            model_from_type(&serde_json::json!({"type":"A9"}))
                .unwrap()
                .hash_algorithm(),
            HashAlgorithm::Unknown
        );
        assert!(model_from_type(&serde_json::json!({"firmware":"a9-1.2.0"})).is_err());
    }
}
