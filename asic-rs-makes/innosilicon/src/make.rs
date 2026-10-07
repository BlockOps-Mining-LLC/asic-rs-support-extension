use crate::models::InnosiliconModel;
use asic_rs_core::{
    data::board::MinerControlBoard, errors::ModelSelectionError, traits::make::MinerMake,
};
use std::{fmt::Display, str::FromStr};

#[derive(Default, Debug)]
pub struct InnosiliconMake;
impl Display for InnosiliconMake {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "Innosilicon")
    }
}
impl MinerMake for InnosiliconMake {
    type Model = InnosiliconModel;
    fn parse_model(model: String) -> Result<Self::Model, ModelSelectionError> {
        InnosiliconModel::from_str(&model)
    }
    fn parse_control_board(&self, _cb_type: &str) -> Option<MinerControlBoard> {
        None
    }
}
