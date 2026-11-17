"""Windows-Abnahme P51 (#313 E5): Dienststart bis „bereit“ mit großem Messjournal.

Nur erfundene Daten: Das Journal erzeugt `crates/depot_sim/tests/messjournal.rs`.
Aufruf:  python -B p51_messung.py <wurzel> <journalordner> <depot_dienst-programm> [runden]

Messung je Runde (frischer Prozess, ohne Browser):
  bereit_ms  Start bis Konsolenzeile „[Dienst] Bereit.“
  status_ms  Start bis /teil/status mit data-bereit="ja"
  journal_ms Start bis die Live-Seite „Journal: Eingerichtet und geprüft“ zeigt
  danach je 5 Abrufe von „/“ und „/depot“ im laufenden Dienst.
Gegenproben: eine alte Journalzeile in einer Kopie ändern (a) vor dem Start, (b) im laufenden
Dienst. Beide Male muss die nächste Vollprüfung den Eingriff melden.
"""

import shutil
import subprocess
import sys
import threading
import time
import urllib.request
from pathlib import Path

ADRESSE = "http://127.0.0.1:47410"
HINWEISE = ("beschädigt", "weicht vom bestätigten Stand ab", "Prüfsumme stimmt nicht")
# Erst wenn das Lagebild das Journal gelesen und geprüft hat, steht dies auf der Live-Seite.
GEPRUEFT = "Eingerichtet und geprüft"


def abruf(pfad, frist=30):
    with urllib.request.urlopen(ADRESSE + pfad, timeout=frist) as antwort:
        return antwort.read().decode("utf-8", "replace")


def ms(seit):
    return round((time.perf_counter() - seit) * 1000)


class Dienst:
    def __init__(self, wurzel, programm):
        self.zeilen = []
        self.bereit = None
        self.start = time.perf_counter()
        self.prozess = subprocess.Popen(
            [programm, "--ohne-browser"],
            cwd=wurzel,
            stdout=subprocess.PIPE,
            stderr=subprocess.STDOUT,
            text=True,
            encoding="utf-8",
            errors="replace",
        )
        threading.Thread(target=self._lies, daemon=True).start()

    def _lies(self):
        for zeile in self.prozess.stdout:
            zeile = zeile.rstrip()
            self.zeilen.append(f"{ms(self.start):>7} ms  {zeile}")
            if self.bereit is None and zeile.startswith("[Dienst] Bereit."):
                self.bereit = ms(self.start)

    def warte_status(self, frist=300):
        while time.perf_counter() - self.start < frist:
            if self.prozess.poll() is not None:
                raise RuntimeError("Dienst beendet:\n" + "\n".join(self.zeilen))
            try:
                if 'data-bereit="ja"' in abruf("/teil/status", 5):
                    return ms(self.start)
            except OSError:
                pass
            time.sleep(0.05)
        raise RuntimeError("kein bereit nach %s s" % frist)

    def ende(self):
        self.prozess.terminate()
        try:
            self.prozess.wait(20)
        except subprocess.TimeoutExpired:
            self.prozess.kill()
            self.prozess.wait(20)


def lege_journal(quelle, daten, aendern=False):
    if daten.exists():
        shutil.rmtree(daten)
    daten.mkdir(parents=True)
    for name in ("journal.txt", "journal.vermerk"):
        shutil.copyfile(quelle / name, daten / name)
    if aendern:
        aendere_alte_zeile(daten / "journal.txt")


def aendere_alte_zeile(pfad):
    """Ändert in Zeile 200 ein Zeichen, Länge gleich: die Kette ab dort stimmt nicht mehr."""
    zeilen = pfad.read_bytes().split(b"\n")
    nr = 199
    alt = zeilen[nr]
    if b"Erfundene" not in alt:
        nr = next(i for i, z in enumerate(zeilen) if b"Erfundene" in z and i > 100)
        alt = zeilen[nr]
    zeilen[nr] = alt.replace(b"Erfundene", b"Erfundenx", 1)
    pfad.write_bytes(b"\n".join(zeilen))
    print(f"[Gegenprobe] Zeile {nr + 1} geändert: {alt[:90]!r} …")


def treffer(seite):
    return [h for h in HINWEISE if h in seite]


