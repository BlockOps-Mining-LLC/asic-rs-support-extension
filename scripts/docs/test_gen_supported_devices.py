"""Regression checks for source-derived backend discovery and firmware identity."""

from pathlib import Path
import unittest
from unittest.mock import patch

import gen_supported_devices as generator


class SupportedDevicesGeneratorTests(unittest.TestCase):
    def test_flat_and_delegated_read_only_backends_have_distinct_rows(self):
        rows = {row.backend: row for row in generator.collect_support_rows()}
        expected = {
            "GoldshellV1": ("Goldshell", "Goldshell Stock (read-only)"),
            "InnosiliconV1": ("Innosilicon", "Innosilicon Stock (read-only)"),
            "KaonsuMiner": ("AntMiner", "KaonSu"),
            "HiveonMiner": ("AntMiner", "Hiveon"),
            "IceRiverV1": ("IceRiver", "IceRiver Stock"),
        }
        for backend, identity in expected.items():
            with self.subTest(backend=backend):
                self.assertIn(backend, rows)
                self.assertEqual((rows[backend].make, rows[backend].firmware), identity)
                self.assertEqual(set(rows[backend].support.values()), {"No"})
        for backend in ("AntMinerV2020", "AntMinerV202307"):
            self.assertEqual(rows[backend].firmware, "AntMiner Stock")
            self.assertEqual(rows[backend].support["Restart"], "Yes")

    def test_delegated_device_info_override_precedes_constructor_identity(self):
        source = """
            impl Wrapper {
                fn new() -> Self {
                    let mut inner = DeviceInfo::new(model, StockFirmware, algo);
                    inner.device_info.firmware = AlternateFirmware.to_string();
                    Self { inner }
                }
            }
            impl Validate for Wrapper { type Firmware = StockFirmware; }
        """
        self.assertEqual(generator.backend_firmware_type(source, "Wrapper"), "AlternateFirmware")

    def test_display_lookup_is_type_specific_and_accepts_write_str(self):
        source = """
            impl Display for Product { fn fmt() { write!(f, "Product name") } }
            impl std::fmt::Display for AlternateFirmware { fn fmt() { f.write_str("Alternate") } }
        """
        with patch.object(generator, "read_text", return_value=source), patch.object(Path, "exists", return_value=True):
            path = Path("firmware.rs")
            self.assertEqual(generator.parse_display_string(path, "AlternateFirmware"), "Alternate")
            self.assertIsNone(generator.parse_display_string(path, "MissingFirmware"))


if __name__ == "__main__":
    unittest.main()
