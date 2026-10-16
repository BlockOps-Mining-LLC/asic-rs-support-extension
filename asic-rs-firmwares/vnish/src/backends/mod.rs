use std::{collections::BTreeSet, net::IpAddr, time::Duration};

use asic_rs_core::data::{
    board::ChipTemperatureMaxStatus, device::HashAlgorithm, hashrate::HashRateUnit,
};
use asic_rs_core::traits::{
    miner::{Miner, MinerConstructor, Validate, WebAPIClient},
    model::MinerModel,
};
use measurements::Temperature;
use reqwest::Method;
use serde_json::Value;
use tokio::time::sleep;
pub use v1_2_0::VnishV120;
pub use v1_3_0::VnishV130;

pub mod v1_2_0;
pub mod v1_3_0;

#[cfg(test)]
mod test;

pub struct Vnish;

fn is_public_read_endpoint(command: &str, method: &Method) -> bool {
    *method == Method::GET
        && matches!(
            command,
            "info"
                | "status"
                | "summary"
                | "metrics"
                | "chains"
                | "chains/factory-info"
                | "settings"
                | "autotune/presets"
        )
}

async fn confirm_stopped(web: &impl WebAPIClient) -> anyhow::Result<()> {
    let mut last_observation = String::from("no status read");
    for attempt in 0..5 {
        match web.send_command("status", false, None, Method::GET).await {
            Ok(status) => match status.pointer("/miner_state").and_then(Value::as_str) {
                Some("stopped") => return Ok(()),
                Some(state) => last_observation = format!("miner_state={state}"),
                None => last_observation = String::from("missing miner_state"),
            },
            Err(error) => last_observation = format!("status read failed: {error}"),
        }
        if attempt < 4 {
            sleep(Duration::from_secs(1)).await;
        }
    }

    anyhow::bail!(
        "VNish stop HTTP response succeeded but stopped state is unconfirmed ({last_observation})"
    )
}

async fn needs_start(web: &impl WebAPIClient) -> anyhow::Result<bool> {
    let status = web.send_command("status", false, None, Method::GET).await?;
    match status.pointer("/miner_state").and_then(Value::as_str) {
        Some("mining" | "auto-tuning" | "auto_tuning" | "tuning") => Ok(false),
        Some("stopped") => Ok(true),
        Some(state) => anyhow::bail!("VNish miner_state={state}; no start request sent"),
        None => anyhow::bail!("VNish status is missing miner_state; no start request sent"),
    }
}

fn board_positions(chains: &[Value], model_count: Option<u8>) -> Vec<u8> {
    let mut positions: BTreeSet<u8> = (0..model_count.unwrap_or(0)).collect();
    for chain in chains {
        let Some(position) = chain
            .get("id")
            .and_then(Value::as_u64)
            .and_then(|id| id.checked_sub(1))
            .and_then(|position| u8::try_from(position).ok())
        else {
            continue;
        };
        positions.insert(position);
    }
    positions.into_iter().collect()
}

fn working_chip_count(chips: &[Value]) -> Option<u16> {
    if chips.is_empty() {
        return None;
    }
    let mut working = 0_u16;
    for chip in chips {
        let rate = chip.get("hr").and_then(nonnegative_rate)?;
        if rate > 0.0 {
            working = working.checked_add(1)?;
        }
    }
    Some(working)
}

fn chip_status_count(chain: &Value, color: &str) -> Option<u16> {
    chain
        .get("chip_statuses")?
        .get(color)?
        .as_u64()
        .and_then(|count| u16::try_from(count).ok())
}

fn nonnegative_rate(value: &Value) -> Option<f64> {
    value
        .as_f64()
        .filter(|rate| rate.is_finite() && *rate >= 0.0)
}

fn board_sensor_temperature(value: &Value) -> Option<Temperature> {
    value
        .as_f64()
        .filter(|temperature| temperature.is_finite() && *temperature > 0.0)
        .map(Temperature::from_celsius)
}

fn reported_chip_max(value: Option<&Value>) -> (Option<Temperature>, ChipTemperatureMaxStatus) {
    match value {
        None | Some(Value::Null) => (None, ChipTemperatureMaxStatus::Missing),
        Some(value) => match value.as_f64() {
            Some(temperature) if temperature.is_finite() && temperature == 0.0 => (
                Some(Temperature::from_celsius(temperature)),
                ChipTemperatureMaxStatus::ReportedZero,
            ),
            Some(temperature) if temperature.is_finite() && temperature < 0.0 => (
                Some(Temperature::from_celsius(temperature)),
                ChipTemperatureMaxStatus::Invalid,
            ),
            Some(temperature) if temperature.is_finite() => (
                Some(Temperature::from_celsius(temperature)),
                ChipTemperatureMaxStatus::Reported,
            ),
            _ => (None, ChipTemperatureMaxStatus::Malformed),
        },
    }
}

