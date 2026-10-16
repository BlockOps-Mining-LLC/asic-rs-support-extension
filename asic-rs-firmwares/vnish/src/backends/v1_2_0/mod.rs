use std::{collections::HashMap, net::IpAddr, str::FromStr, time::Duration};

use anyhow;
use asic_rs_core::{
    config::{
        collector::{ConfigCollector, ConfigExtractor, ConfigField, ConfigLocation},
        pools::PoolGroupConfig,
        preset::PresetInfo,
        scaling::ScalingConfig,
        temperature::TemperatureConfig,
        timezone::TimezoneConfig,
        tuning::TuningConfig,
    },
    data::{
        board::{BoardData, ChipData, MinerControlBoard},
        collector::{
            DataCollector, DataExtensions, DataExtractor, DataField, DataLocation, get_by_pointer,
        },
        command::MinerCommand,
        device::DeviceInfo,
        fan::FanData,
        firmware::RestoreStockOsResult,
        hashrate::HashRate,
        message::{MessageSeverity, MinerMessage},
        miner::TuningTarget,
        operating_state::OperatingState,
        pool::{PoolData, PoolGroupData, PoolURL},
    },
    traits::{miner::*, model::MinerModel},
};
use asic_rs_makes_antminer::hardware::AntMinerControlBoard;
use async_trait::async_trait;
use macaddr::MacAddr;
use measurements::{AngularVelocity, Frequency, Power, Temperature, Voltage};
use semver::Version;
use serde_json::{Value, json};
use web::VnishWebAPI;

use crate::{
    firmware::VnishFirmware,
    timezone::{tz_to_vnish_offset_now, vnish_offset_to_tz},
};

mod web;

#[derive(Debug)]
pub struct VnishV120 {
    ip: IpAddr,
    web: VnishWebAPI,
    device_info: DeviceInfo,
}

impl VnishV120 {
    pub fn new(ip: IpAddr, model: impl MinerModel) -> Self {
        let auth = Self::default_auth();
        let algo = model.hash_algorithm();
        VnishV120 {
            ip,
            web: VnishWebAPI::new(ip, 80, auth),
            device_info: DeviceInfo::new(model, VnishFirmware::default(), algo),
        }
    }
}

#[async_trait]
impl APIClient for VnishV120 {
    async fn get_api_result(&self, command: &MinerCommand) -> anyhow::Result<Value> {
        match command {
            MinerCommand::WebAPI { .. } => self.web.get_api_result(command).await,
            _ => Err(anyhow::anyhow!(
                "Unsupported command type for {} API",
                VnishFirmware::default()
            )),
        }
    }
}

impl GetConfigsLocations for VnishV120 {
    fn get_configs_locations(&self, data_field: ConfigField) -> Vec<ConfigLocation> {
        const WEB_SUMMARY: MinerCommand = MinerCommand::WebAPI {
            command: "summary",
            parameters: None,
        };
        const WEB_SETTINGS: MinerCommand = MinerCommand::WebAPI {
            command: "settings",
            parameters: None,
        };
        match data_field {
            ConfigField::Temperature => vec![(
                WEB_SUMMARY,
                ConfigExtractor {
                    func: get_by_pointer,
                    key: Some("/miner"),
                    tag: None,
                },
            )],
            ConfigField::Timezone => vec![(
                WEB_SETTINGS,
                ConfigExtractor {
                    func: get_by_pointer,
                    key: Some("/regional/timezone"),
                    tag: None,
                },
            )],
            _ => vec![],
        }
    }
}

impl CollectConfigs for VnishV120 {
    fn get_config_collector(&self) -> ConfigCollector<'_> {
        ConfigCollector::new(self)
    }
}

impl GetDataLocations for VnishV120 {
    fn get_locations(&self, data_field: DataField) -> Vec<DataLocation> {
        const WEB_INFO: MinerCommand = MinerCommand::WebAPI {
            command: "info",
            parameters: None,
        };
        const WEB_STATUS: MinerCommand = MinerCommand::WebAPI {
            command: "status",
            parameters: None,
        };
        const WEB_SUMMARY: MinerCommand = MinerCommand::WebAPI {
            command: "summary",
            parameters: None,
        };
        const WEB_CHAINS: MinerCommand = MinerCommand::WebAPI {
            command: "chains",
            parameters: None,
        };
        const WEB_FACTORY_INFO: MinerCommand = MinerCommand::WebAPI {
            command: "chains/factory-info",
            parameters: None,
        };
        const WEB_SETTINGS: MinerCommand = MinerCommand::WebAPI {
            command: "settings",
            parameters: None,
        };

        match data_field {
            DataField::Mac => vec![(
                WEB_INFO,
                DataExtractor {
                    func: get_by_pointer,
                    key: Some("/system/network_status/mac"),
                    tag: None,
                },
            )],
            DataField::SerialNumber => vec![
                (
                    WEB_FACTORY_INFO,
                    DataExtractor {
                        func: get_by_pointer,
                        key: Some("/psu_serial"),
                        tag: None,
                    },
                ),
                (
                    WEB_INFO,
                    DataExtractor {
                        func: get_by_pointer,
                        key: Some("/serial"),
                        tag: None,
                    },
                ),
            ],
            DataField::Hostname => vec![(
                WEB_INFO,
                DataExtractor {
                    func: get_by_pointer,
                    key: Some("/system/network_status/hostname"),
                    tag: None,
                },
            )],
            DataField::ApiVersion => vec![(
                WEB_INFO,
                DataExtractor {
                    func: get_by_pointer,
                    key: Some("/fw_version"),
                    tag: None,
                },
            )],
            DataField::FirmwareVersion => vec![(
                WEB_INFO,
                DataExtractor {
                    func: get_by_pointer,
                    key: Some("/fw_version"),
                    tag: None,
                },
            )],
            DataField::ControlBoardVersion => vec![(
                WEB_INFO,
                DataExtractor {
                    func: get_by_pointer,
                    key: Some("/platform"),
                    tag: None,
                },
            )],
            DataField::Uptime => vec![(
                WEB_INFO,
                DataExtractor {
                    func: get_by_pointer,
                    key: Some("/system/uptime"),
                    tag: None,
                },
            )],
            // `/info`'s `hr_measure` rides along with every hashrate figure so
            // the value can be scaled by the unit VNish actually reports —
            // GH/s on SHA-256 models, MH/s on Scrypt ones — rather than a
            // hardcoded assumption.
            DataField::Hashrate => vec![
                (
                    WEB_SUMMARY,
                    DataExtractor {
                        func: get_by_pointer,
                        key: Some("/miner/hr_realtime"),
                        tag: Some("hashrate"),
                    },
                ),
                (
                    WEB_INFO,
                    DataExtractor {
                        func: get_by_pointer,
                        key: Some("/hr_measure"),
                        tag: Some("unit"),
                    },
                ),
            ],
            DataField::ExpectedHashrate => vec![
                (
                    WEB_FACTORY_INFO,
                    DataExtractor {
                        func: get_by_pointer,
                        key: Some("/hr_stock"),
                        tag: Some("hashrate"),
                    },
                ),
                (
                    WEB_SUMMARY,
                    DataExtractor {
                        func: get_by_pointer,
                        key: Some("/miner/hr_stock"),
                        tag: Some("hashrate"),
                    },
                ),
                (
                    WEB_INFO,
                    DataExtractor {
                        func: get_by_pointer,
                        key: Some("/hr_measure"),
                        tag: Some("unit"),
                    },
                ),
            ],
            DataField::Wattage => vec![(
                WEB_SUMMARY,
                DataExtractor {
                    func: get_by_pointer,
                    key: Some("/miner/power_consumption"),
                    tag: None,
                },
            )],
            DataField::Fans => vec![(
                WEB_SUMMARY,
                DataExtractor {
                    func: get_by_pointer,
                    key: Some("/miner/cooling/fans"),
                    tag: None,
                },
            )],
            DataField::Hashboards => vec![
                (
                    WEB_SUMMARY,
                    DataExtractor {
                        func: get_by_pointer,
                        key: Some("/miner/chains"),
                        tag: Some("chains"),
                    },
                ),
                (
                    WEB_CHAINS,
                    DataExtractor {
                        func: get_by_pointer,
                        key: Some(""),
                        tag: Some("chains"),
                    },
                ),
                (
                    WEB_INFO,
                    DataExtractor {
                        func: get_by_pointer,
                        key: Some("/hr_measure"),
                        tag: Some("unit"),
                    },
                ),
            ],
            DataField::Chips => vec![(
                WEB_CHAINS,
                DataExtractor {
                    func: get_by_pointer,
                    key: Some(""),
                    tag: None,
                },
            )],
            DataField::Pools => vec![(
                WEB_SUMMARY,
                DataExtractor {
                    func: get_by_pointer,
                    key: Some("/miner/pools"),
                    tag: None,
                },
            )],
            DataField::DevFeeConnected => vec![(
                WEB_SUMMARY,
                DataExtractor {
                    func: get_by_pointer,
                    key: Some("/miner/pools"),
                    tag: None,
                },
            )],
            DataField::IsMining => vec![(
                WEB_STATUS,
                DataExtractor {
                    func: get_by_pointer,
                    key: Some("/miner_state"),
                    tag: None,
                },
            )],
            DataField::OperatingState => vec![(
                WEB_STATUS,
                DataExtractor {
                    func: get_by_pointer,
                    key: Some("/miner_state"),
                    tag: None,
                },
            )],
            DataField::LightFlashing => vec![(
                WEB_STATUS,
                DataExtractor {
                    func: get_by_pointer,
                    key: Some("/find_miner"),
                    tag: None,
                },
            )],
            DataField::FluidTemperature => vec![(
                WEB_SUMMARY,
                DataExtractor {
                    func: get_by_pointer,
                    key: Some("/miner/chains"),
                    tag: None,
                },
            )],
            DataField::OutletFluidTemperature => vec![(
                WEB_SUMMARY,
                DataExtractor {
                    func: get_by_pointer,
                    key: Some("/miner/chains"),
                    tag: None,
                },
            )],
            DataField::Messages => vec![(
                WEB_SUMMARY,
                DataExtractor {
                    func: get_by_pointer,
                    key: Some("/miner/miner_status"),
                    tag: None,
                },
            )],
            DataField::TuningTarget => vec![(
                WEB_SETTINGS,
                DataExtractor {
                    func: get_by_pointer,
                    key: Some("/miner/overclock/preset"),
                    tag: None,
                },
            )],
            _ => vec![],
        }
    }
}

