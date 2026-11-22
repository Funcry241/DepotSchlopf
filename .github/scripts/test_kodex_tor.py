"""Execute the real observation script with deterministic GitHub responses."""
import argparse
from contextlib import redirect_stdout
from dataclasses import dataclass
import io
import json
import os
from pathlib import Path
import re
import subprocess
import tempfile
from unittest.mock import patch

HEAD = "1" * 40
BASE = "2" * 40
OTHER = "3" * 40
REPO = "Funcry241/DepotSchlopf"
SERVER = "https://github.com"
API = "https://api.github.com"
RUN = 100


@dataclass
class Probe:
    name: str
    output: tuple[str, str]
    r30: str = "ok"
    conclusion: str = "success"
    status: str = "success"
    basis: str = BASE
    description_head: str = HEAD
    attempt_in_description: int = 1
    author: str = "github-actions[bot]"
    head_seen_by_workflow: str = HEAD
    event_seen_by_workflow: str = "pull_request_target"
    status_present: bool = True
    event: str = "pull_request"
    base_ref: str = "main"
    pending_rounds: int = 0


def workflow_script(workflow):
    section = workflow.read_text().split("\n  kodex-tor:\n", 1)[1].split("\n  kodex-nachweis:\n", 1)[0]
    _, remainder = section.split("          python - <<'PY'\n", 1)
    source, _ = remainder.split("\n          PY", 1)
    return compile("\n".join(line.removeprefix("          ") for line in source.splitlines()),
                   str(workflow), "exec")


def run_probe(script, probe):
    tick = 0.0
    polls = 0
    reads = []

    def sleep(seconds):
        nonlocal tick
        assert seconds > 0
        tick += seconds

    def fake_api(args, **kwargs):
        nonlocal polls
        url = args[-1]
        assert kwargs["timeout"] <= 15
        reads.append(url)
        if url == f"{API}/repos/{REPO}/commits/{HEAD}/statuses?per_page=100":
            polls += 1
            status = ("pending" if polls <= probe.pending_rounds else probe.status)
            response = [{
                "context": "kodex-basis", "state": status, "creator": {"login": probe.author},
                "target_url": f"{SERVER}/{REPO}/actions/runs/{RUN}",
                "description": (f"head={probe.description_head} basis={probe.basis} "
                                f"R30={probe.r30} versuch={probe.attempt_in_description}"),
            }] if probe.status_present else []
        elif url == f"{API}/repos/{REPO}/actions/runs/{RUN}":
            response = {
                "event": probe.event_seen_by_workflow, "path": ".github/workflows/kodex-basis.yml",
                "head_sha": probe.head_seen_by_workflow, "status": "completed",
                "conclusion": probe.conclusion, "run_attempt": 1,
            }
        else:
            raise AssertionError(f"Unerwarteter API-Aufruf: {url}")
        return subprocess.CompletedProcess(args, 0, stdout=json.dumps(response), stderr="")

    with tempfile.TemporaryDirectory() as folder:
        output = Path(folder) / "github-output"
        environment = {
            "GITHUB_OUTPUT": str(output), "GITHUB_EVENT_NAME": probe.event, "BASE_REF": probe.base_ref,
            "GITHUB_REPOSITORY": REPO, "GITHUB_SERVER_URL": SERVER, "GITHUB_API_URL": API,
            "HEAD_SHA": HEAD, "BASE_SHA": BASE, "GH_TOKEN": "synthese",
        }
        stdout = io.StringIO()
        with patch.dict(os.environ, environment, clear=True), patch("subprocess.run", fake_api), \
                patch("time.monotonic", lambda: tick), patch("time.sleep", sleep), redirect_stdout(stdout):
            try:
                exec(script, {})
            except SystemExit as exc:
                assert exc.code in (0, None)
        expected = dict(zip(("start", "reason"), probe.output))
        actual = dict(line.split("=", 1) for line in output.read_text().splitlines())
        assert actual == expected, (probe.name, actual, expected, stdout.getvalue())
        assert tick <= 90 and (bool(reads) == (probe.event != "push" and probe.base_ref == "main"))
        print(f"{probe.name:39} {actual['start']:5} {actual['reason']:18} {tick:>4.0f} s virtuell")



