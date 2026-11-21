"""Gegenproben für die Wiederverwendung identischer PR-Prüfungen."""

import unittest
from os import environ
from unittest.mock import patch

from kodex_nachweis import WORKFLOW, finde_lauf, hole_export, name, pr_basis

BAUM = "1" * 40
BASIS = "2" * 40
KOPF = "3" * 40
ANDERER_BAUM = "4" * 40
REPO = "Funcry241/DepotSchlopf"


class Nachweis(unittest.TestCase):
    def setUp(self):
        self.beleg = {
            "name": name(BAUM, BASIS),
            "expired": False,
            "workflow_run": {
                "id": 42, "repository_id": 1, "head_repository_id": 1, "head_sha": KOPF
            },
        }
        self.lauf = {
            "id": 42, "repository": {"id": 1}, "head_repository": {"id": 1},
            "event": "pull_request", "path": WORKFLOW,
            "status": "completed", "conclusion": "success",
            "run_attempt": 1, "head_sha": KOPF,
        }
        self.git = {"sha": KOPF, "tree": {"sha": BAUM}}
        self.windows_update = {
            "name": "kodex-windows-update",
            "status": "completed",
            "conclusion": "success",
            "steps": [{
                "name": "Alter Updater bei laufendem Dienst",
                "status": "completed",
                "conclusion": "success",
            }],
        }
        self.jobs = {"jobs": [self.windows_update]}

    def api(self, pfad):
        return {
            f"repos/{REPO}/actions/artifacts?name={name(BAUM, BASIS)}&per_page=20":
                {"artifacts": [self.beleg]},
            f"repos/{REPO}/actions/artifacts?name={name(BAUM, ANDERER_BAUM)}&per_page=20":
                {"artifacts": []},
            f"repos/{REPO}/actions/runs/42": self.lauf,
            f"repos/{REPO}/git/commits/{KOPF}": self.git,
            f"repos/{REPO}/actions/runs/42/attempts/1/jobs?per_page=100": self.jobs,
        }[pfad]

    def finde(self, basis=BASIS):
        return finde_lauf(self.api, REPO, 1, BAUM, basis)

    def test_identischer_baum_und_identische_basis(self):
        self.assertEqual(self.finde(), 42)

    def test_andere_basis(self):
        self.assertIsNone(self.finde(ANDERER_BAUM))

    def test_artefaktname_gleicher_git_baum_anders(self):
        self.git["tree"]["sha"] = ANDERER_BAUM
        self.assertIsNone(self.finde())

    def test_metadaten_muessen_zusammenpassen(self):
        for feld, wert in {
            "status": "in_progress",
            "conclusion": "failure",
            "event": "push",
            "path": ".github/workflows/anderer.yml",
            "head_repository": {"id": 8},
            "head_sha": "5" * 40,
            "run_attempt": 2,
        }.items():
            with self.subTest(feld=feld):
                alt = self.lauf[feld]
                self.lauf[feld] = wert
                self.assertIsNone(self.finde())
                self.lauf[feld] = alt

    def test_windows_update_uebersprungen_trotz_gruenem_workflow(self):
        self.windows_update["conclusion"] = "skipped"
        self.assertIsNone(self.finde())

    def test_windows_update_fehlt(self):
        self.jobs["jobs"].clear()
        self.assertIsNone(self.finde())

    def test_windows_update_schritt_uebersprungen_trotz_gruenem_job(self):
        self.windows_update["steps"][0]["conclusion"] = "skipped"
        self.assertIsNone(self.finde())

    def test_pr_basis_entspricht_dem_ersten_elterncommit_und_dem_ereignis(self):
        with patch("kodex_nachweis.befehl", return_value=f"{BAUM} {BASIS} {KOPF}"):
            self.assertEqual(pr_basis(BAUM, KOPF, BASIS), BASIS)

    def test_pr_basis_weist_falschen_checkout_oder_kopf_ab(self):
        with patch("kodex_nachweis.befehl", return_value=f"{BAUM} {BASIS} {KOPF}"):
            with self.assertRaises(ValueError):
                pr_basis(ANDERER_BAUM, KOPF, BASIS)
            with self.assertRaises(ValueError):
                pr_basis(BAUM, ANDERER_BAUM, BASIS)

    def test_pr_basis_weist_abweichende_basisrevision_des_ereignisses_ab(self):
        with patch("kodex_nachweis.befehl", return_value=f"{BAUM} {BASIS} {KOPF}"):
            with self.assertRaisesRegex(ValueError, "Basisrevision des PR-Ereignisses"):
                pr_basis(BAUM, KOPF, "5" * 40)

    def test_geaenderte_cachegrundlage_startet_regulaeren_lauf(self):
        event = {
            "ref": "refs/heads/main", "after": KOPF, "before": BASIS,
            "repository": {"id": 1},
        }
        env = {"GITHUB_REPOSITORY": REPO, "GITHUB_SHA": KOPF}
        with (
            patch.dict(environ, env),
            patch("kodex_nachweis.ereignis_daten", return_value=event),
            patch("kodex_nachweis.befehl", side_effect=[KOPF, "Cargo.lock"]),
            patch("kodex_nachweis.finde_lauf") as suche,
            patch("builtins.print"),
        ):
            hole_export()
        suche.assert_not_called()

    def test_abgelaufen(self):
        self.beleg["expired"] = True
        self.assertIsNone(self.finde())


if __name__ == "__main__":
    unittest.main()