impl GetIP for VnishV120 {
    fn get_ip(&self) -> IpAddr {
        self.ip
    }
}

impl GetDeviceInfo for VnishV120 {
    fn get_device_info(&self) -> DeviceInfo {
        self.device_info.clone()
    }
}

impl CollectData for VnishV120 {
    fn get_collector(&self) -> DataCollector<'_> {
        DataCollector::new(self)
    }
}

impl GetMAC for VnishV120 {
    fn parse_mac(&self, data: &HashMap<DataField, Value>) -> Option<MacAddr> {
        data.extract::<String>(DataField::Mac)
            .and_then(|s| MacAddr::from_str(&s).ok())
    }
}

impl GetSerialNumber for VnishV120 {
    fn parse_serial_number(&self, data: &HashMap<DataField, Value>) -> Option<String> {
        data.extract::<String>(DataField::SerialNumber)
    }
}

impl GetHostname for VnishV120 {
    fn parse_hostname(&self, data: &HashMap<DataField, Value>) -> Option<String> {
        data.extract::<String>(DataField::Hostname)
    }
}

impl GetApiVersion for VnishV120 {
    fn parse_api_version(&self, data: &HashMap<DataField, Value>) -> Option<String> {
        data.extract::<String>(DataField::ApiVersion)
    }
}

impl GetFirmwareVersion for VnishV120 {
    fn parse_firmware_version(&self, data: &HashMap<DataField, Value>) -> Option<String> {
        data.extract::<String>(DataField::FirmwareVersion)
    }
}

impl GetControlBoardVersion for VnishV120 {
    fn parse_control_board_version(
        &self,
        data: &HashMap<DataField, Value>,
    ) -> Option<MinerControlBoard> {
        data.extract::<String>(DataField::ControlBoardVersion)
            .and_then(|s| AntMinerControlBoard::parse(&s).map(|cb| cb.into()))
    }
}

