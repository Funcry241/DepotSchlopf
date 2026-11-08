"""Gegenprobe nur für isolierte Prüfmaschinen, niemals im Benutzerbetrieb starten."""
import argparse
import contextlib
import os
from pathlib import Path
import socket
import subprocess
import tempfile

ALT = "4da352e16cba40b07ff62e50a81f4491523c57b9"


def lauf(args, cwd, protokoll, env=None):
    with protokoll.open("a", encoding="utf-8") as log:
        ergebnis = subprocess.run(args, cwd=cwd, env=env, stdout=log,
                                  stderr=subprocess.STDOUT, timeout=1800, check=False)
    if ergebnis.returncode:
        zeilen = protokoll.read_text(encoding="utf-8", errors="replace").splitlines()
        raise RuntimeError(f"Prüfbefehl fehlgeschlagen: {args[0]}\n" + "\n".join(zeilen[-100:]))


def git(args, cwd, env):
    return subprocess.check_output(["git", *args], cwd=cwd, env=env,
                                   stderr=subprocess.STDOUT, text=True).strip()


def sperren(datei):
    datei.seek(0)
    if os.name == "nt":
        import msvcrt
        msvcrt.locking(datei.fileno(), msvcrt.LK_NBLCK, 1)
    else:
        import fcntl
        fcntl.flock(datei.fileno(), fcntl.LOCK_EX | fcntl.LOCK_NB)


def entsperren(datei):
    datei.seek(0)
    if os.name == "nt":
        import msvcrt
        msvcrt.locking(datei.fileno(), msvcrt.LK_UNLCK, 1)
    else:
        import fcntl
        fcntl.flock(datei.fileno(), fcntl.LOCK_UN)


@contextlib.contextmanager
def laufender_dienst():
    # Nicht übernehmen oder löschen, falls hier bereits ein echter Wächter läuft.
    pfad = Path(tempfile.gettempdir()) / "depotschlopf-waechter-47410.lock"
    with pfad.open("a+b") as sperre:
        sperren(sperre)
        try:
            with socket.socket(socket.AF_INET, socket.SOCK_STREAM) as port:
                port.bind(("127.0.0.1", 47410))
                port.listen(1)
                yield pfad
        finally:
            entsperren(sperre)


def pruefe(wurzel, arbeit):
    env = {k: v for k, v in os.environ.items() if not k.upper().startswith("GIT_")}
    env.update(GIT_CONFIG_GLOBAL="NUL" if os.name == "nt" else "/dev/null",
               GIT_CONFIG_NOSYSTEM="1", GIT_ALLOW_PROTOCOL="file")
    protokoll = arbeit / "gegenprobe.log"
    neu = git(["rev-parse", "HEAD"], wurzel, env)
    altquelle, ursprung, projekt = (arbeit / p for p in ("alt", "ursprung.git", "projekt"))
    git(["clone", "--local", "--no-hardlinks", "--no-checkout", str(wurzel), str(altquelle)], arbeit, env)
    git(["checkout", "--detach", ALT], altquelle, env)
    bau_env = dict(env, CARGO_TARGET_DIR=str(arbeit / "bau-alt"))
    print("[Gegenprobe] Baue den installierten alten Updater.", flush=True)
    lauf(["cargo", "build", "--locked", "-p", "depot_lauf"], altquelle, protokoll, bau_env)
    git(["clone", "--bare", "--local", "--no-hardlinks", str(wurzel), str(ursprung)], arbeit, env)
    git(["update-ref", "refs/heads/main", neu], ursprung, env)
    git(["clone", "--local", "--no-hardlinks", "--branch", "main", str(ursprung), str(projekt)], arbeit, env)
    git(["reset", "--hard", ALT], projekt, env)
    daten = projekt / "daten"
    daten.mkdir(exist_ok=True)
    beleg = daten / "gegenprobe.txt"
    beleg.write_bytes(b"Papierdaten unveraendert\n")

    # Die alte Gate-Umgebung übernimmt XDG_CONFIG_HOME, entfernt aber GIT_CONFIG_*.
    # So wäre im inneren Testlauf wieder die störende persönliche Git-Config sichtbar.
    xdg = arbeit / "nutzer"
    (xdg / "git").mkdir(parents=True)
    (xdg / "git/config").write_text('[protocol "file"]\nallow = never\n', encoding="utf-8")
    update_env = dict(os.environ, XDG_CONFIG_HOME=str(xdg))
    for k in list(update_env):
        if k.upper().startswith("GIT_"):
            del update_env[k]
    # Nur der äußere lokale Ersatz für GitHub darf die lokale Quelle abrufen.
    update_env.update(GIT_CONFIG_COUNT="1", GIT_CONFIG_KEY_0="protocol.file.allow",
                      GIT_CONFIG_VALUE_0="always")
    altprogramm = arbeit / "bau-alt/debug" / ("depotschlopf.exe" if os.name == "nt" else "depotschlopf")
    print("[Gegenprobe] Halte Dienst-Port und Wächtersperre während des alten Updates.", flush=True)
    with laufender_dienst() as pfad:
        lauf([str(altprogramm), "update", neu], projekt, protokoll, update_env)
        with pfad.open("r+b") as zweite:
            try:
                sperren(zweite)
            except (OSError, BlockingIOError):
                pass
            else:
                entsperren(zweite)
                raise RuntimeError("Bestehende Wächtersperre wurde verloren.")
    text = protokoll.read_text(encoding="utf-8", errors="replace")
    if "[Kodex] Gate offen." not in text or "Tests laufen mit eigenen temporären Dateien" not in text:
        raise RuntimeError("Der alte Updater hat das neue isolierte Gate nicht vollständig durchlaufen.")
    if git(["rev-parse", "HEAD"], projekt, env) != neu:
        raise RuntimeError("Der geprüfte Stand wurde nicht eingespielt.")
    if not (daten / "update_offen.txt").is_file() or beleg.read_bytes() != b"Papierdaten unveraendert\n":
        raise RuntimeError("Update-Nachweis fehlt oder Papierdaten wurden verändert.")
    print("[Gegenprobe] Alter Updater erfolgreich: Dienst-Port und Sperre blieben belegt; Daten blieben unverändert.", flush=True)


if __name__ == "__main__":
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--isolierte-pruefmaschine", action="store_true", required=True)
    parser.add_argument("--arbeit", type=Path, required=True,
                        help="Neuer leerer Arbeitsordner; bleibt für die Diagnose erhalten.")
    args = parser.parse_args()
    args.arbeit.mkdir(parents=True, exist_ok=False)
    pruefe(Path.cwd().resolve(), args.arbeit.resolve())
