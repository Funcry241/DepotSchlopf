"""Gegenproben für die Wiederverwendung identischer PR-Prüfungen."""

import unittest

from kodex_nachweis import WORKFLOW, finde_lauf, name

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

    def api(self, pfad):
        return {
            f"repos/{REPO}/actions/artifacts?name={name(BAUM, BASIS)}&per_page=20":
                {"artifacts": [self.beleg]},
            f"repos/{REPO}/actions/artifacts?name={name(BAUM, ANDERER_BAUM)}&per_page=20":
                {"artifacts": []},
            f"repos/{REPO}/actions/runs/42": self.lauf,
            f"repos/{REPO}/git/commits/{KOPF}": self.git,
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

    def test_abgelaufen(self):
        self.beleg["expired"] = True
        self.assertIsNone(self.finde())


if __name__ == "__main__":
    unittest.main()