impl GetHashboards for VnishV120 {
    fn parse_hashboards(&self, data: &HashMap<DataField, Value>) -> Vec<BoardData> {
        let hashboards_data = data.get(&DataField::Hashboards);
        let Some(all_chains) = hashboards_data
            .and_then(|v| v.pointer("/chains"))
            .and_then(|v| v.as_array())
        else {
            return Vec::new();
        };
        let unit = super::rate_unit(
            hashboards_data.and_then(|v| v.pointer("/unit")),
            self.device_info.algo,
        );
        let algo = self.device_info.algo;
        let chip_chains = data.get(&DataField::Chips).and_then(|v| v.as_array());

        let mut hashboards: Vec<BoardData> =
            super::board_positions(all_chains, self.device_info.hardware.board_count())
                .into_iter()
                .map(|position| {
                    BoardData::new(
                        position,
                        self.device_info.hardware.chips_for_board(position as usize),
                    )
                })
                .collect();

        // Both /summary and /chains endpoints are concatenated into all_chains.
        // Vnish chain IDs are 1-based; map to 0-based position by adding 1 when matching.
        for board in hashboards.iter_mut() {
            let id = board.position as u64 + 1;
            let mut merged = serde_json::Map::new();
            for entry in all_chains
                .iter()
                .filter(|c| c.pointer("/id").and_then(|v| v.as_u64()) == Some(id))
            {
                if let Value::Object(obj) = entry {
                    for (k, v) in obj {
                        merged.entry(k.clone()).or_insert_with(|| v.clone());
                    }
                }
            }

            if merged.is_empty() {
                continue;
            }

            let chain = Value::Object(merged);
            board.hashrate = ["/hashrate_rt", "/hr_realtime"]
                .iter()
                .find_map(|&p| chain.pointer(p).and_then(super::nonnegative_rate))
                .and_then(|f| {
                    unit.map(|unit| {
                        HashRate {
                            value: f,
                            unit,
                            algo,
                        }
                        .as_default_unit()
                    })
                });
            board.expected_hashrate = ["/hashrate_ideal", "/hr_nominal"]
                .iter()
                .find_map(|&p| chain.pointer(p).and_then(super::nonnegative_rate))
                .and_then(|f| {
                    unit.map(|unit| {
                        HashRate {
                            value: f,
                            unit,
                            algo,
                        }
                        .as_default_unit()
                    })
                });
            board.board_temperature = chain
                .pointer("/pcb_temp/max")
                .and_then(super::board_sensor_temperature);
            // Per-board chip temperatures: coolest chip -> inlet, hottest -> outlet.
            // Coolant (water) temperatures are reported separately as
            // fluid / outlet_fluid temperatures on the miner, not here.
            board.inlet_chip_temperature = chain
                .pointer("/chip_temp/min")
                .and_then(super::board_sensor_temperature);
            board.outlet_chip_temperature = chain
                .pointer("/chip_temp/max")
                .and_then(super::board_sensor_temperature);
            let (chip_max, chip_max_status) =
                super::reported_chip_max(chain.pointer("/chip_temp/max"));
            board.reported_chip_max_temperature = chip_max;
            board.reported_chip_max_status = Some(chip_max_status);
            board.reported_chip_max_source = Some("vnish:chains[].chip_temp.max".to_owned());
            board.working_chips = chain
                .pointer("/chips")
                .and_then(|v| v.as_array())
                .and_then(|chips| super::working_chip_count(chips));
            board.serial_number = chain
                .pointer("/serial")
                .and_then(|v| v.as_str())
                .map(String::from)
                .or_else(|| data.extract::<String>(DataField::SerialNumber));
            if let Some(chip_chain) = chip_chains.and_then(|chains| {
                chains
                    .iter()
                    .find(|c| c.pointer("/id").and_then(|v| v.as_u64()) == Some(id))
            }) {
                board.chips = chip_chain
                    .pointer("/chips")
                    .and_then(|v| v.as_array())
                    .map(|chips_arr| {
                        chips_arr
                            .iter()
                            .enumerate()
                            .map(|(idx, chip)| {
                                let reported_rate =
                                    chip.pointer("/hr").and_then(super::nonnegative_rate);
                                let hashrate = reported_rate.and_then(|f| {
                                    unit.map(|unit| {
                                        {
                                            HashRate {
                                                value: f,
                                                unit,
                                                algo,
                                            }
                                        }
                                        .as_default_unit()
                                    })
                                });
                                let working = reported_rate.map(|rate| rate > 0.0);
                                ChipData {
                                    position: chip
                                        .pointer("/id")
                                        .and_then(|v| v.as_u64())
                                        .unwrap_or(idx as u64)
                                        as u16,
                                    hashrate,
                                    temperature: chip
                                        .pointer("/temp")
                                        .and_then(super::board_sensor_temperature),
                                    voltage: chip
                                        .pointer("/volt")
                                        .and_then(|v| v.as_i64())
                                        .map(|v| Voltage::from_millivolts(v as f64)),
                                    frequency: chip
                                        .pointer("/freq")
                                        .and_then(|v| v.as_i64())
                                        .map(|f| Frequency::from_megahertz(f as f64)),
                                    tuned: None,
                                    working,
                                }
                            })
                            .collect()
                    })
                    .unwrap_or_default();
            }
            board.voltage = chain
                .pointer("/voltage")
                .and_then(|v| v.as_i64())
                .map(|v| Voltage::from_millivolts(v as f64));
            board.frequency = chain
                .pointer("/frequency")
                .or_else(|| chain.pointer("/freq"))
                .and_then(|v| v.as_f64())
                .map(Frequency::from_megahertz);
            board.tuned =
                data.extract::<String>(DataField::IsMining)
                    .and_then(|s| match s.as_str() {
                        "auto-tuning" => Some(false),
                        "mining" => Some(true),
                        _ => None,
                    });
            board.active = chain
                .pointer("/status/state")
                .and_then(|v| v.as_str())
                .map(|s| s == "mining")
                .or_else(|| board.hashrate.as_ref().map(|h| h.value > 0.0));
            board.state = chain
                .pointer("/status/state")
                .or_else(|| chain.pointer("/status"))
                .and_then(Value::as_str)
                .filter(|state| !state.trim().is_empty())
                .map(str::to_owned);
            board.status_description = chain
                .pointer("/status/description")
                .or_else(|| chain.pointer("/description"))
                .and_then(Value::as_str)
                .filter(|description| !description.trim().is_empty())
                .map(str::to_owned);
            board.red_chips = super::chip_status_count(&chain, "red");
            board.orange_chips = super::chip_status_count(&chain, "orange");
            board.grey_chips = super::chip_status_count(&chain, "grey");
        }

        hashboards
    }
}

impl GetHashrate for VnishV120 {
    fn parse_hashrate(&self, data: &HashMap<DataField, Value>) -> Option<HashRate> {
        let field = data.get(&DataField::Hashrate);
        let value = super::nonnegative_rate(field?.pointer("/hashrate")?)?;
        let unit = super::rate_unit(
            field.and_then(|v| v.pointer("/unit")),
            self.device_info.algo,
        )?;
        Some(
            HashRate {
                value,
                // `/info`'s `hr_measure`: `"GH/s"`, `"MH/s"` or `"N/A"`. GH/s
                // when it cannot say, which is what SHA-256 models report.
                unit,
                algo: self.device_info.algo,
            }
            .as_default_unit(),
        )
    }
}

impl GetExpectedHashrate for VnishV120 {
    fn parse_expected_hashrate(&self, data: &HashMap<DataField, Value>) -> Option<HashRate> {
        let field = data.get(&DataField::ExpectedHashrate);
        let value = super::nonnegative_rate(field?.pointer("/hashrate")?)?;
        let unit = super::rate_unit(
            field.and_then(|v| v.pointer("/unit")),
            self.device_info.algo,
        )?;
        Some(
            HashRate {
                value,
                // `/info`'s `hr_measure`: `"GH/s"`, `"MH/s"` or `"N/A"`. GH/s
                // when it cannot say, which is what SHA-256 models report.
                unit,
                algo: self.device_info.algo,
            }
            .as_default_unit(),
        )
    }
}

impl GetFans for VnishV120 {
    fn parse_fans(&self, data: &HashMap<DataField, Value>) -> Vec<FanData> {
        let mut fans: Vec<FanData> = Vec::new();

        if let Some(fans_data) = data.get(&DataField::Fans)
            && let Some(fans_array) = fans_data.as_array()
        {
            for (idx, fan) in fans_array.iter().enumerate() {
                if let (Ok(position), Some(rpm)) = (
                    i16::try_from(idx),
                    fan.pointer("/rpm")
                        .and_then(Value::as_u64)
                        .and_then(|rpm| u16::try_from(rpm).ok()),
                ) {
                    fans.push(FanData {
                        position,
                        rpm: Some(AngularVelocity::from_rpm(rpm as f64)),
                    });
                }
            }
        }

        fans
    }
}

impl GetPsuFans for VnishV120 {}
#[async_trait]
impl SupportsTimezoneConfig for VnishV120 {
    fn supports_timezone_config(&self) -> bool {
        true
    }