fn rate_unit(label: Option<&Value>, algorithm: HashAlgorithm) -> Option<HashRateUnit> {
    match label {
        Some(Value::String(label)) if label == "N/A" => {
            (algorithm == HashAlgorithm::SHA256).then_some(HashRateUnit::GigaHash)
        }
        Some(Value::String(label)) => label.parse().ok(),
        Some(_) => None,
        None => (algorithm == HashAlgorithm::SHA256).then_some(HashRateUnit::GigaHash),
    }
}

fn parse_devfee_connected(pools: &Value) -> Option<bool> {
    let mut disconnected = false;

    for pool in pools.as_array()? {
        let is_devfee = pool
            .get("pool_type")
            .and_then(Value::as_str)
            .is_some_and(|pool_type| pool_type.eq_ignore_ascii_case("DevFee"));
        if !is_devfee {
            continue;
        }

        let Some(status) = pool.get("status").and_then(Value::as_str) else {
            continue;
        };
        if ["active", "working", "rejecting"]
            .iter()
            .any(|candidate| status.eq_ignore_ascii_case(candidate))
        {
            return Some(true);
        }
        if ["offline", "disabled"]
            .iter()
            .any(|candidate| status.eq_ignore_ascii_case(candidate))
        {
            disconnected = true;
        }
    }

    disconnected.then_some(false)
}

impl MinerConstructor for Vnish {
    #[allow(clippy::new_ret_no_self)]
    #[allow(clippy::if_same_then_else)]
    fn new(ip: IpAddr, model: impl MinerModel, version: Option<semver::Version>) -> Box<dyn Miner> {
        // Manual throttle (`tuning_percent`) was introduced in the 1.3.x line;
        // assumed cutoff 1.3.0 (1.3.4 verified live, 1.2.x API has no endpoint).
        if VnishV130::validate(version.as_ref()) {
            Box::new(VnishV130::new(ip, model))
        } else if VnishV120::validate(version.as_ref()) {
            Box::new(VnishV120::new(ip, model))
        } else {
            Box::new(VnishV120::new(ip, model))
        }
    }
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::{parse_devfee_connected, rate_unit};
    use asic_rs_core::data::{device::HashAlgorithm, hashrate::HashRateUnit};

    #[test]
    fn rate_unit_only_infers_gigahash_for_missing_or_na_sha256_units() {
        assert_eq!(
            rate_unit(None, HashAlgorithm::SHA256),
            Some(HashRateUnit::GigaHash)
        );
        assert_eq!(
            rate_unit(Some(&json!("N/A")), HashAlgorithm::SHA256),
            Some(HashRateUnit::GigaHash)
        );
        assert_eq!(rate_unit(Some(&json!("N/A")), HashAlgorithm::Scrypt), None);
        assert_eq!(
            rate_unit(Some(&json!("broken")), HashAlgorithm::SHA256),
            None
        );
        assert_eq!(rate_unit(Some(&json!(42)), HashAlgorithm::SHA256), None);
        assert_eq!(
            rate_unit(Some(&json!("MH/s")), HashAlgorithm::Scrypt),
            Some(HashRateUnit::MegaHash)
        );
    }

    #[test]
    fn devfee_connection_uses_typed_pool_status() {
        assert_eq!(
            parse_devfee_connected(&json!([
                { "pool_type": "UserPool", "status": "working" },
                { "pool_type": "DevFee", "status": "offline" }
            ])),
            Some(false)
        );
        assert_eq!(
            parse_devfee_connected(&json!([
                { "pool_type": "DevFee", "status": "offline" },
                { "pool_type": "DevFee", "status": "rejecting" }
            ])),
            Some(true)
        );
        assert_eq!(
            parse_devfee_connected(&json!([
                { "pool_type": "UserPool", "status": "offline" }
            ])),
            None
        );
        assert_eq!(
            parse_devfee_connected(&json!([
                { "pool_type": "DevFee", "status": "unknown" }
            ])),
            None
        );
    }
}
