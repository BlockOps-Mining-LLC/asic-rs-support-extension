// SPDX-License-Identifier: Apache-2.0
use crate::{backend::KaonsuMiner, telemetry::payload, web::KaonsuWebAPI};
use asic_rs_core::{
    data::command::DiscoveryCommand,
    discovery::{HTTP_WEB_ROOT, RPC_VERSION},
    errors::ModelSelectionError,
    traits::{
        discovery::DiscoveryCommands,
        entry::FirmwareEntry,
        firmware::MinerFirmware,
        identification::{FirmwareIdentification, WebResponse},
        miner::{HasDefaultAuth, Miner, MinerAuth},
    },
};
use asic_rs_makes_antminer::models::AntMinerModel;
use async_trait::async_trait;
use serde_json::Value;
use std::{fmt::Display, net::IpAddr, str::FromStr};

#[derive(Default, Debug)]
pub struct KaonsuFirmware;
impl Display for KaonsuFirmware {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("KaonSu")
    }
}
impl DiscoveryCommands for KaonsuFirmware {
    fn get_discovery_commands(&self) -> Vec<DiscoveryCommand> {
        vec![
            RPC_VERSION,
            HTTP_WEB_ROOT,
            DiscoveryCommand::Web {
                command: "/kaonsu/v1/brief",
                port: 80,
            },
        ]
    }
}
fn marker(value: &str) -> bool {
    let value = value.to_ascii_lowercase();
    value.contains("marafw") || value.contains("mara fw") || value.contains("kaonsu")
}
impl FirmwareIdentification for KaonsuFirmware {
    fn identify_rpc(&self, response: &str) -> bool {
        marker(response)
    }
    fn identify_web(&self, response: &WebResponse<'_>) -> bool {
        // The sanitized authentication fixture proves MaraFW's realm.
        // MD5 and a KS model alone are not firmware identification.
        marker(response.auth_header) || marker(response.body)
    }
}
pub(crate) fn model_from_overview(value: &Value) -> Result<AntMinerModel, ModelSelectionError> {
    let overview = payload(value, None).ok_or(ModelSelectionError::UnexpectedModelResponse)?;
    let model = ["model_extended", "model"]
        .into_iter()
        .find_map(|key| {
            overview
                .get(key)
                .and_then(Value::as_str)
                .map(str::trim)
                .filter(|model| !model.is_empty())
        })
        .ok_or(ModelSelectionError::UnexpectedModelResponse)?;
    AntMinerModel::from_str(model)
}
fn matches_brief_contract(value: &Value) -> bool {
    payload(value, None).is_some_and(|brief| {
        [
            "hashrate_average",
            "hashrate_realtime",
            "hashrate_realtime_10m",
            "power_consumption_estimated",
            "work_mode",
            "status",
        ]
        .into_iter()
        .filter(|key| brief.get(key).is_some())
        .count()
            >= 2
    })
}
async fn read_identity(ip: IpAddr, auth: MinerAuth) -> Result<AntMinerModel, ModelSelectionError> {
    let api = KaonsuWebAPI::new(ip, auth);
    // Both dedicated KaonSu paths must succeed with the evidenced schema.
    // A bare model label from unrelated stock firmware cannot authorize it.
    let brief = api
        .get("brief")
        .await
        .map_err(|_| ModelSelectionError::NoModelResponse)?;
    if !matches_brief_contract(&brief) {
        return Err(ModelSelectionError::UnexpectedModelResponse);
    }
    let overview = api
        .get("overview")
        .await
        .map_err(|_| ModelSelectionError::NoModelResponse)?;
    model_from_overview(&overview)
}
#[async_trait]
impl MinerFirmware for KaonsuFirmware {
    type Model = AntMinerModel;
    async fn get_model(ip: IpAddr) -> Result<Self::Model, ModelSelectionError> {
        read_identity(ip, KaonsuMiner::default_auth()).await
    }
    async fn get_version(_ip: IpAddr) -> Option<semver::Version> {
        None
    }
}
#[async_trait]
impl FirmwareEntry for KaonsuFirmware {
    async fn build_miner(
        &self,
        ip: IpAddr,
        auth: Option<&MinerAuth>,
    ) -> Result<Box<dyn Miner>, ModelSelectionError> {
        let auth = auth.cloned().unwrap_or_else(KaonsuMiner::default_auth);
        let model = read_identity(ip, auth.clone()).await?;
        Ok(Box::new(KaonsuMiner::with_auth(ip, model, auth)))
    }
}