    /// VNish stores a fixed UTC offset (e.g. `"GMT+2"`) — it does not track DST,
    /// and it has no list endpoint, so we fall back to the whole-hour offsets.
    /// Both are reported as the equivalent IANA `Etc/GMT*` zones.
    fn parse_timezone_config(
        &self,
        data: &HashMap<ConfigField, Value>,
    ) -> anyhow::Result<TimezoneConfig> {
        const DEFAULT_OFFSETS: &[&str] = &[
            "GMT-12", "GMT-11", "GMT-10", "GMT-9", "GMT-8", "GMT-7", "GMT-6", "GMT-5", "GMT-4",
            "GMT-3", "GMT-2", "GMT-1", "GMT+0", "GMT+1", "GMT+2", "GMT+3", "GMT+4", "GMT+5",
            "GMT+6", "GMT+7", "GMT+8", "GMT+9", "GMT+10", "GMT+11", "GMT+12", "GMT+13", "GMT+14",
        ];
        let obj = data
            .get(&ConfigField::Timezone)
            .ok_or_else(|| anyhow::anyhow!("No timezone data returned"))?;
        // `"GMT+2"` (UTC+2) becomes the canonical `Etc/GMT-2`: the sign flips.
        let timezone = obj
            .pointer("/current")
            .and_then(|v| v.as_str())
            .map(vnish_offset_to_tz)
            .transpose()?;
        let available = DEFAULT_OFFSETS
            .iter()
            .map(|offset| vnish_offset_to_tz(offset))
            .collect::<anyhow::Result<Vec<_>>>()?;
        Ok(TimezoneConfig {
            timezone,
            available,
        })
    }

    /// Set the fixed UTC offset (read-modify-write the settings object).
    async fn set_timezone_config(&self, config: TimezoneConfig) -> anyhow::Result<bool> {
        let timezone = config
            .timezone
            .ok_or_else(|| anyhow::anyhow!("Timezone config has no timezone to set"))?;
        // Only the fixed-offset `Etc/GMT*` zones survive the trip; a DST zone is
        // rejected with a hint at its current equivalent (see `tz_to_vnish_offset`).
        let timezone = tz_to_vnish_offset_now(timezone)?;
        let mut settings = self.web.settings().await?;
        match settings.pointer_mut("/regional/timezone") {
            Some(tz) => {
                tz["current"] = json!(timezone);
            }
            None => anyhow::bail!(
                "{} settings has no /regional/timezone",
                VnishFirmware::default()
            ),
        }
        self.web.set_settings(settings).await.map(|_| true)
    }
}

impl GetFluidTemperature for VnishV120 {
    fn parse_fluid_temperature(&self, data: &HashMap<DataField, Value>) -> Option<Temperature> {
        // Fluid temperature mirrors other firmwares' "environment temperature":
        // the coolant entering the machine. For hydro miners that's the
        // per-board inlet water temperature.
        let chains = data
            .get(&DataField::FluidTemperature)
            .and_then(|v| v.as_array())?;
        chains
            .iter()
            .filter_map(|c| c.pointer("/inlet_water_temp").and_then(|v| v.as_i64()))
            .max()
            .map(|t| Temperature::from_celsius(t as f64))
    }
    fn parse_outlet_fluid_temperature(
        &self,
        data: &HashMap<DataField, Value>,
    ) -> Option<Temperature> {
        // Coolant exhaust: the per-board outlet water temperature on hydro
        // miners. Air-cooled models do not report it, yielding None.
        let chains = data
            .get(&DataField::OutletFluidTemperature)
            .and_then(|v| v.as_array())?;
        chains
            .iter()
            .filter_map(|c| c.pointer("/outlet_water_temp").and_then(|v| v.as_i64()))
            .max()
            .map(|t| Temperature::from_celsius(t as f64))
    }
}

impl GetWattage for VnishV120 {
    fn parse_wattage(&self, data: &HashMap<DataField, Value>) -> Option<Power> {
        data.extract::<i64>(DataField::Wattage)
            .filter(|watts| *watts >= 0)
            .map(|watts| Power::from_watts(watts as f64))
    }
}

// VNish 1.2.x has no manual throttle endpoint; tuning_percent lands in v1_3_0.
impl GetTuningPercent for VnishV120 {}

impl GetTuningTarget for VnishV120 {
    /// On VNish the active tuning target is the selected autotune preset
    /// (`miner.overclock.preset` in `/settings`).
    fn parse_tuning_target(&self, data: &HashMap<DataField, Value>) -> Option<TuningTarget> {
        let preset = data.get(&DataField::TuningTarget)?;
        preset
            .as_str()
            .map(String::from)
            .or_else(|| preset.as_i64().map(|n| n.to_string()))
            .map(TuningTarget::Preset)
    }
}

impl GetScaledTuningTarget for VnishV120 {}
impl GetTuningCapabilities for VnishV120 {}
impl GetLightFlashing for VnishV120 {
    fn parse_light_flashing(&self, data: &HashMap<DataField, Value>) -> Option<bool> {
        data.extract::<bool>(DataField::LightFlashing)
    }
}

impl GetMessages for VnishV120 {
    /// Surface the miner's own state verdict as a message. VNish self-manages
    /// (won't start mining below the configured `min_startup_water_temp`, and
    /// protects itself above `restart_temp`), so any non-operating state is
    /// reported here rather than computed from invented thresholds.
    fn parse_messages(&self, data: &HashMap<DataField, Value>) -> Vec<MinerMessage> {
        let mut messages = Vec::new();

        if let Some(status) = data.get(&DataField::Messages)
            && let Some(state) = status.pointer("/miner_state").and_then(|v| v.as_str())
        {
            let normal = matches!(
                state.to_lowercase().as_str(),
                "mining" | "auto-tuning" | "auto_tuning" | "tuning"
            );
            if !normal {
                let severity = match state.to_lowercase().as_str() {
                    "failure" | "failed" | "error" | "broken" | "stopped" => MessageSeverity::Error,
                    _ => MessageSeverity::Warning,
                };
                messages.push(MinerMessage::new(
                    0,
                    0,
                    format!("Miner state: {state}"),
                    severity,
                ));
            }
        }

        messages
    }
}

impl GetUptime for VnishV120 {
    fn parse_uptime(&self, data: &HashMap<DataField, Value>) -> Option<Duration> {
        data.extract::<String>(DataField::Uptime)
            .and_then(|uptime_str| {
                // Parse uptime strings like "10 days, 18:00"
                let trimmed = uptime_str.trim();

                // Try to parse format like "X days, HH:MM" or "X days"
                if trimmed.contains("days") {
                    let mut total_seconds = 0u64;

                    // Extract days
                    if let Some(days_part) = trimmed.split("days").next()
                        && let Ok(days) = days_part.trim().parse::<u64>()
                    {
                        total_seconds += days * 24 * 60 * 60;
                    }

                    // Extract hours and minutes if present (after comma)
                    if let Some(time_part) = trimmed.split(',').nth(1) {
                        let time_part = time_part.trim();
                        if let Some((hours_str, minutes_str)) = time_part.split_once(':')
                            && let (Ok(hours), Ok(minutes)) = (
                                hours_str.trim().parse::<u64>(),
                                minutes_str.trim().parse::<u64>(),
                            )
                        {
                            total_seconds += hours * 60 * 60 + minutes * 60;
                        }
                    }

                    return Some(Duration::from_secs(total_seconds));
                }

                // Handle "H:MM" or "HH:MM" format (uptime < 1 day)
                if let Some((hours_str, minutes_str)) = trimmed.split_once(':')
                    && let (Ok(hours), Ok(minutes)) = (
                        hours_str.trim().parse::<u64>(),
                        minutes_str.trim().parse::<u64>(),
                    )
                {
                    return Some(Duration::from_secs(hours * 3600 + minutes * 60));
                }

                None
            })
    }
}

impl GetBestShare for VnishV120 {}
impl GetSessionBestShare for VnishV120 {}

