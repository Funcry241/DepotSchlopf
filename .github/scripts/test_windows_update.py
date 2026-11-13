"""Gegenproben zu #150 V5."""
import unittest

from windows_update import update_noetig, windows_gruen


class WindowsUpdateTest(unittest.TestCase):
    def test_lagebild_pr_ueberspringt_update_und_bleibt_gruen(self):
        self.assertFalse(update_noetig(
            "pull_request", ["crates/depot_lage/src/lib.rs", "kodex/aenderung.txt"]
        ))
        self.assertTrue(windows_gruen("success", "false", "success", "skipped"))

    def test_updater_pr_prueft_update(self):
        self.assertTrue(update_noetig("pull_request", ["crates/depot_lauf/src/update.rs"]))
        self.assertTrue(windows_gruen("success", "true", "success", "success"))
        self.assertFalse(windows_gruen("success", "true", "success", "skipped"))

    def test_push_auf_main_prueft_update_immer(self):
        self.assertTrue(update_noetig("push", []))
        self.assertTrue(update_noetig("push", ["docs/kaltstart.md"]))

    def test_weitere_relevante_pfade(self):
        for pfad in (
            "crates/depot_kodex/src/lib.rs", "kodex/festschreibung.txt",
            "Cargo.toml", "Cargo.lock", "rust-toolchain.toml", ".github/workflows/kodex.yml",
        ):
            with self.subTest(pfad=pfad):
                self.assertTrue(update_noetig("pull_request", [pfad]))

    def test_fehler_und_unklare_filterentscheidung_sperren(self):
        for werte in (
            ("failure", "false", "success", "skipped"),
            ("cancelled", "false", "success", "skipped"),
            ("success", "", "success", "skipped"),
            ("success", "true", "success", "failure"),
            ("success", "false", "failure", "skipped"),
        ):
            with self.subTest(werte=werte):
                self.assertFalse(windows_gruen(*werte))


if __name__ == "__main__":
    unittest.main()