def check_job_dependencies(workflow):
    source = workflow.read_text()
    headings = list(re.finditer(r"(?m)^  ([\w-]+):\n", source))
    jobs = {
        found[1]: source[found.end():next_heading.start() if next_heading else len(source)]
        for found, next_heading in zip(headings, [*headings[1:], None])
    }
    filters = {
        "windows-update-filter": "needs.kodex-nachweis.outputs.wiederverwendet != 'ja'",
        "kodex-linux-gate": "needs.kodex-nachweis.outputs.wiederverwendet != 'ja'",
        "kodex-linux-update": "needs.kodex-nachweis.outputs.wiederverwendet != 'ja'",
        "kodex-windows-gate": "needs.kodex-nachweis.outputs.wiederverwendet != 'ja'",
        "kodex-windows-update": (
            "needs.windows-update-filter.result == 'success' && "
            "needs.windows-update-filter.outputs.update == 'true'"
        ),
    }
    for name, further_condition in filters.items():
        job = jobs[name]
        assert re.search(r"(?m)^    needs: \[kodex-tor, ", job), name
        expected = "!cancelled() && needs.kodex-tor.outputs.start == 'true' && " + further_condition
        assert "    if: " + "$" + "{{ " + expected + " }}" in job, name
    for name in ("kodex", "kodex-windows"):
        job = jobs[name]
        assert "    if: " + "$" + "{{ always() }}" in job, name
        assert "TOR_START: " + "$" + "{{ needs.kodex-tor.outputs.start }}" in job, name
        _, commands = job.split("      - name: Startfreigabe des Kodex-Tors prüfen\n", 1)
        commands = commands.split("        run: |\n", 1)[1].split("\n      -", 1)[0]
        for state, start, expected in [
            ("success", "true", 0), ("success", "false", 1), ("success", "", 1),
            ("cancelled", "true", 1), ("skipped", "", 1), ("failure", "true", 1),
        ]:
            process = subprocess.run(["bash", "-c", commands], capture_output=True, text=True,
                                     env={**os.environ, "TOR": state, "TOR_START": start, "TOR_GRUND": "Test"})
            assert process.returncode == expected, (name, state, start, process.stdout, process.stderr)
    print("Fünf Folgejobs brauchen die Startfreigabe. Beide Pflichtchecks bleiben bei Sperre rot.")


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("workflow", type=Path)
    args = parser.parse_args()
    script = workflow_script(args.workflow)
    waiting = ("false", "basis-unbestimmt")
    green = ("true", "basis-gruen")
    probes = [
        Probe("Grüner Nachweis", green),
        Probe("Grüner Nachweis kommt nach 3 Abfragen", green, pending_rounds=3),
        Probe("R30-Freigabe offen", ("false", "R30-offen"), r30="offen", conclusion="failure", status="failure"),
        Probe("Basisprüfung fehlgeschlagen", ("false", "basis-fehler"), r30="fehler", conclusion="failure", status="failure"),
        Probe("Anderer Stand von main", ("false", "basis-bewegt"), basis=OTHER),
        Probe("Abbruch trotz grünem Status", waiting, conclusion="cancelled"),
        Probe("Abbruch trotz Fehlerstatus", waiting, conclusion="cancelled", status="failure", r30="fehler"),
        Probe("Basisstatus fehlt ganz", waiting, status_present=False),
        Probe("Alter Prüfversuch", waiting, attempt_in_description=2),
        Probe("Nachweis eines anderen PR-Heads", waiting, description_head=OTHER),
        Probe("Basislauf eines anderen PR-Heads", waiting, head_seen_by_workflow=OTHER),
        Probe("Fremder Absender meldet Grün", waiting, author="fremder-nutzer"),
        Probe("Falscher GitHub-Ereignistyp", waiting, event_seen_by_workflow="pull_request"),
        Probe("Zielzweig ist nicht main", ("false", "basis-nicht-main"), base_ref="probe"),
        Probe("Push auf main", ("true", "push"), event="push"),
    ]
    for probe in probes:
        run_probe(script, probe)
    print(f"{len(probes)} Gegenproben bestanden.")
    check_job_dependencies(args.workflow)


if __name__ == "__main__":
    main()