impl GetOperatingState for VnishV120 {
    fn parse_operating_state(&self, data: &HashMap<DataField, Value>) -> Option<OperatingState> {
        let label = data.get(&DataField::OperatingState)?.as_str()?;
        match label {
            "auto-tuning" | "auto_tuning" => Some(OperatingState::Tuning {}),
            "failure" | "failed" | "broken" => Some(OperatingState::Error {}),
            "idle" => Some(OperatingState::Idling {}),
            _ => OperatingState::from_label(label),
        }
    }
}

impl GetDevFeeConnected for VnishV120 {
    fn parse_devfee_connected(&self, data: &HashMap<DataField, Value>) -> Option<bool> {
        super::parse_devfee_connected(data.get(&DataField::DevFeeConnected)?)
    }
}

impl GetIsMining for VnishV120 {
    fn parse_is_mining(&self, data: &HashMap<DataField, Value>) -> bool {
        data.extract::<String>(DataField::IsMining)
            .map(|state| state == "mining" || state == "auto-tuning")
            .unwrap_or(true)
    }
}

impl GetPools for VnishV120 {
    fn parse_pools(&self, data: &HashMap<DataField, Value>) -> Vec<PoolGroupData> {
        let mut pools: Vec<PoolData> = Vec::new();

        if let Some(pools_data) = data.get(&DataField::Pools)
            && let Some(pools_array) = pools_data.as_array()
        {
            for (idx, pool) in pools_array.iter().enumerate() {
                let url = pool
                    .pointer("/url")
                    .and_then(|v| v.as_str())
                    .map(String::from)
                    .map(PoolURL::from);

                let user = pool
                    .pointer("/user")
                    .and_then(|v| v.as_str())
                    .map(String::from);

                let accepted_shares = pool.pointer("/accepted").and_then(|v| v.as_u64());
                let rejected_shares = pool.pointer("/rejected").and_then(|v| v.as_u64());
                let pool_status = pool.pointer("/status").and_then(|v| v.as_str());
                let (active, alive) = Self::parse_pool_status(pool_status);

                pools.push(PoolData {
                    position: Some(idx as u16),
                    url,
                    accepted_shares,
                    rejected_shares,
                    last_share_time: pool
                        .pointer("/ls_time")
                        .and_then(asic_rs_core::util::parse_last_share_time),
                    active,
                    alive,
                    user,
                });
            }
        }

        vec![PoolGroupData {
            name: String::new(),
            quota: 1,
            pools,
        }]
    }
}

impl VnishV120 {
    fn parse_pool_status(status: Option<&str>) -> (Option<bool>, Option<bool>) {
        match status {
            Some("active" | "working") => (Some(true), Some(true)),
            Some("offline" | "disabled") => (Some(false), Some(false)),
            Some("rejecting") => (Some(false), Some(true)),
            _ => (None, None),
        }
    }
}

#[async_trait]
impl SetFaultLight for VnishV120 {
    fn supports_set_fault_light(&self) -> bool {
        true
    }

    async fn set_fault_light(&self, fault: bool) -> anyhow::Result<bool> {
        if self.get_light_flashing().await == Some(fault) {
            return Ok(true);
        }

        let response = self.web.find_miner(fault).await?;
        Ok(response
            .pointer("/on")
            .and_then(|v| v.as_bool())
            .map(|on| on == fault)
            .unwrap_or(true))
    }
}

#[async_trait]
impl SetPowerLimit for VnishV120 {
    fn supports_set_power_limit(&self) -> bool {
        true
    }

    /// VNish sets power by selecting a tuned autotune preset, not an arbitrary
    /// wattage (mirrors pyasic's behaviour). Pick the highest tuned preset whose
    /// power is <= the requested limit, set `overclock.preset` to it and verify.
    async fn set_power_limit(&self, limit: Power) -> anyhow::Result<bool> {
        let target_watts = limit.as_watts() as i64;

        let presets = self.web.autotune_presets().await?;
        let preset_list = presets
            .as_array()
            .or_else(|| presets.get("presets").and_then(|p| p.as_array()))
            .ok_or_else(|| anyhow::anyhow!("unexpected /autotune/presets response"))?;

        // Each preset: { "name": "3495", "pretty": "3495 watt ~ 132 TH",
        //               "status": "tuned", ... }.
        // Consider ALL presets (tuned and un-tuned): an un-tuned preset is a
        // valid target, it just triggers a tuning cycle (effectively a restart)
        // before it settles — same behaviour as e.g. Whatsminer rebooting on a
        // power-limit change. Watts come from the preset `name` (the bare number,
        // present for tuned and un-tuned alike); non-numeric names like
        // "disabled" are skipped by the parse.
        let best = preset_list
            .iter()
            .filter_map(|p| {
                let name = p.get("name")?.as_str()?;
                name.trim().parse::<i64>().ok()
            })
            .filter(|&w| w <= target_watts)
            .max();

        let Some(preset_watts) = best else {
            return Ok(false);
        };

        let mut settings = self.web.settings().await?;
        {
            let overclock = settings
                .pointer_mut("/miner/overclock")
                .ok_or_else(|| anyhow::anyhow!("settings missing miner.overclock"))?;
            overclock["preset"] = json!(preset_watts.to_string());
        }
        let overclock = settings
            .pointer("/miner/overclock")
            .cloned()
            .ok_or_else(|| anyhow::anyhow!("settings missing miner.overclock"))?;

        self.web
            .set_settings(json!({ "miner": { "overclock": overclock } }))
            .await?;

        // Verify the preset was accepted.
        let updated = self.web.settings().await?;
        let applied = updated.pointer("/miner/overclock/preset").and_then(|p| {
            p.as_i64()
                .or_else(|| p.as_str().and_then(|s| s.parse().ok()))
        });
        Ok(applied == Some(preset_watts))
    }
}

// VNish 1.2.x has no manual throttle endpoint; tuning_percent lands in v1_3_0.
#[async_trait]
impl SetHashboardsEnabled for VnishV120 {}

impl SetTuningPercent for VnishV120 {}

#[async_trait]
impl SupportsPresets for VnishV120 {
    fn supports_presets(&self) -> bool {
        true
    }

    /// VNish exposes its autotune presets at `/autotune/presets`, each
    /// `{ "name": "5560", "pretty": "5560 watt ~ 175 TH", "status": "tuned" }`.
    async fn get_presets(&self) -> Vec<PresetInfo> {
        let Ok(presets) = self.web.autotune_presets().await else {
            return Vec::new();
        };
        let list = presets
            .as_array()
            .cloned()
            .or_else(|| presets.get("presets").and_then(|p| p.as_array()).cloned())
            .unwrap_or_default();
        list.iter()
            .filter_map(|p| {
                let name = p.get("name")?.as_str()?.to_string();
                Some(PresetInfo {
                    name,
                    pretty: p.get("pretty").and_then(|v| v.as_str()).map(String::from),
                    status: p.get("status").and_then(|v| v.as_str()).map(String::from),
                })
            })
            .collect()
    }
}

#[async_trait]
impl SupportsPoolsConfig for VnishV120 {
    async fn get_pools_config(&self) -> anyhow::Result<Vec<PoolGroupConfig>> {
        Ok(self
            .get_pools()
            .await
            .iter()
            .map(|g| g.clone().into())
            .collect())
    }

    async fn set_pools_config(&self, config: Vec<PoolGroupConfig>) -> anyhow::Result<bool> {
        let pools: Vec<Value> = config
            .iter()
            .flat_map(|group| group.pools.iter())
            .enumerate()
            .map(|(idx, pool)| {
                json!({
                    "url": format!("{}:{}", pool.url.host, pool.url.port),
                    "user": pool.username.as_str(),
                    "pass": pool.password.as_str(),
                    "order": idx,
                    "id": idx,
                })
            })
            .collect();

        Ok(self
            .web
            .set_settings(json!({ "miner": { "pools": pools } }))
            .await
            .is_ok())
    }

