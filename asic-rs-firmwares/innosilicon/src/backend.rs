use crate::{firmware::InnosiliconFirmware, web::InnosiliconWebAPI};
use asic_rs_core::{
    config::collector::{ConfigCollector, ConfigField, ConfigLocation},
    data::{
        board::BoardData,
        collector::{DataCollector, DataExtractor, DataField, DataLocation, get_by_pointer},
        command::MinerCommand,
        device::{DeviceInfo, HashAlgorithm},
        fan::FanData,
        hashrate::{HashRate, HashRateUnit},
        message::{MessageSeverity, MinerMessage},
        pool::{PoolData, PoolGroupData, PoolURL},
    },
    traits::{miner::*, model::MinerModel},
    util::send_rpc_command,
};
use async_trait::async_trait;
use macaddr::MacAddr;
use measurements::{AngularVelocity, Power, Temperature};
use serde_json::Value;
use std::{
    collections::{BTreeMap, HashMap},
    net::IpAddr,
    str::FromStr,
    time::Duration,
};

#[derive(Debug)]
pub struct InnosiliconV1 {
    ip: IpAddr,
    web: InnosiliconWebAPI,
    device_info: DeviceInfo,
}
impl InnosiliconV1 {
    pub fn new(ip: IpAddr, model: impl MinerModel) -> Self {
        let algo = model.hash_algorithm();
        Self {
            ip,
            web: InnosiliconWebAPI::new(ip, Self::default_auth()),
            device_info: DeviceInfo::new(model, InnosiliconFirmware, algo),
        }
    }
    pub fn allows_rpc(command: &str) -> bool {
        matches!(command, "version" | "summary" | "stats" | "pools")
    }
}
fn number(value: Option<&Value>) -> Option<f64> {
    value
        .and_then(|v| v.as_f64().or_else(|| v.as_str()?.parse().ok()))
        .filter(|n| n.is_finite() && *n >= 0.0)
}
fn integer(value: Option<&Value>) -> Option<u64> {
    value.and_then(|v| v.as_u64().or_else(|| v.as_str()?.parse().ok()))
}
fn string(data: &HashMap<DataField, Value>, field: DataField) -> Option<String> {
    data.get(&field)?.as_str().map(str::to_string)
}
fn first_elapsed<'a>(data: &'a Value, _: Option<&str>) -> Option<&'a Value> {
    data.get("STATS")?
        .as_array()?
        .iter()
        .find_map(|r| r.get("Elapsed"))
}
fn location(
    command: MinerCommand,
    pointer: &'static str,
    tag: Option<&'static str>,
) -> DataLocation {
    (
        command,
        DataExtractor {
            func: get_by_pointer,
            key: Some(pointer),
            tag,
        },
    )
}
fn rpc(command: &'static str) -> MinerCommand {
    MinerCommand::RPC {
        command,
        parameters: None,
    }
}
fn web(command: &'static str) -> MinerCommand {
    MinerCommand::WebAPI {
        command,
        parameters: None,
    }
}
fn rate(value: f64, unit: HashRateUnit, algo: HashAlgorithm) -> HashRate {
    let rate = HashRate { value, unit, algo };
    if algo == HashAlgorithm::Unknown {
        rate
    } else {
        rate.as_default_unit()
    }
}
fn declared_hashrate(data: &Value, algo: HashAlgorithm, allow_average: bool) -> Option<HashRate> {
    if let Some(total) = data.get("all") {
        if let Some(value) = number(total.get("Hash Rate H")) {
            return Some(rate(value, HashRateUnit::Hash, algo));
        }
        if let Some(value) = number(total.get("Hash Rate")) {
            return Some(rate(value, HashRateUnit::MegaHash, algo));
        }
    }
    let summary = data.get("summary")?;
    let value = ["MHS 1m", "MHS 5s", "MHS 20s"]
        .into_iter()
        .find_map(|k| number(summary.get(k)))
        .or_else(|| {
            allow_average
                .then(|| number(summary.get("MHS av")))
                .flatten()
        })?;
    Some(rate(value, HashRateUnit::MegaHash, algo))
}
fn reported_power(data: &HashMap<DataField, Value>) -> Option<(Power, String)> {
    let raw = data.get(&DataField::Wattage)?;
    if let Some(watts) = number(raw.get("all")) {
        return Some((
            Power::from_watts(watts),
            "innosilicon.web:/api/getAll#/all/power".to_owned(),
        ));
    }
    // Retain the row index so provenance identifies the exact reported field.
    // As in the source contract, select the first row containing `power`.
    let (position, row) = raw
        .get("stats")?
        .as_array()?
        .iter()
        .enumerate()
        .find(|(_, row)| row.get("power").is_some())?;
    Some((
        Power::from_watts(number(row.get("power"))?),
        format!("innosilicon.rpc:stats#/STATS/{position}/power"),
    ))
}
#[async_trait]
impl APIClient for InnosiliconV1 {
    async fn get_api_result(&self, command: &MinerCommand) -> anyhow::Result<Value> {
        match command {
            MinerCommand::RPC {
                command,
                parameters: None,
            } if Self::allows_rpc(command) => send_rpc_command(&self.ip, command)
                .await
                .ok_or_else(|| anyhow::anyhow!("Innosilicon read RPC failed")),
            MinerCommand::WebAPI {
                command,
                parameters: None,
            } if InnosiliconWebAPI::allows_read(command) => self.web.read(command).await,
            _ => anyhow::bail!(
                "Innosilicon telemetry backend accepts read-only commands without parameters"
            ),
        }
    }
}
impl GetDataLocations for InnosiliconV1 {
    fn get_locations(&self, field: DataField) -> Vec<DataLocation> {
        match field {
            DataField::Mac => vec![
                location(web("getAll"), "/all/mac", None),
                location(web("overview"), "/version/ethaddr", None),
            ],
            DataField::ApiVersion => vec![location(rpc("version"), "/VERSION/0/API", None)],
            DataField::FirmwareVersion => {
                vec![location(rpc("version"), "/VERSION/0/CGMiner", None)]
            }
            DataField::Hashrate | DataField::IsMining => vec![
                location(web("getAll"), "/all/total_hash", Some("all")),
                location(rpc("summary"), "/SUMMARY/0", Some("summary")),
            ],
            DataField::Hashboards => vec![
                location(web("getAll"), "/all", Some("all")),
                location(rpc("stats"), "/STATS", Some("stats")),
            ],
            DataField::Wattage => vec![
                location(web("getAll"), "/all/power", Some("all")),
                location(rpc("stats"), "/STATS", Some("stats")),
            ],
            DataField::Fans => vec![location(rpc("stats"), "/STATS", None)],
            DataField::Messages => vec![location(web("getErrorDetail"), "/code", None)],
            DataField::Uptime => vec![(
                rpc("stats"),
                DataExtractor {
                    func: first_elapsed,
                    key: None,
                    tag: None,
                },
            )],
            DataField::Pools => vec![location(rpc("pools"), "/POOLS", None)],
            _ => vec![],
        }
    }
}
impl GetConfigsLocations for InnosiliconV1 {
    fn get_configs_locations(&self, _: ConfigField) -> Vec<ConfigLocation> {
        vec![]
    }
}
impl CollectConfigs for InnosiliconV1 {
    fn get_config_collector(&self) -> ConfigCollector<'_> {
        ConfigCollector::new(self)
    }
}
impl CollectData for InnosiliconV1 {
    fn get_collector(&self) -> DataCollector<'_> {
        DataCollector::new(self)
    }
}
impl GetIP for InnosiliconV1 {
    fn get_ip(&self) -> IpAddr {
        self.ip
    }
}
impl GetDeviceInfo for InnosiliconV1 {
    fn get_device_info(&self) -> DeviceInfo {
        self.device_info.clone()
    }
}
impl GetMAC for InnosiliconV1 {
    fn parse_mac(&self, data: &HashMap<DataField, Value>) -> Option<MacAddr> {
        MacAddr::from_str(data.get(&DataField::Mac)?.as_str()?).ok()
    }
}
impl GetApiVersion for InnosiliconV1 {
    fn parse_api_version(&self, data: &HashMap<DataField, Value>) -> Option<String> {
        string(data, DataField::ApiVersion)
    }
}
impl GetFirmwareVersion for InnosiliconV1 {
    fn parse_firmware_version(&self, data: &HashMap<DataField, Value>) -> Option<String> {
        string(data, DataField::FirmwareVersion)
    }
}
impl GetHashrate for InnosiliconV1 {
    fn parse_hashrate(&self, data: &HashMap<DataField, Value>) -> Option<HashRate> {
        declared_hashrate(data.get(&DataField::Hashrate)?, self.device_info.algo, true)
    }
}
impl GetIsMining for InnosiliconV1 {
    fn parse_is_mining(&self, data: &HashMap<DataField, Value>) -> bool {
        data.get(&DataField::IsMining)
            .and_then(|v| declared_hashrate(v, self.device_info.algo, false))
            .is_some_and(|r| r.value > 0.0)
    }
}
impl GetUptime for InnosiliconV1 {
    fn parse_uptime(&self, data: &HashMap<DataField, Value>) -> Option<Duration> {
        integer(data.get(&DataField::Uptime)).map(Duration::from_secs)
    }
}
impl GetWattage for InnosiliconV1 {
    fn parse_wattage(&self, data: &HashMap<DataField, Value>) -> Option<Power> {
        // Follow the vendor read contract: use its reported total, never sum
        // ambiguous per-chain rows or replace absent readings with estimates.
        reported_power(data).map(|(power, _)| power)
    }
    fn parse_wattage_source(&self, data: &HashMap<DataField, Value>) -> Option<String> {
        reported_power(data).map(|(_, source)| source)
    }
}
impl GetHashboards for InnosiliconV1 {
    fn parse_hashboards(&self, data: &HashMap<DataField, Value>) -> Vec<BoardData> {
        let Some(raw) = data.get(&DataField::Hashboards) else {
            return vec![];
        };
        let mut boards: BTreeMap<u8, BoardData> = BTreeMap::new();
        for row in raw
            .get("stats")
            .and_then(Value::as_array)
            .into_iter()
            .flatten()
        {
            let Some(position) = integer(row.get("Chain ID")).and_then(|p| u8::try_from(p).ok())
            else {
                continue;
            };
            let board = boards
                .entry(position)
                .or_insert_with(|| BoardData::new(position, None));
            board.working_chips =
                integer(row.get("Num active chips")).and_then(|v| u16::try_from(v).ok());
        }
        for row in raw
            .pointer("/all/chain")
            .and_then(Value::as_array)
            .into_iter()
            .flatten()
        {
            let Some(position) = integer(row.get("ASC")).and_then(|p| u8::try_from(p).ok()) else {
                continue;
            };
            let board = boards
                .entry(position)
                .or_insert_with(|| BoardData::new(position, None));
            // Sensor roles match pyasic's Innosilicon read parser, pending live
            // verification of the exact firmware's sensor labels.
            board.board_temperature = number(row.get("Temp min")).map(Temperature::from_celsius);
            board.outlet_chip_temperature =
                number(row.get("Temp max")).map(Temperature::from_celsius);
            board.hashrate = number(row.get("Hash Rate H"))
                .map(|value| rate(value, HashRateUnit::Hash, self.device_info.algo));
            board.active = board.hashrate.as_ref().map(|r| r.value > 0.0);
        }
        boards.into_values().collect()
    }
}
impl GetFans for InnosiliconV1 {
    fn parse_fans(&self, data: &HashMap<DataField, Value>) -> Vec<FanData> {
        let mut fans: BTreeMap<i16, FanData> = BTreeMap::new();
        // `getAll.fansSpeed` is a percentage. There is no verified maximum
        // fan speed, so it cannot be converted to RPM with an assumed 6000.
        for row in data
            .get(&DataField::Fans)
            .and_then(Value::as_array)
            .into_iter()
            .flatten()
        {
            for (key, value) in row.as_object().into_iter().flatten() {
                let Some(position) = key
                    .strip_prefix("fan")
                    .and_then(|p| p.parse::<i16>().ok())
                    .filter(|p| *p >= 0)
                else {
                    continue;
                };
                if let Some(rpm) = number(Some(value)) {
                    fans.insert(
                        position,
                        FanData {
                            position,
                            rpm: Some(AngularVelocity::from_rpm(rpm)),
                        },
                    );
                }
            }
        }
        fans.into_values().collect()
    }
}
impl GetMessages for InnosiliconV1 {
    fn parse_messages(&self, data: &HashMap<DataField, Value>) -> Vec<MinerMessage> {
        let Some(code) = integer(data.get(&DataField::Messages)).filter(|c| *c != 0) else {
            return vec![];
        };
        vec![MinerMessage::new(
            0,
            code,
            format!("Innosilicon reported error code {code}"),
            MessageSeverity::Error,
        )]
    }
}
impl GetPools for InnosiliconV1 {
    fn parse_pools(&self, data: &HashMap<DataField, Value>) -> Vec<PoolGroupData> {
        let Some(rows) = data.get(&DataField::Pools).and_then(Value::as_array) else {
            return vec![];
        };
        let pools = rows
            .iter()
            .map(|row| PoolData {
                position: integer(row.get("POOL")).and_then(|v| u16::try_from(v).ok()),
                url: row
                    .get("URL")
                    .and_then(Value::as_str)
                    .map(|s| PoolURL::from(s.to_string())),
                user: row.get("User").and_then(Value::as_str).map(str::to_string),
                accepted_shares: integer(row.get("Accepted")),
                rejected_shares: integer(row.get("Rejected")),
                active: row.get("Stratum Active").and_then(Value::as_bool),
                alive: row
                    .get("Status")
                    .and_then(Value::as_str)
                    .map(|s| s.eq_ignore_ascii_case("Alive")),
                last_share_time: None,
            })
            .collect();
        vec![PoolGroupData {
            name: String::new(),
            quota: 1,
            pools,
        }]
    }
}