def runde(wurzel, programm, nr):
    d = Dienst(wurzel, programm)
    try:
        status = d.warte_status()
        while True:
            seite = abruf("/", 120)
            if treffer(seite):
                raise RuntimeError(f"Messjournal gilt als beschädigt: {treffer(seite)}")
            if GEPRUEFT in seite:
                journal_ms = ms(d.start)
                break
            if time.perf_counter() - d.start > 600:
                raise RuntimeError("Journal nach 600 s nicht geprüft")
            time.sleep(0.05)
        live = []
        depot = []
        for _ in range(5):
            t = time.perf_counter()
            abruf("/", 120)
            live.append(ms(t))
            t = time.perf_counter()
            abruf("/depot", 120)
            depot.append(ms(t))
        print(
            f"[Messung] Runde {nr}: bereit_ms={d.bereit} status_ms={status} "
            f"journal_ms={journal_ms} live_ms={live} depot_ms={depot}"
        )
    finally:
        d.ende()
        print("\n".join(f"  {z}" for z in d.zeilen[:20]))


def gegenprobe_vor_start(wurzel, programm, quelle):
    lege_journal(quelle, wurzel / "daten", aendern=True)
    d = Dienst(wurzel, programm)
    try:
        d.warte_status()
        gefunden = []
        while time.perf_counter() - d.start < 600:
            seite = abruf("/", 120)
            gefunden = treffer(seite)
            if gefunden or GEPRUEFT in seite:
                break
            time.sleep(0.05)
        print(
            f"[Gegenprobe a] vor dem Start geändert: erkannt={bool(gefunden)} "
            f"nach {ms(d.start)} ms ab Start {gefunden}"
        )
        zeige_meldung(seite)
        return bool(gefunden)
    finally:
        d.ende()


def gegenprobe_im_lauf(wurzel, programm, quelle, frist=600):
    daten = wurzel / "daten"
    lege_journal(quelle, daten)
    d = Dienst(wurzel, programm)
    try:
        d.warte_status()
        while GEPRUEFT not in abruf("/", 120):
            if time.perf_counter() - d.start > 600:
                raise RuntimeError("Journal vor dem Eingriff nicht geprüft")
            time.sleep(0.05)
        aendere_alte_zeile(daten / "journal.txt")
        t = time.perf_counter()
        while time.perf_counter() - t < frist:
            seite = abruf("/", 120)
            gefunden = treffer(seite)
            if gefunden:
                print(f"[Gegenprobe b] im Lauf geändert: erkannt nach {ms(t)} ms {gefunden}")
                zeige_meldung(seite)
                return True
            time.sleep(1)
        print(f"[Gegenprobe b] im Lauf geändert: NICHT erkannt binnen {frist} s")
        return False
    finally:
        d.ende()


def zeige_meldung(seite):
    for h in HINWEISE:
        i = seite.find(h)
        if i >= 0:
            print("  Meldung: …" + " ".join(seite[max(0, i - 160) : i + 160].split()) + "…")
            return


def main():
    wurzel, quelle, programm = Path(sys.argv[1]), Path(sys.argv[2]), sys.argv[3]
    runden = int(sys.argv[4]) if len(sys.argv) > 4 else 3
    if (wurzel / "daten").exists():
        # Nie neben echten Daten messen: Das Skript ersetzt daten/ vollständig.
        sys.exit(f"[Messung] Abbruch: {wurzel / 'daten'} existiert. Nur in einem frischen Checkout messen.")
    groesse = (quelle / "journal.txt").stat().st_size
    zeilen = (quelle / "journal.txt").read_bytes().count(b"\n")
    print(f"[Messung] Journal: {groesse} Byte, {zeilen} Zeilen")
    lege_journal(quelle, wurzel / "daten")
    for nr in range(1, runden + 1):
        runde(wurzel, programm, nr)
    ok_a = gegenprobe_vor_start(wurzel, programm, quelle)
    ok_b = gegenprobe_im_lauf(wurzel, programm, quelle)
    shutil.rmtree(wurzel / "daten", ignore_errors=True)
    if not (ok_a and ok_b):
        sys.exit(1)


if __name__ == "__main__":
    main()
