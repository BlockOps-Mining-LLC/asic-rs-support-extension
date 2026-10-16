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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn al3_keeps_verified_board_fan_counts_but_unknown_expected_chip_capacity() {
        let hardware = MinerHardware::from(IceRiverModel::AL3);
        assert_eq!(hardware.board_count(), Some(3));
        assert_eq!(hardware.chips_for_board(0), None);
        assert_eq!(hardware.total_chips(), None);
        assert_eq!(hardware.fans, Some(4));
        assert_eq!(MinerHardware::from(IceRiverModel::KS3).total_chips(), None);
    }
}
