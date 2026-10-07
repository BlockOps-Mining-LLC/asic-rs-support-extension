use asic_rs_core::data::device::MinerHardware;

use crate::models::IceRiverModel;

impl From<IceRiverModel> for MinerHardware {
    fn from(model: IceRiverModel) -> Self {
        // Counts follow the established pyasic IceRiver model definitions.
        // Unspecified chip counts stay unknown instead of borrowing another model's counts.
        match model {
            IceRiverModel::AL3 => Self {
                fans: Some(4),
                boards: Some(vec![Some(156); 3]),
            },
            IceRiverModel::KS0 => Self {
                fans: Some(0),
                boards: Some(vec![None; 3]),
            },
            IceRiverModel::KS2
            | IceRiverModel::KS3M
            | IceRiverModel::KS5L
            | IceRiverModel::KS5M => Self {
                fans: Some(4),
                boards: Some(vec![Some(18); 3]),
            },
            IceRiverModel::KS5 => Self {
                fans: Some(4),
                boards: Some(vec![Some(92); 3]),
            },
            IceRiverModel::KS1 | IceRiverModel::KS3 | IceRiverModel::KS3L => Self {
                fans: Some(4),
                boards: Some(vec![None; 3]),
            },
            IceRiverModel::Unknown(_) => Self::default(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn al3_hardware_counts_are_per_board_and_total() {
        let hardware = MinerHardware::from(IceRiverModel::AL3);
        assert_eq!(hardware.board_count(), Some(3));
        assert_eq!(hardware.chips_for_board(0), Some(156));
        assert_eq!(hardware.total_chips(), Some(468));
        assert_eq!(hardware.fans, Some(4));
        assert_eq!(MinerHardware::from(IceRiverModel::KS3).total_chips(), None);
    }
}
