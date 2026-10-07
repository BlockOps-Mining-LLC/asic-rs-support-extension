use std::net::IpAddr;

use asic_rs_core::traits::{
    miner::{Miner, MinerConstructor},
    model::MinerModel,
};

pub mod v1;

pub struct IceRiver;

impl MinerConstructor for IceRiver {
    fn new(
        ip: IpAddr,
        model: impl MinerModel,
        _version: Option<semver::Version>,
    ) -> Box<dyn Miner> {
        Box::new(v1::IceRiverV1::new(ip, model))
    }
}