    fn supports_pools_config(&self) -> bool {
        true
    }
}

#[async_trait]
impl Restart for VnishV120 {
    async fn restart(&self) -> anyhow::Result<bool> {
        Ok(self.web.restart().await.is_ok())
    }

    fn supports_restart(&self) -> bool {
        true
    }
}

#[async_trait]
impl Pause for VnishV120 {
    #[allow(unused_variables)]
    async fn pause(&self, at_time: Option<Duration>) -> anyhow::Result<bool> {
        self.web.stop().await?;
        super::confirm_stopped(&self.web).await?;
        Ok(true)
    }

    fn supports_pause(&self) -> bool {
        true
    }
}

#[async_trait]
impl Resume for VnishV120 {
    #[allow(unused_variables)]
    async fn resume(&self, at_time: Option<Duration>) -> anyhow::Result<bool> {
        if super::needs_start(&self.web).await? {
            self.web.start().await?;
        }
        Ok(true)
    }

    fn supports_resume(&self) -> bool {
        true
    }
}

#[async_trait]
impl ChangePassword for VnishV120 {
    async fn change_password(&mut self, password: &str) -> anyhow::Result<bool> {
        let success = self.web.change_password(password).await?;
        if success {
            let username = self.web.username().to_string();
            self.set_auth(MinerAuth::new(username, password));
        }
        Ok(success)
    }

    fn supports_change_password(&self) -> bool {
        true
    }
}

#[async_trait]
impl ReadLogs for VnishV120 {
    async fn read_logs(&self) -> anyhow::Result<String> {
        self.web.read_logs().await
    }

    fn supports_read_logs(&self) -> bool {
        true
    }
}

#[async_trait]
impl FactoryReset for VnishV120 {
    async fn factory_reset(&self) -> anyhow::Result<bool> {
        self.web.factory_reset().await
    }

    fn supports_factory_reset(&self) -> bool {
        true
    }
}

#[async_trait]
impl RestoreStockOs for VnishV120 {
    async fn restore_stock_os(&self) -> anyhow::Result<RestoreStockOsResult> {
        let reboot_after_seconds = self.web.restore_stock_os().await?;
        Ok(RestoreStockOsResult::accepted(Some(reboot_after_seconds)))
    }

    fn supports_restore_stock_os(&self) -> bool {
        true
    }
}

#[async_trait]
impl SupportsScalingConfig for VnishV120 {
    fn supports_scaling_config(&self) -> bool {
        false
    }
}

#[async_trait]
impl SupportsTemperatureConfig for VnishV120 {
    fn supports_temperature_config(&self) -> bool {
        true
    }

    /// VNish reports configured thermal limits in `/summary`: the minimum
    /// startup water temperature and the self-protection restart temperature.
    /// `hot` is not exposed via `/summary` (left `None` = not reported, not
    /// "no limit").
    fn parse_temperature_config(
        &self,
        data: &HashMap<ConfigField, Value>,
    ) -> anyhow::Result<TemperatureConfig> {
        let miner = data
            .get(&ConfigField::Temperature)
            .ok_or_else(|| anyhow::anyhow!("No temperature config returned by miner"))?;
        Ok(TemperatureConfig {
            hot: None,
            danger: miner.pointer("/misc/restart_temp").and_then(|v| v.as_f64()),
            minimum: miner
                .pointer("/cooling/min_startup_water_temp")
                .and_then(|v| v.as_f64()),
        })
    }
}

#[async_trait]
impl UpgradeFirmware for VnishV120 {
    fn supports_upgrade_firmware(&self) -> bool {
        false
    }
}

impl HasDefaultAuth for VnishV120 {
    fn default_auth() -> MinerAuth {
        MinerAuth::new("", "admin")
    }
}

impl HasAuth for VnishV120 {
    fn set_auth(&mut self, auth: MinerAuth) {
        self.web.set_auth(auth);
    }
}

impl Validate for VnishV120 {
    type Firmware = VnishFirmware;

    fn validate(version: Option<&semver::Version>) -> bool {
        version.is_some_and(|v| *v < Version::new(1, 3, 0))
    }
}

#[async_trait]
impl SupportsTuningConfig for VnishV120 {
    fn supports_tuning_config(&self) -> bool {
        true
    }

    /// VNish tunes by selecting a named autotune preset. A `Preset` target sets
    /// `miner.overclock.preset` directly; a `Power` target reuses the
    /// preset-picking logic in `set_power_limit`.
    async fn set_tuning_config(
        &self,
        config: TuningConfig,
        _scaling_config: Option<ScalingConfig>,
    ) -> anyhow::Result<bool> {
        match config.target {
            TuningTarget::Manual { .. } => {
                anyhow::bail!(
                    "Manual tuning target is not supported on {}",
                    VnishFirmware::default()
                )
            }
            TuningTarget::Preset(name) => {
                let mut settings = self.web.settings().await?;
                {
                    let overclock = settings
                        .pointer_mut("/miner/overclock")
                        .ok_or_else(|| anyhow::anyhow!("settings missing miner.overclock"))?;
                    overclock["preset"] = json!(name);
                }
                let overclock = settings
                    .pointer("/miner/overclock")
                    .cloned()
                    .ok_or_else(|| anyhow::anyhow!("settings missing miner.overclock"))?;
                self.web
                    .set_settings(json!({ "miner": { "overclock": overclock } }))
                    .await?;
                let updated = self.web.settings().await?;
                let applied = updated.pointer("/miner/overclock/preset").and_then(|p| {
                    p.as_str()
                        .map(String::from)
                        .or_else(|| p.as_i64().map(|n| n.to_string()))
                });
                Ok(applied.as_deref() == Some(name.as_str()))
            }
            TuningTarget::Power(limit) => self.set_power_limit(limit).await,
            TuningTarget::HashRate(_) => {
                anyhow::bail!(
                    "HashRate tuning target is not supported on {}",
                    VnishFirmware::default()
                )
            }
            TuningTarget::MiningMode(_) => {
                anyhow::bail!(
                    "MiningMode tuning target is not supported on {}",
                    VnishFirmware::default()
                )
            }
        }
    }
}

#[async_trait]
impl SupportsFanConfig for VnishV120 {
    fn supports_fan_config(&self) -> bool {
        false
    }
}

#[cfg(test)]
mod tests {
    use asic_rs_core::data::board::ChipTemperatureMaxStatus;
    use asic_rs_core::data::device::HashAlgorithm;
    use asic_rs_core::data::hashrate::HashRateUnit;
    use asic_rs_core::test::api::MockAPIClient;
    use asic_rs_makes_antminer::models::AntMinerModel;

    use super::*;
    use crate::backends::test::{MockHttpServer, mock_http_sequence, mock_http_server};

    fn miner_with_mock_web(port: u16) -> VnishV120 {
        let ip = IpAddr::from([127, 0, 0, 1]);
        let mut miner = VnishV120::new(ip, AntMinerModel::S19);
        miner.web = VnishWebAPI::new(ip, port, MinerAuth::from_token("test-token"));
        miner
    }

