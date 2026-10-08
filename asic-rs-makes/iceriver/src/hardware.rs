use asic_rs_core::data::device::MinerHardware;

use crate::models::IceRiverModel;

impl From<IceRiverModel> for MinerHardware {
    fn from(model: IceRiverModel) -> Self {
        match model {
            IceRiverModel::AL3 => Self {
                fans: Some(4),
                // Live chipnum is an observed working count, not capacity.
                boards: Some(vec![None; 3]),
            },
            // Model identity alone does not establish the firmware's board or chip capacity.
            _ => Self::default(),
        }
    }
}
