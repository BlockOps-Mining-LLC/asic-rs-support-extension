use asic_rs_core::data::device::MinerHardware;

use crate::models::GoldshellModel;

impl From<GoldshellModel> for MinerHardware {
    fn from(_: GoldshellModel) -> Self {
        // Observed working chips do not establish model-specific expected capacity.
        Self::default()
    }
}