    #[tokio::test]
    async fn resume_skips_duplicate_start_when_tuning() -> anyhow::Result<()> {
        let server =
            mock_http_sequence(vec![(200, "OK", r#"{"miner_state":"auto-tuning"}"#)]).await?;
        let miner = miner_with_mock_web(server.port);

        assert!(miner.resume(None).await?);
        let requests = server.task.await??;
        assert_eq!(requests.len(), 1);
        assert!(requests[0].starts_with("GET /api/v1/status HTTP/1.1"));
        Ok(())
    }

    #[tokio::test]
    async fn resume_skips_plain_tuning_and_rejects_unknown_state() -> anyhow::Result<()> {
        let server = mock_http_sequence(vec![(200, "OK", r#"{"miner_state":"tuning"}"#)]).await?;
        assert!(miner_with_mock_web(server.port).resume(None).await?);
        assert_eq!(server.task.await??.len(), 1);

        let server = mock_http_sequence(vec![(200, "OK", r#"{"miner_state":"starting"}"#)]).await?;
        let error = miner_with_mock_web(server.port)
            .resume(None)
            .await
            .unwrap_err();
        assert!(error.to_string().contains("miner_state=starting"));
        assert_eq!(server.task.await??.len(), 1);
        Ok(())
    }

    #[tokio::test]
    async fn resume_starts_stopped_miner_once() -> anyhow::Result<()> {
        let server = mock_http_sequence(vec![
            (200, "OK", r#"{"miner_state":"stopped"}"#),
            (200, "OK", "{}"),
        ])
        .await?;
        let miner = miner_with_mock_web(server.port);

        assert!(miner.resume(None).await?);
        let requests = server.task.await??;
        assert!(requests[0].starts_with("GET /api/v1/status HTTP/1.1"));
        assert!(requests[1].starts_with("POST /api/v1/mining/start HTTP/1.1"));
        assert_eq!(requests.len(), 2);
        Ok(())
    }

    #[tokio::test]
    async fn pause_confirms_stopped_after_empty_acknowledgement() -> anyhow::Result<()> {
        let server = mock_http_sequence(vec![
            (200, "OK", ""),
            (200, "OK", r#"{"miner_state":"stopped"}"#),
        ])
        .await?;
        let miner = miner_with_mock_web(server.port);

        assert!(miner.pause(None).await?);
        let requests = server.task.await??;
        assert!(requests[0].starts_with("POST /api/v1/mining/stop HTTP/1.1"));
        assert!(requests[1].starts_with("GET /api/v1/status HTTP/1.1"));
        Ok(())
    }

    #[test]
    fn unknown_model_keeps_reported_board_rows() {
        let miner = VnishV120::new(
            IpAddr::from([127, 0, 0, 1]),
            AntMinerModel::Unknown("unmapped model".to_string()),
        );
        let data = HashMap::from([
            (
                DataField::Hashboards,
                json!({"unit": "GH/s", "chains": [
                    {"id": 1, "hashrate_rt": 12000.0, "chips": [{"hr": 0.0}, {"hr": -1.0}],
                     "status": {"state": "mining", "description": "Thermal warning"},
                     "chip_statuses": {"red": 0, "orange": "bad", "grey": 88}},
                    {"id": 3, "hashrate_rt": 0.0, "chips": [{"hr": 0.0}],
                     "status": {"state": "failure", "description": "Chain break"},
                     "chip_statuses": {"red": 76, "orange": 0, "grey": 12}},
                    {"id": 4, "hashrate_rt": -1.0,
                     "chip_statuses": {"red": 70000, "orange": -1, "grey": true}},
                    {"id": 0, "hashrate_rt": 9000.0}
                ]}),
            ),
            (
                DataField::Chips,
                json!([{"id": 1, "chips": [{"hr": -1.0}, {"hr": 0.0}]}]),
            ),
            (
                DataField::Hashrate,
                json!({"hashrate": -1.0, "unit": "GH/s"}),
            ),
            (
                DataField::ExpectedHashrate,
                json!({"hashrate": 0.0, "unit": "GH/s"}),
            ),
        ]);

        let boards = miner.parse_hashboards(&data);
        assert_eq!(
            boards
                .iter()
                .map(|board| board.position)
                .collect::<Vec<_>>(),
            vec![0, 2, 3]
        );
        assert!(boards.iter().all(|board| board.expected_chips.is_none()));
        assert_eq!(boards[0].working_chips, None);
        assert_eq!(boards[1].working_chips, Some(0));
        assert_eq!(boards[0].chips[0].working, None);
        assert_eq!(boards[0].chips[1].working, Some(false));
        assert_eq!(boards[0].state.as_deref(), Some("mining"));
        assert_eq!(
            boards[0].status_description.as_deref(),
            Some("Thermal warning")
        );
        assert_eq!(boards[0].red_chips, Some(0));
        assert_eq!(boards[0].orange_chips, None);
        assert_eq!(boards[0].grey_chips, Some(88));
        assert_eq!(boards[1].state.as_deref(), Some("failure"));
        assert_eq!(boards[1].status_description.as_deref(), Some("Chain break"));
        assert_eq!(boards[1].red_chips, Some(76));
        assert_eq!(boards[1].orange_chips, Some(0));
        assert_eq!(boards[2].state, None);
        assert_eq!(boards[2].red_chips, None);
        assert_eq!(boards[2].orange_chips, None);
        assert_eq!(boards[2].grey_chips, None);
        assert!(boards[2].hashrate.is_none());
        assert!(miner.parse_hashrate(&data).is_none());
        assert_eq!(
            miner.parse_expected_hashrate(&data).map(|rate| rate.value),
            Some(0.0)
        );
        assert_eq!(
            boards[1].hashrate.as_ref().map(|rate| rate.value),
            Some(0.0)
        );

        let mut missing_unit = data.clone();
        missing_unit
            .get_mut(&DataField::Hashboards)
            .and_then(Value::as_object_mut)
            .expect("hashboard telemetry")
            .remove("unit");
        missing_unit
            .get_mut(&DataField::ExpectedHashrate)
            .and_then(Value::as_object_mut)
            .expect("expected rate telemetry")
            .remove("unit");
        let unitless_boards = miner.parse_hashboards(&missing_unit);
        assert!(unitless_boards.iter().all(|board| board.hashrate.is_none()));
        assert_eq!(unitless_boards[0].chips[1].working, Some(false));
        assert!(miner.parse_expected_hashrate(&missing_unit).is_none());
    }

    #[test]
    fn reported_wattage_keeps_zero_but_rejects_invalid_values() {
        let miner = VnishV120::new(IpAddr::from([127, 0, 0, 1]), AntMinerModel::S21);
        for (value, expected) in [
            (json!(3200), Some(3200.0)),
            (json!(0), Some(0.0)),
            (json!(-1), None),
            (json!("3200"), None),
            (Value::Null, None),
        ] {
            let data = HashMap::from([(DataField::Wattage, value)]);
            assert_eq!(
                miner.parse_wattage(&data).map(|power| power.as_watts()),
                expected
            );
        }
    }

    #[test]
    fn fan_rpm_keeps_zero_and_ignores_invalid_readings() {
        let miner = VnishV120::new(IpAddr::from([127, 0, 0, 1]), AntMinerModel::S21);
        let data = HashMap::from([(
            DataField::Fans,
            json!([
                {"rpm": 6000}, {"rpm": 0}, {"rpm": -1}, {"rpm": 65536},
                {"rpm": true}, {"rpm": "unavailable"}, {}
            ]),
        )]);

        let fans = miner.parse_fans(&data);
        assert_eq!(
            fans.iter().map(|fan| fan.position).collect::<Vec<_>>(),
            vec![0, 1]
        );
        assert!((fans[0].rpm.unwrap().as_rpm() - 6000.0).abs() < 1e-6);
        assert_eq!(fans[1].rpm.unwrap().as_rpm(), 0.0);
    }

    #[test]
    fn zero_board_sensor_does_not_become_a_safe_temperature() {
        let miner = VnishV120::new(
            IpAddr::from([127, 0, 0, 1]),
            AntMinerModel::Unknown("unmapped model".to_owned()),
        );
        let data = HashMap::from([
            (
                DataField::Hashboards,
                json!({"unit": "GH/s", "chains": [
                    {"id": 1, "hashrate_rt": 12000, "pcb_temp": {"min": 70, "max": 0},
                     "chip_temp": {"min": 80, "max": 0}},
                    {"id": 2, "hashrate_rt": 12000, "pcb_temp": {"max": 74},
                     "chip_temp": {"min": 65, "max": 142}},
                    {"id": 3, "hashrate_rt": 12000, "chip_temp": {"min": 75}},
                    {"id": 4, "hashrate_rt": 12000, "chip_temp": {"min": 76, "max": "bad"}},
                    {"id": 5, "hashrate_rt": 12000, "chip_temp": {"min": 77, "max": -1}}
                ]}),
            ),
            (
                DataField::Chips,
                json!([{"id": 1, "chips": [{"temp": 0}, {"temp": 72}]}]),
            ),
        ]);

        let boards = miner.parse_hashboards(&data);
        assert!(boards[0].board_temperature.is_none());
        assert_eq!(boards[0].inlet_chip_temperature.unwrap().as_celsius(), 80.0);
        assert!(boards[0].outlet_chip_temperature.is_none());
        assert_eq!(
            boards[0]
                .reported_chip_max_temperature
                .unwrap()
                .as_celsius(),
            0.0
        );
        assert_eq!(
            boards[0].reported_chip_max_status,
            Some(ChipTemperatureMaxStatus::ReportedZero)
        );
        assert_eq!(
            boards[0].reported_chip_max_source.as_deref(),
            Some("vnish:chains[].chip_temp.max")
        );
        assert!(boards[0].chips[0].temperature.is_none());
        assert_eq!(boards[0].chips[1].temperature.unwrap().as_celsius(), 72.0);
        assert_eq!(
            boards[1].outlet_chip_temperature.unwrap().as_celsius(),
            142.0
        );
        assert_eq!(
            boards[1].reported_chip_max_status,
            Some(ChipTemperatureMaxStatus::Reported)
        );
        assert_eq!(
            boards[2].reported_chip_max_status,
            Some(ChipTemperatureMaxStatus::Missing)
        );
        assert!(boards[2].reported_chip_max_temperature.is_none());
        assert!(boards[2].hashrate.is_some());
        assert_eq!(
            boards[3].reported_chip_max_status,
            Some(ChipTemperatureMaxStatus::Malformed)
        );
        assert!(boards[3].reported_chip_max_temperature.is_none());
        assert_eq!(
            boards[4].reported_chip_max_status,
            Some(ChipTemperatureMaxStatus::Invalid)
        );
        assert_eq!(
            boards[4]
                .reported_chip_max_temperature
                .unwrap()
                .as_celsius(),
            -1.0
        );
        assert_eq!(
            miner
                .parse_data(data)
                .average_temperature
                .unwrap()
                .as_celsius(),
            74.0
        );
    }

    #[tokio::test]
    async fn restore_stock_os_sends_bodyless_request_and_parses_delay() -> anyhow::Result<()> {
        let MockHttpServer { port, task } = mock_http_server(200, "OK", r#"{"after":3}"#).await?;
        let miner = miner_with_mock_web(port);

        assert!(miner.supports_factory_reset());
        assert!(miner.supports_restore_stock_os());
        assert_eq!(
            miner.restore_stock_os().await?,
            RestoreStockOsResult::accepted(Some(3))
        );

        let request = task.await??;
        let (headers, body) = request
            .split_once("\r\n\r\n")
            .expect("complete HTTP request");
        assert!(headers.starts_with("POST /api/v1/firmware/remove HTTP/1.1\r\n"));
        assert!(
            headers
                .lines()
                .any(|line| { line.eq_ignore_ascii_case("authorization: Bearer test-token") })
        );
        assert!(body.is_empty(), "VNish 1.2.x request body was {body:?}");

        Ok(())
    }

    #[tokio::test]
    async fn restore_stock_os_surfaces_incompatible_model() -> anyhow::Result<()> {
        let MockHttpServer { port, task } =
            mock_http_server(400, "Bad Request", r#"{"error":"unsupported model"}"#).await?;
        let miner = miner_with_mock_web(port);

        let error = miner.restore_stock_os().await.unwrap_err();

        let request = task.await??;
        assert!(request.starts_with("POST /api/v1/firmware/remove HTTP/1.1\r\n"));
        assert!(error.to_string().contains("HTTP error: 400"));

        Ok(())
    }

    /// The 1.2.x backend carries the same hashrate parsing as 1.3.x, so it had
    /// the same hardcoded SHA-256/GH-per-second assumption. `hr_measure` is in
    /// the 1.2.x `/info` response too.
    #[tokio::test]
    async fn scrypt_model_is_read_in_the_unit_it_reports() {
        let miner = VnishV120::new(IpAddr::from([127, 0, 0, 1]), AntMinerModel::L9);

        let mut results = HashMap::new();
        results.insert(
            MinerCommand::WebAPI {
                command: "info",
                parameters: None,
            },
            json!({ "miner": "Antminer L9", "hr_measure": "MH/s", "fw_version": "1.2.7" }),
        );
        results.insert(
            MinerCommand::WebAPI {
                command: "summary",
                parameters: None,
            },
            json!({ "miner": { "hr_realtime": 16200.0, "hr_stock": 16000.0 } }),
        );

        let mock_api = MockAPIClient::new(results);
        let mut collector = DataCollector::new_with_client(&miner, &mock_api);
        let miner_data = miner.parse_data(collector.collect_all().await);

        let hashrate = miner_data.hashrate.expect("hashrate");
        assert_eq!(hashrate.algo, HashAlgorithm::Scrypt);
        let value = hashrate.as_unit(HashRateUnit::MegaHash).value;
        assert!((value - 16200.0).abs() < 1e-6, "got {value} MH/s");
    }

    /// VNish's `"GMT+2"` is UTC+2, which is `Etc/GMT-2` in the POSIX-signed
    /// IANA names the config reports; the offered list flips the same way.
    #[test]
    fn timezone_is_reported_as_the_sign_inverted_etc_gmt_zone() {
        use asic_rs_core::config::timezone::Tz;

        let miner = VnishV120::new(IpAddr::from([127, 0, 0, 1]), AntMinerModel::S19);

        let mut data = HashMap::new();
        data.insert(ConfigField::Timezone, json!({ "current": "GMT+2" }));
        let config = miner.parse_timezone_config(&data).expect("timezone config");

        assert_eq!(config.timezone, Some(Tz::Etc__GMTMinus2));
        assert_eq!(config.available.len(), 27);
        assert_eq!(config.available.first(), Some(&Tz::Etc__GMTPlus12));
        assert!(config.available.contains(&Tz::Etc__GMT));
        assert_eq!(config.available.last(), Some(&Tz::Etc__GMTMinus14));
    }
}