impl GetSerialNumber for InnosiliconV1 {}
impl GetHostname for InnosiliconV1 {}
impl GetControlBoardVersion for InnosiliconV1 {}
impl GetExpectedHashrate for InnosiliconV1 {}
impl GetPsuFans for InnosiliconV1 {}
impl GetFluidTemperature for InnosiliconV1 {}
impl GetTuningPercent for InnosiliconV1 {}
impl GetTuningTarget for InnosiliconV1 {}
impl GetScaledTuningTarget for InnosiliconV1 {}
impl GetTuningCapabilities for InnosiliconV1 {}
impl GetLightFlashing for InnosiliconV1 {}
impl GetBestShare for InnosiliconV1 {}
impl GetSessionBestShare for InnosiliconV1 {}
impl GetOperatingState for InnosiliconV1 {}
impl GetDevFeeConnected for InnosiliconV1 {}
impl SetFaultLight for InnosiliconV1 {
    fn supports_set_fault_light(&self) -> bool {
        false
    }
}
impl SetPowerLimit for InnosiliconV1 {
    fn supports_set_power_limit(&self) -> bool {
        false
    }
}
impl Restart for InnosiliconV1 {
    fn supports_restart(&self) -> bool {
        false
    }
}
impl Pause for InnosiliconV1 {
    fn supports_pause(&self) -> bool {
        false
    }
}
impl Resume for InnosiliconV1 {
    fn supports_resume(&self) -> bool {
        false
    }
}
impl ChangePassword for InnosiliconV1 {
    fn supports_change_password(&self) -> bool {
        false
    }
}
impl FactoryReset for InnosiliconV1 {
    fn supports_factory_reset(&self) -> bool {
        false
    }
}
impl ReadLogs for InnosiliconV1 {
    fn supports_read_logs(&self) -> bool {
        false
    }
}
impl UpgradeFirmware for InnosiliconV1 {
    fn supports_upgrade_firmware(&self) -> bool {
        false
    }
}
impl RestoreStockOs for InnosiliconV1 {}
impl SetHashboardsEnabled for InnosiliconV1 {}
impl SetTuningPercent for InnosiliconV1 {}
impl SupportsPresets for InnosiliconV1 {}
impl SupportsPoolsConfig for InnosiliconV1 {
    fn supports_pools_config(&self) -> bool {
        false
    }
}
impl SupportsScalingConfig for InnosiliconV1 {
    fn supports_scaling_config(&self) -> bool {
        false
    }
}
impl SupportsTemperatureConfig for InnosiliconV1 {}
impl SupportsTimezoneConfig for InnosiliconV1 {}
impl SupportsTuningConfig for InnosiliconV1 {}
impl SupportsFanConfig for InnosiliconV1 {}
impl HasAuth for InnosiliconV1 {
    fn set_auth(&mut self, auth: MinerAuth) {
        self.web.set_auth(auth);
    }
}
impl HasDefaultAuth for InnosiliconV1 {
    fn default_auth() -> MinerAuth {
        MinerAuth::new("admin", "admin")
    }
}
impl Validate for InnosiliconV1 {
    type Firmware = InnosiliconFirmware;
}

