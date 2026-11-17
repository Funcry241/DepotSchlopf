"""Bindet einen vollständig grünen PR-Lauf an seinen geprüften Baum und seine Basis."""

import json
import os
from pathlib import Path
import re
import subprocess
import sys

WORKFLOW = ".github/workflows/kodex.yml"
EXPORT = "assistant_export_latest"
SHA = re.compile(r"[0-9a-f]{40}")


def befehl(*args):
    return subprocess.run(
        args, capture_output=True, text=True, check=True, timeout=35
    ).stdout.strip()


def git_baum(sha):
    if not SHA.fullmatch(sha):
        raise ValueError("Git-SHA ungültig")
    return befehl("git", "rev-parse", "--verify", f"{sha}^{{tree}}")


def name(baum, basis):
    if not SHA.fullmatch(baum) or not SHA.fullmatch(basis):
        raise ValueError("Baum-SHA ungültig")
    return f"geprueft-{baum}-{basis}"


def form_treffer(beleg, lauf, repo_id):
    """Nur GitHubs eigene Metadaten entscheiden über Herkunft und Abschluss."""
    kopf = beleg.get("workflow_run") or {}
    repo = lauf.get("repository") or {}
    kopfrepo = lauf.get("head_repository") or {}
    return (
        beleg.get("expired") is False
        and repo_id == kopf.get("repository_id") == kopf.get("head_repository_id")
        and repo_id == repo.get("id") == kopfrepo.get("id")
        and lauf.get("id") == kopf.get("id")
        and lauf.get("status") == "completed"
        and lauf.get("conclusion") == "success"
        and lauf.get("event") == "pull_request"
        and lauf.get("path") == WORKFLOW
        and lauf.get("run_attempt") == 1
        and lauf.get("head_sha") == kopf.get("head_sha")
    )


def windows_update_bestanden(jobs):
    """Ein übersprungener Job oder Prüfschritt trägt keinen Nachweis."""
    for job in jobs:
        if (
            job.get("name") == "kodex-windows-update"
            and job.get("status") == "completed"
            and job.get("conclusion") == "success"
        ):
            return any(
                schritt.get("name") == "Alter Updater bei laufendem Dienst"
                and schritt.get("status") == "completed"
                and schritt.get("conclusion") == "success"
                for schritt in job.get("steps", [])
            )
    return False


def finde_lauf(api, repo, repo_id, baum, basis):
    belege = api(f"repos/{repo}/actions/artifacts?name={name(baum, basis)}&per_page=20")
    for beleg in belege.get("artifacts", []):
        kopf = beleg.get("workflow_run") or {}
        lauf_id = kopf.get("id")
        if type(lauf_id) is not int or lauf_id <= 0:
            continue
        if beleg.get("name") != name(baum, basis) or beleg.get("expired") is not False:
            continue
        lauf = api(f"repos/{repo}/actions/runs/{lauf_id}")
        if not form_treffer(beleg, lauf, repo_id):
            continue
        sha = lauf.get("head_sha", "")
        if not isinstance(sha, str) or not SHA.fullmatch(sha):
            continue
        # Das Artefakt allein darf keinen anderen Baum als geprüft ausgeben.
        git_commit = api(f"repos/{repo}/git/commits/{sha}")
        if git_commit.get("sha") != sha or (git_commit.get("tree") or {}).get("sha") != baum:
            continue
        jobs = api(f"repos/{repo}/actions/runs/{lauf_id}/attempts/1/jobs?per_page=100")
        if windows_update_bestanden(jobs.get("jobs", [])):
            return lauf_id
    return None


def api(pfad):
    return json.loads(befehl("gh", "api", pfad))


def ausgabe(feld, wert):
    with Path(os.environ["GITHUB_OUTPUT"]).open("a", encoding="utf-8") as ziel:
        print(f"{feld}={wert}", file=ziel)


def ereignis_daten():
    return json.loads(Path(os.environ["GITHUB_EVENT_PATH"]).read_text(encoding="utf-8"))


def pr_basis(geprueft, kopf):
    eltern = befehl("git", "show", "-s", "--format=%H %P", "HEAD").split()
    if len(eltern) != 3 or eltern[0] != geprueft or eltern[2] != kopf:
        raise ValueError("Checkout ist nicht der Merge-Stand des PR-Laufs")
    return eltern[1]


def belege_pr():
    ereignis = ereignis_daten()
    pr = ereignis["pull_request"]
    repository = ereignis["repository"]["id"]
    if pr["base"]["ref"] != "main" or pr["head"]["repo"]["id"] != repository:
        return
    if os.environ["GITHUB_RUN_ATTEMPT"] != "1":
        return
    kopf = pr["head"]["sha"]
    geprueft = os.environ["GITHUB_SHA"]
    basis = pr_basis(geprueft, kopf)
    baum = git_baum(geprueft)
    # Nur der mit GitHubs API unabhängig überprüfbare Fall bekommt einen Beleg.
    if git_baum(kopf) != baum:
        return
    belegname = name(baum, git_baum(basis))
    Path(os.environ["RUNNER_TEMP"], "kodex-nachweis.txt").write_text(
        f"{belegname}\n", encoding="utf-8"
    )
    ausgabe("name", belegname)


def hole_export():
    ereignis = ereignis_daten()
    if ereignis["ref"] != "refs/heads/main" or ereignis.get("forced"):
        return
    repo = os.environ["GITHUB_REPOSITORY"]
    stand = os.environ["GITHUB_SHA"]
    if ereignis["after"] != stand or befehl("git", "rev-parse", "HEAD") != stand:
        return
    lauf = finde_lauf(
        api, repo, ereignis["repository"]["id"], git_baum(stand), git_baum(ereignis["before"])
    )
    if lauf is None:
        return
    ordner = Path(os.environ["RUNNER_TEMP"], "kodex-export")
    befehl("gh", "run", "download", str(lauf), "--repo", repo, "--name", EXPORT, "--dir", str(ordner))
    export = ordner / f"{EXPORT}.zip"
    if not export.is_file() or export.is_symlink():
        return
    ausgabe("lauf", lauf)
    print(f"Identischer Baum und identische Basis, vollständiger PR-Lauf {lauf}.")


if __name__ == "__main__":
    aufgabe = sys.argv[1]
    if aufgabe == "basis":
        pr = ereignis_daten()["pull_request"]
        print(pr_basis(os.environ["GITHUB_SHA"], pr["head"]["sha"]))
        sys.exit(0)
    try:
        {"beleg": belege_pr, "export": hole_export}[aufgabe]()
    except (OSError, ValueError, KeyError, TypeError, subprocess.SubprocessError) as fehler:
        # Bei fehlendem oder nicht nachprüfbarem Beleg bleiben die regulären Jobs aktiv.
        print(f"Kodex-Nachweis nicht verfügbar ({type(fehler).__name__}).")
