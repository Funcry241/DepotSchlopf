# DepotSchlopf

[![Kodex](https://github.com/Funcry241/DepotSchlopf/actions/workflows/kodex.yml/badge.svg)](https://github.com/Funcry241/DepotSchlopf/actions/workflows/kodex.yml)
[![Lizenz: MIT](https://img.shields.io/badge/Lizenz-MIT-blue.svg)](LICENSE)
![Rust 1.98](https://img.shields.io/badge/Rust-1.98-orange.svg)
![Status: experimentell](https://img.shields.io/badge/Status-experimentell-red.svg)

Rust-Rahmen, in dem zwei KIs je einen Topf eines echten Aktiendepots beraten: **Dachs** (Claude) und **Maus** (GPT). Die KIs schlagen nur Zielgewichte vor. Ein festes, getestetes Regelwerk entscheidet, was wirklich gehandelt wird. Harte Grenzen gelten für Risiko, Handelskosten und KI-Kosten.

> **Essenz lenkt. KI schlägt vor. Regelwerk entscheidet. Auftragsstelle schreibt. Journal belegt. Abgleich prüft. Kodex wacht.**

> [!WARNING]
> Experimentelles Privatprojekt. **Keine Anlageberatung**, keine Steuerberatung. Totalverlust ist möglich. Siehe [Risiko- und Haftungshinweis](docs/RISIKO.md).

Neu hier? Zuerst [docs/kaltstart.md](docs/kaltstart.md) lesen.

## Was DepotSchlopf garantiert (Standardwerte)

| Gefahr | Was im Code dagegen steht |
|---|---|
| KI erfindet Werte oder verlangt Unmögliches | Vorschlag wird ganz verworfen; nur Werte der Positivliste |
| Zu viel in einem Wert | höchstens 25 % je Wert, höchstens 8 Werte |
| Produkte mit Verlust über dem Einsatz | nur Aktien (ETFs vorbereitet, gesperrt); keine Derivate, kein Hebel, kein Leerverkauf, nur Bargeld |
| Gebühren fressen den Ertrag | Mindestorder 500 €, Umschichtung erst ab 5 Prozentpunkten, höchstens 4 Orders je Topf und Zyklus, Umschlag höchstens 50 % |
| Absturz des Marktes | Tagesverlust 5 % = Halt; 20 % unter Höchststand = Notbremse, alles verkaufen |
| KI-Kosten laufen aus dem Ruder | Erlaubnis erst nach dauerhafter Buchung der Höchstkosten; je KI 2 Aufrufe je Zyklus, 1 Zyklus je Tag, 4 Aufrufe und 4 € je Monat, 50 € je Jahr |
| Grenzen werden heimlich gelockert | Kodex vergleicht jede Änderung mit `kodex/festschreibung.txt` und lehnt ab |
| Neue KI-Sitzung kennt die Vorgeschichte nicht | jeder Lauf schreibt `assistant_export_latest.zip` mit Übergabe und Quellstand |
| Unklare Lage | fail-closed: Halt statt Raten |

## Aufbau

| Crate | Inhalt |
|---|---|
| `depot_kern` | Geld in Cent (geprüfte Rechnung), ISIN mit Prüfziffer, Töpfe, Kurse, Vorschläge, Grenzen mit Fingerabdruck, Positivliste |
| `depot_regelwerk` | Reine Funktion ohne Netz, Dateien oder Uhr: prüft Vorschläge und erzeugt `GenehmigterAuftrag` (nur hier erzeugbar). Dazu Kursprüfung und Wächter |
| `depot_journal` | Das Journal als einzige dauerhafte Datei: Hash-Kette mit Anhangsvermerk, geprüfte Wiederherstellung; Papierdepot, Aufträge mit Zustand, belegte Kurse mit Quelle und Abrufzeit, Beobachtungen und KI-Buchungen. Dazu der Kontowächter: Deckung vor jeder Reservierung, Abgleich mit dem Konto, Halt bei jeder Abweichung |
| `depot_ki` | Regeln des KI-Budgets ohne Datei; das Kassenbuch steht im Journal, eine Erlaubnis gibt es nur nach Buchung der Höchstkosten dort |
| `depot_sim` | Simulierter Lauf mit erfundenen Daten und Papiergeld: Dachs und Maus beobachten, begründen im Journal und entscheiden im Schutzrahmen; ausgeführt wird über ein erfundenes Papierkonto, mit dem der Kontowächter abgleicht; echte Kurse halten den Lauf an |
| `depot_sammler` | Holt Xetra-Schlusskurse und Marktnachrichten lesend von Alpha Vantage und die Termine der EZB von ihrer Seite, legt jede Antwort roh unter `daten/roh/` ab, belegt jeden Kurs, Termin und jede Nachricht im Journal und prüft die Kurse. Mit Netz |
| `depot_berater` | Dachs (Claude Sonnet 5.5) und Maus (GPT-6.1 Terra) als Berater im Papierhandel: fragt nur mit gebuchter Erlaubnis, rechnet nach jedem Aufruf die Kosten nach und sperrt bei jeder Überschreitung. Mit Netz; `run.cmd papier` |
| `depot_lage` | Das Lagebild ohne Netz: Bereitschaft für den nächsten Papiertag mit Ampel und nächstem Schritt, KI-Budget je KI direkt aus dem Kassenbuch, Papierdepot, Aufträge, Tagesergebnis der letzten Handelstage (gelaufen, ohne Handel oder ausgefallen, mit Grund, und welche KI beigetragen hat), Begründungen von Dachs und Maus, festgeschriebene Grenzen. Rechnet mit denselben Prüfungen wie Papiertag und Kostenschutz |
| `depot_dienst` | Der Dienst mit Oberfläche nur auf 127.0.0.1: Live-Ansicht, eine Config-Seite, Zeitplan, Aufträge als eigene Läufe. Kleiner eigener HTTP-Server aus der Standardbibliothek mit Host-, Herkunfts- und Token-Prüfung |
| `depot_lauf` | Der Ein-Befehl-Einstieg `run.cmd` mit Assistenten-Export nach jedem Lauf |
| `depot_kodex` | Das Gate: Deklaration gegen realen Diff, Pfadrollen, Festschreibung, Quellscan, Essenz, Format, Clippy, Tests |

Geplant: breitere Auswahl, `run.cmd tag`, Backtest, Steuer-Export aus dem Journal, Broker-Adapter (Interactive Brokers, Cash-Konto).

## Essenz und Kodex

- [Essenz](docs/essenz.md): Erfahrungsregeln D00–D17. Beratend, vor jeder Änderung lesen.
- Kodex: entscheidet nach jeder Änderung. Jede Änderung deklariert sich in `kodex/aenderung.txt`. Harte Grenzen, KI-Budget, Produktarten, Abhängigkeiten und Lints stehen in `kodex/festschreibung.txt` und ändern sich nur mit Freigabe des Eigentümers.

## Schnellstart

Voraussetzung: [Rust](https://rustup.rs/) (die Version wird über `rust-toolchain.toml` automatisch gewählt) und Git.

```
git clone https://github.com/Funcry241/DepotSchlopf.git
cd DepotSchlopf
run.cmd
```

Ein Befehl macht alles: Gate prüfen (Struktur, Festschreibung, Quellscan, Essenz, Format, Clippy, Tests) und danach `assistant_export_latest.zip` schreiben. Diese Datei kann man einer neuen KI-Sitzung geben. Unter Linux oder macOS: `cargo run -p depot_lauf`.

Gate und Export nehmen genau denselben Stand: was mit `git add` vorgemerkt ist. Der Lauf prüft ihn in einer eigenen Kopie außerhalb des Projektordners, ohne dessen lokale Cargo-Einstellungen. Nicht vorgemerkte Änderungen werden weder geprüft noch exportiert; die Konsole nennt sie als Hinweis.

### Echte Schlusskurse (nur lesend)

Einmal den kostenlosen Schlüssel von Alpha Vantage in der Windows-Anmeldeinformationsverwaltung ablegen; `cmdkey` fragt ihn verdeckt ab:

```
cmdkey /generic:DepotSchlopf/alphavantage /user:depotschlopf /pass
run.cmd kurse einrichten
```

Danach holt `run.cmd kurse` die Schlusskurse der Beobachtungsliste (ein Abruf je Wert, 15 Sekunden Abstand), belegt sie in `daten/journal.txt` und zeigt das Ergebnis der Kursprüfung. Bei einem Gesamtabruf oder einem Xetra-Abruf holt er nach den Kursen mit einem weiteren Abruf die neuesten Marktnachrichten und belegt höchstens 15 davon; `run.cmd nachrichten` holt nur sie, etwa nach einem Fehler. Bei fehlenden Nachrichten können verwendbare Kurse weiter genutzt werden. Gibt eine Antwort einen Schlüssel zurück, greift die Kurssperre. Es entstehen keine Aufträge und keine KI-Anfragen. Der Schlüssel steht nie im Journal, in einer Meldung oder im Export.

### Termine der EZB (nur lesend)

`run.cmd termine` holt die Sitzungstermine des EZB-Rats von der Seite der EZB, legt die Seite roh unter `daten/roh/ezb/` ab und belegt jeden Termin mit Quelle und Abrufzeit in `daten/journal.txt`. Ein Schlüssel ist nicht nötig, nur das Journal (`run.cmd kurse einrichten`). Ein unveränderter Termin wird nicht noch einmal geschrieben, ein geänderter fortgeschrieben.

### Papierhandel mit Dachs und Maus

`run.cmd papier` führt nach `run.cmd kurse` einen Papiertag mit den heute belegten Schlusskursen: Dachs und Maus schlagen vor und prüfen sich gegenseitig, dann Kontowächter, Regelwerk und Papierkonto `daten/papierkonto.txt`, nur Papiergeld, einmal je Handelstag. Ohne Kurse von heute hält er an. Pflicht sind nur die Kurse der gehaltenen Werte: Ein Kandidat ohne gültigen Kurs ist an dem Tag nicht handelbar und steht so im Lagebild. Jeder Wert gehört zu einer Börse, deren Schluss und Feiertage gelten (Xetra, NYSE). An einem Tag, an dem keine Börse der Liste handelt, läuft nichts, und nichts ist verpasst. Die Schlüssel legt Wolfgang einmal in der Windows-Anmeldeinformationsverwaltung an:

```text
cmdkey /generic:DepotSchlopf/anthropic /user:depotschlopf /pass
cmdkey /generic:DepotSchlopf/openai /user:depotschlopf /pass
```

Fehlt ein Schlüssel, wird dieser Berater nicht gefragt. Die KI-Kosten sind auf 20 € im Monat je KI begrenzt. Überschreitet ein Aufruf seine Reservierung, liegt danach `daten/ki.halt` mit dem Grund, und keine KI wird mehr gefragt, bis sie nach der Prüfung aufgehoben ist (Einrichtung, Abschnitt „Sperre des Kostenschutzes“). Das Aufheben ändert nichts am Kassenbuch; jeder weitere Aufruf durchläuft wieder die Kostenprüfung.

`run.cmd lage` schreibt das Lagebild nach `daten/lage.html` (im Browser öffnen): was für den nächsten Papiertag bereit ist oder fehlt, KI-Budget, Papierdepot, Begründungen und Grenzen. Es liest nur.

### Oberfläche (Dienst auf 127.0.0.1)

Doppelklick auf `DepotSchlopf.cmd` startet den Dienst und öffnet http://127.0.0.1:47410/. Läuft er schon, öffnet sich nur die Seite. Der Dienst lauscht nur auf diesem Rechner und schreibt selbst nie ins Journal; jede Arbeit ist ein eigener Lauf wie `run.cmd kurse`.

- **Gesamtstatus** oben auf jeder Seite: vollständig bereit, läuft ohne KI, pausiert oder Eingriff nötig, jeweils mit Gründen (Schlüssel abgelehnt, Anbieterlimit mit nächstem Versuch, fehlendes Guthaben, Budget, Zeitplan aus).
- **Live:** was gerade läuft (mit Protokoll), was wartet, der nächste geplante Lauf, dazu das Lagebild mit KI-Budget je KI, Papierdepot und dem Gespräch von Dachs und Maus. Die Seite frischt sich selbst auf und zeigt, wenn der Dienst nicht antwortet. Nach einem Neustart steht unter Tätigkeit, was vorher lief und ob ein Lauf unterbrochen wurde. Doppelklicks und erneutes Absenden starten nichts doppelt. Das **Tagesergebnis** zeigt je Handelstag, ob der Papiertag lief, ohne Handel blieb (etwa weil die Kurse zu alt waren) oder ausfiel, und ob Dachs und Maus tatsächlich beigetragen haben oder warum nicht. Alle Ergebnisse im Papierdepot sind simuliert: ausgeführt zum Schlusskurs des Entscheidungstags.
- **Einrichtung:** die eine Config-Seite. Schlüssel gehen direkt in die Windows-Anmeldeinformationsverwaltung; die Seite zeigt keinen Schlüssel, nur seinen Stand (fehlt, hinterlegt, geprüft, abgelehnt) nach dem letzten echten Aufruf (`daten/anbieter.txt`). Verbindungstest der Quellen, Zeitplan (Mo–Fr, Termine 17:40, Kurse 17:45, danach Papiertag; jeder Lauf höchstens einmal am Tag; **ab Werk aus**, er läuft erst, wenn er hier eingeschaltet und gespeichert ist, und einschalten lässt er sich erst, wenn jeder Schlüssel hinterlegt und kein Eingriff mehr nötig ist; abgelehnte Eingaben bleiben im Formular), Autostart mit Windows, geführte Aufhebung der Sperre des Kostenschutzes. Grenzen und Budgets stehen nur zur Ansicht; sie ändern sich nur über eine Grenzfreigabe.

- **Update:** „Nach Update suchen“ nennt die neuen Stände auf `main`. „Update einspielen“ prüft den neuen Stand zuerst mit dem ganzen Gate in einem eigenen Ordner und rückt den Projektordner nur dann vorwärts, ohne Lokales zu überschreiben. Danach startet der Dienst neu und die Seite lädt sich selbst neu; kommt der neue Stand nicht hoch, setzt `DepotSchlopf.cmd` den alten zurück. Dasselbe ohne Oberfläche: `run.cmd update pruefen` und `run.cmd update`.

Schutz gegen fremde Webseiten: Jede Anfrage braucht `Host` 127.0.0.1 oder localhost mit dem eigenen Port, jede Änderung zusätzlich die eigene Herkunft und den Zufallswert dieses Dienststarts. Antworten verbieten Einbetten und fremde Skripte.

## Mitmachen

[Beitragsregeln](docs/CONTRIBUTING.md) · [Sicherheit](docs/SECURITY.md) · [Umgang miteinander](docs/CODE_OF_CONDUCT.md) · [Änderungen](CHANGELOG.md)

## Lizenz

[MIT](LICENSE) © 2026 Funcry241