#[cfg(test)]
mod tests {
    use super::*;
    use asic_rs_makes_innosilicon::models::InnosiliconModel;
    use serde_json::json;
    fn miner() -> InnosiliconV1 {
        InnosiliconV1::new(
            IpAddr::from([127, 0, 0, 1]),
            InnosiliconModel::Unknown("Innosilicon".into()),
        )
    }
    fn fixture() -> Value {
        serde_json::from_str(include_str!("../tests/fixtures/telemetry_contract.json")).unwrap()
    }
    #[test]
    fn explicitly_declared_units_survive_unknown_model_and_zero_web_rates() {
        let raw = fixture();
        let data = HashMap::from([(
            DataField::Hashrate,
            json!({"all":raw["all"]["total_hash"],"summary":raw["SUMMARY"][0]}),
        )]);
        let rate = miner().parse_hashrate(&data).unwrap();
        assert_eq!(rate.value, 25000000.0);
        assert_eq!(rate.unit, HashRateUnit::Hash);
        assert_eq!(rate.algo, HashAlgorithm::Unknown);
        let stopped = json!({"all":{"Hash Rate H":0},"summary":{"MHS 1m":100}});
        assert!(!miner().parse_is_mining(&HashMap::from([(DataField::IsMining, stopped)])));
        assert!(!miner().parse_is_mining(&HashMap::new()));
    }
    #[test]
    fn missing_hardware_and_power_remain_unknown_while_reported_zero_is_kept() {
        let raw = fixture();
        let boards = miner().parse_hashboards(&HashMap::from([(
            DataField::Hashboards,
            json!({"all":raw["all"],"stats":raw["STATS"]}),
        )]));
        assert_eq!(boards.len(), 2);
        assert_eq!(boards[0].working_chips, Some(16));
        assert_eq!(boards[1].working_chips, Some(0));
        assert!(boards.iter().all(|b| b.expected_chips.is_none()));
        assert!(miner().get_expected_hashboards().is_none());
        assert_eq!(
            miner()
                .parse_wattage(&HashMap::from([(
                    DataField::Wattage,
                    json!({"all":0,"stats":[{"power":1500}]})
                )]))
                .unwrap()
                .as_watts(),
            0.0
        );
        assert!(miner().parse_wattage(&HashMap::new()).is_none());
        assert!(
            miner()
                .parse_fans(&HashMap::from([(
                    DataField::Fans,
                    json!([{"fansSpeed":50}])
                )]))
                .is_empty()
        );
    }
    #[test]
    fn reported_zero_retains_its_exact_source_without_claiming_measurement_method() {
        let miner = miner();
        let web_zero = HashMap::from([(
            DataField::Wattage,
            json!({"all":0,"stats":[{"power":1500}]}),
        )]);
        assert_eq!(miner.parse_wattage(&web_zero).unwrap().as_watts(), 0.0);
        assert_eq!(
            miner.parse_wattage_source(&web_zero).as_deref(),
            Some("innosilicon.web:/api/getAll#/all/power")
        );
        assert_eq!(miner.parse_wattage_is_estimated(&web_zero), None);
        assert_eq!(miner.parse_wattage_firmware_source(&web_zero), None);
        assert_eq!(miner.parse_wattage_indicator(&web_zero), None);
        let rpc_zero = HashMap::from([(
            DataField::Wattage,
            json!({"stats":[{"Elapsed":10},{"power":0},{"power":1500}]}),
        )]);
        assert_eq!(miner.parse_wattage(&rpc_zero).unwrap().as_watts(), 0.0);
        assert_eq!(
            miner.parse_wattage_source(&rpc_zero).as_deref(),
            Some("innosilicon.rpc:stats#/STATS/1/power")
        );
        assert_eq!(miner.parse_wattage_source(&HashMap::new()), None);
    }
    #[tokio::test]
    async fn all_control_methods_and_mutating_commands_fail_without_network() {
        let miner = miner();
        assert!(!miner.supports_pause());
        assert!(!miner.supports_restart());
        assert!(!miner.supports_resume());
        assert!(!miner.supports_pools_config());
        assert!(!miner.supports_change_password());
        assert!(miner.pause(None).await.is_err());
        assert!(miner.get_api_result(&rpc("restart")).await.is_err());
        assert!(miner.get_api_result(&web("poweroff")).await.is_err());
        assert!(
            miner
                .get_api_result(&MinerCommand::WebAPI {
                    command: "getAll",
                    parameters: Some(json!({"power":0}))
                })
                .await
                .is_err()
        );
    }
}
