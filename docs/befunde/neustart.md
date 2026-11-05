# Nach einem Neustart: geprüften Stand ohne Cargo starten

Maus' zweiter von drei Punkten am 05.11.2026. Erster Teil: Nach einem PC-Neustart soll der letzte geprüfte Programmstand direkt starten, ohne dass Cargo beim Start gebraucht wird. Der zweite Teil, einen noch nicht begonnenen Papiertag nachholen und zeigen, wo der Dienst stehen blieb, steht in `nachholen.md`.

## Befund vorher

- `DepotSchlopf.cmd` (auch über den Autostart) brauchte immer Cargo: Der Wächter lief über `cargo run` und baute den Dienst mit `cargo build`. Jeder Auftrag (Kurse, Termine, Papiertag) lief über `cargo run -p depot_lauf`, und das startete noch einmal `cargo run -p depot_sammler` oder `-p depot_berater`.
- Nach einem Windows-Neustart hing der Start damit an der Werkzeugkette und am Bauen. Fehlte Cargo im Pfad oder scheiterte der Bau, lief nichts.

## Regel

**Geprüft** heißt: Genau dieser Git-Baum hat das ganze Gate bestanden. `run.cmd` vermerkt den Baum des Index nach „Fertig“, `run.cmd update` den Baum des neuen Stands nach dem Gate im Prüfordner. Beides landet als Zeile `geprueft-v1;<baum>` in `daten/geprueft.txt`.

**Sichern** (`depot_dienst::programmstand::sichere`): Der Wächter nennt dem mit Cargo gebauten Dienst den Baum, aus dem gebaut wurde (`--baum`). Läuft der Dienst 17 Sekunden und ist kein Update mehr auf Probe, kopiert er die Programme `depot_waechter`, `depot_dienst`, `depot_sammler` und `depot_berater` nach `daten/programm/<baum>/`. In `stand.txt` steht für jede Datei ein SHA-256-Abdruck. Danach zeigt `daten/programm/starte.cmd` auf den neuen Wächter. Gesichert wird nur, wenn

- der Baum in `daten/geprueft.txt` steht,
- der Projektordner sauber ist (`git status` meldet nichts) und
- er noch auf demselben Baum steht wie beim Bau.

Der neue und der vorige Stand bleiben, ältere werden gelöscht, soweit sie nicht in Gebrauch sind.

**Starten:** `DepotSchlopf.cmd` ruft `daten\programm\starte.cmd`, wenn es sie gibt, sonst wie bisher Cargo. Der Wächter des Stands nimmt den gesicherten Stand nur, wenn er `startklar` ist:

| Bedingung | sonst |
|---|---|
| kein Update auf Probe (`daten/update_offen.txt`) | mit Cargo bauen, damit der Rückfall greifen kann |
| jede Programmdatei hat ihren Abdruck | mit Cargo bauen |
| Projektordner sauber und auf genau dem gesicherten Baum | mit Cargo bauen |

Aus dem Stand laufen Kurse, Termine, Papiertag und das Einrichten des Journals direkt mit den Programmen des Stands. Nur das Update braucht weiter Cargo, denn es prüft mit dem ganzen Gate. Der Dienst prüft beim Start noch einmal, ob der Stand startklar ist, sonst nimmt er für die Aufträge Cargo.

`starte.cmd` hat Aufruf und Ende in einer Zeile (`"…\depot_waechter.exe" %* & exit /b`). cmd liest eine Befehlsdatei zeilenweise nach. Schreibt der Dienst sie neu, während der Wächter läuft, liest cmd danach nichts Neues mehr.

## Anzeige

In der Einrichtung unter „Update“ steht „Start nach einem Neustart:“ mit einem dieser Texte:

- „Gestartet aus dem geprüften Programmstand … ohne Cargo“
- „Programmstand … gesichert. Der nächste Start, auch nach einem Windows-Neustart, braucht kein Cargo.“
- „Programmstand nicht gesichert: Dieser Stand hat das Gate hier noch nicht bestanden. Einmal run.cmd ausführen oder ein Update einspielen.“

## Nachweis

`crates/depot_dienst/tests/programmstand.rs`:

- `nur_ein_gepruefter_sauberer_stand_wird_gesichert`
- `startklar_nur_wenn_alles_passt`: Update auf Probe, veränderte Programmdatei, neuer Stand im Ordner, Ablösung durch einen neuen geprüften Stand
- `auftraege_starten_die_programme_des_stands`

Für den Test unter Windows: Einmal `run.cmd` (Gate offen), dann `DepotSchlopf.cmd`, mindestens 20 Sekunden laufen lassen. Die Einrichtung meldet „Programmstand … gesichert“. Danach Windows neu starten. Der Autostart startet den Dienst, und die Einrichtung meldet „Gestartet aus dem geprüften Programmstand … ohne Cargo“.
