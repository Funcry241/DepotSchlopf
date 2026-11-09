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

## Schutzregeln im derzeitigen Papierbetrieb (Standardwerte)

| Gefahr | Was im Code dagegen steht |
|---|---|
| KI erfindet Werte oder verlangt Unmögliches | Vorschlag wird ganz verworfen; nur Werte der Positivliste |
| Zu viel in einem Wert | höchstens 40 % je Wert, höchstens 4 Werte |
| Produkte mit Verlust über dem Einsatz | Aktien und freigegebene physische UCITS-ETFs; keine Derivate, kein Hebel, kein Leerverkauf, nur Bargeld |
| Gebühren fressen den Ertrag | Mindestorder 100 € im Papierhandel, Umschichtung erst ab 5 Prozentpunkten, höchstens 4 Orders je Topf und Zyklus, Umschlag höchstens 100 % |
| Absturz des Marktes | Tagesverlust 5 % = Halt; 20 % unter Höchststand = Notbremse, alles verkaufen; Papier-Gesamtverlustsperre bei 30 % unter dem historischen Höchststand, weitere Freigabe nur ausdrücklich in der Einrichtung |
| KI-Kosten laufen aus dem Ruder | Erlaubnis erst nach dauerhafter Buchung der Höchstkosten; je KI höchstens 30 Aufrufe je Tag und 690 im Monat; unverändert 20 € je Monat und 240 € je Jahr, mit Tagesrahmen und bestätigter Tokenabrechnung |
| Grenzen werden heimlich gelockert | Kodex vergleicht jede Änderung mit `kodex/festschreibung.txt` und lehnt ab |
| Neue KI-Sitzung kennt die Vorgeschichte nicht | jeder Lauf schreibt `assistant_export_latest.zip` mit Übergabe und Quellstand |
| Unklare Lage | fail-closed: Halt statt Raten |

Die aktuellen Grenzen kommen technisch aus einem gemeinsamen Paket (`Grenzen::standard()`), einschließlich 100 € Mindestorder. Es gibt noch kein getrenntes Echtgeld-Grenzpaket. Vor einer Echtgeld-Anbindung müssen dessen Grenzen getrennt festgelegt und ausdrücklich freigegeben werden. Die Freigabe der 100-€-Mindestorder aktiviert keinen Echtgeldhandel.

Bei den derzeit angesetzten Mindestkosten von 1,50 € je Order entsprechen 100 € Ordervolumen 1,5 % Kosten je Kauf bzw. Verkauf, zusammen etwa 3 % bei gleichem Volumen. Das ist eine Kostenannahme des Papierkontos, keine Zusage eines Brokertarifs.

## Aufbau

| Crate | Inhalt |
|---|---|
| `depot_kern` | Geld in Cent (geprüfte Rechnung), ISIN mit Prüfziffer, Töpfe, Kurse, Vorschläge, Grenzen mit Fingerabdruck, Positivliste |
| `depot_regelwerk` | Reine Funktion ohne Netz, Dateien oder Uhr: prüft Vorschläge und erzeugt `GenehmigterAuftrag` (nur hier erzeugbar). Dazu Kursprüfung und Wächter |
| `depot_journal` | Das Journal als einzige dauerhafte Datei: Hash-Kette mit Anhangsvermerk, geprüfte Wiederherstellung; Papierdepot, Aufträge mit Zustand, belegte Kurse mit Quelle und Abrufzeit, Beobachtungen und KI-Buchungen. Dazu der Kontowächter: Deckung vor jeder Reservierung, Abgleich mit dem Konto, Halt bei jeder Abweichung |
| `depot_ki` | Regeln des KI-Budgets ohne Datei; das Kassenbuch steht im Journal, eine Erlaubnis gibt es nur nach Buchung der Höchstkosten dort |
| `depot_sim` | Simulierter Lauf mit erfundenen Daten und Papiergeld: Dachs und Maus beobachten, begründen im Journal und entscheiden im Schutzrahmen; ausgeführt wird über ein erfundenes Papierkonto, mit dem der Kontowächter abgleicht; echte Kurse halten den Lauf an |
| `depot_sammler` | Holt Schlusskurse von Alpha Vantage/Tiingo und kostenlose RSS-Schlagzeilen von tagesschau/BBC World und die Termine der EZB von ihrer Seite, legt jede Antwort roh unter `daten/roh/` ab, belegt jeden Kurs, Termin und jede Nachricht im Journal und prüft die Kurse. Mit Netz |
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

Danach holt `run.cmd kurse` die Schlusskurse der Beobachtungsliste (ein Abruf je Wert, 15 Sekunden Abstand), belegt sie in `daten/journal.txt` und zeigt das Ergebnis der Kursprüfung. Nach einem Kursabruf holt er kostenlose RSS-Schlagzeilen von tagesschau und BBC World, ohne Nachrichtenschlüssel oder Premiumzugang, und belegt insgesamt höchstens 15 Titel mit Link und Veröffentlichungszeit. Je Quelle gilt eine dauerhafte Abrufpause von mindestens 90 Minuten. RSS-Abrufvermerke sind Verwaltung und beginnen keinen Papiertag; frühere solche Vermerke werden ohne Änderung deiner Dateien erkannt. `run.cmd nachrichten` holt nur sie; in der Einrichtung gibt es dafür einen eigenen Knopf „Jetzt prüfen“. Zusätzliche Abrufe tagsüber lassen sich dort einschalten (empfohlen für das Weltmodell): bei aktivem Zeitplan täglich von 08 bis vor 23 Uhr, frühestens alle 90 Minuten, auch am Wochenende. Ist auch die automatische Vorbereitung eingeschaltet, kann ein erfolgreicher Schlagzeilenabruf an einem börsenfreien Tag KI-Budget für die Vorbereitung nutzen. Der Abruf und die Vorbereitung erzeugen keine Orders. Alte Zeitpläne behalten diese zusätzliche Option ausgeschaltet. Artikelseiten werden nicht abgerufen. Bei fehlenden Nachrichten können verwendbare Kurse weiter genutzt werden. Gibt eine Antwort einen Schlüssel zurück, greift die Kurssperre. Es entstehen keine Aufträge und keine KI-Anfragen. Der Schlüssel steht nie im Journal, in einer Meldung oder im Export.

**Kurssperre untersuchen:** In der Einrichtung erscheint bei einer Sperre der Knopf „Kurssperre sicher untersuchen“ (alternativ `run.cmd kurse diagnose`). Er fragt ausschließlich die bereits freigegebenen Kursquellen ab: je Wert ein Versuch, 15 Sekunden Abstand, bis zu 15 Abrufe. Das verbraucht Quellenkontingent und kann mehrere Minuten dauern. Das kopierbare Protokoll unter Live → Tätigkeit nennt nur Quelle, erkannten gespeicherten Zugang und Antwortart; weder Schlüssel noch Antwortinhalte. Es werden keine Rohantworten oder Kurse gespeichert, kein Journal geöffnet und keine KI gefragt. Die Kurssperre bleibt bestehen, auch wenn der Fehler gerade nicht erneut auftritt. Ohne Sperre wird nichts abgerufen. Diese Diagnose beweist keine Ursache und keinen Zugriff durch Dritte. Neue Sicherheitsabbrüche schreiben dieselben geheimnisfreien Metadaten auch in den Sperrgrund.


### Termine der EZB (nur lesend)

`run.cmd termine` holt die Sitzungstermine des EZB-Rats von der Seite der EZB, legt die Seite roh unter `daten/roh/ezb/` ab und belegt jeden Termin mit Quelle und Abrufzeit in `daten/journal.txt`. Ein Schlüssel ist nicht nötig, nur das Journal (`run.cmd kurse einrichten`). Ein unveränderter Termin wird nicht noch einmal geschrieben, ein geänderter fortgeschrieben.

### Papierhandel mit Dachs und Maus

`run.cmd papier` führt nach `run.cmd kurse` einen Papiertag mit den heute belegten Schlusskursen: Dachs und Maus schlagen vor und prüfen sich gegenseitig, dann Kontowächter, Regelwerk und Papierkonto `daten/papierkonto.txt`, nur Papiergeld, einmal je Handelstag. Ohne Kurse von heute hält er an. Pflicht sind nur die Kurse der gehaltenen Werte: Ein Kandidat ohne gültigen Kurs ist an dem Tag nicht handelbar und steht so im Lagebild. Jeder Wert gehört zu einer Börse, deren Schluss und Feiertage gelten (Xetra, NYSE). An einem Tag, an dem keine Börse der Liste handelt, läuft nichts, und nichts ist verpasst. Die Schlüssel legt Wolfgang einmal in der Windows-Anmeldeinformationsverwaltung an:

```text
cmdkey /generic:DepotSchlopf/anthropic /user:depotschlopf /pass
cmdkey /generic:DepotSchlopf/openai /user:depotschlopf /pass
```

Fehlt ein Schlüssel, wird dieser Berater nicht gefragt. Die KI-Kosten sind auf 20 € im Monat je KI begrenzt. Vor jedem Aufruf werden die Höchstkosten dauerhaft reserviert. Plausible Tokenangaben rechnen diesen Aufruf danach zu den festgeschriebenen konservativen Preisen ab, auf volle Cent aufgerundet. Ohne eindeutige Abrechnung bleibt die volle Reservierung gebucht. Die Live-Seite zeigt Monatsrest, freien Tagesrahmen, verbleibende Handelstage und heutige Aufrufe. Überschreitet ein Aufruf seine Reservierung, liegt danach `daten/ki.halt` mit dem Grund, und keine KI wird mehr gefragt, bis sie nach der Prüfung aufgehoben ist (Einrichtung, Abschnitt „Sperre des Kostenschutzes“). Das Aufheben ändert nichts am Kassenbuch; jeder weitere Aufruf durchläuft wieder die Kostenprüfung.

`run.cmd lage` schreibt das Lagebild nach `daten/lage.html` (im Browser öffnen): was für den nächsten Papiertag bereit ist oder fehlt, KI-Budget, Papierdepot, Begründungen und Grenzen. Es liest nur.

### Oberfläche (Dienst auf 127.0.0.1)

Doppelklick auf `DepotSchlopf.cmd` startet den Dienst und öffnet http://127.0.0.1:47410/. Läuft er schon, öffnet sich nur die Seite. Der Dienst lauscht nur auf diesem Rechner und schreibt selbst nie ins Journal; jede Arbeit ist ein eigener Lauf wie `run.cmd kurse`.

- **Gesamtstatus** oben auf jeder Seite: vollständig bereit, KI eingeschränkt, pausiert oder Eingriff nötig, jeweils mit Gründen und getrenntem Zeitplanstatus (Schlüssel abgelehnt, Anbieterlimit mit nächstem Versuch, fehlendes Guthaben, Budget, Zeitplan aus).
- **Live:** was gerade läuft (mit Protokoll), was wartet, der nächste geplante Lauf, dazu das Lagebild mit KI-Budget je KI, Papierdepot und dem Gespräch von Dachs und Maus. Die Seite frischt sich selbst auf und zeigt, wenn der Dienst nicht antwortet. Nach einem Neustart steht unter Tätigkeit, was vorher lief und ob ein Lauf unterbrochen wurde. Doppelklicks und erneutes Absenden starten nichts doppelt. Das **Tagesergebnis** zeigt je Handelstag, ob der Papiertag lief, ohne Handel blieb (etwa weil die Kurse zu alt waren) oder ausfiel, und ob Dachs und Maus tatsächlich beigetragen haben oder warum nicht. Alle Ergebnisse im Papierdepot sind simuliert: ausgeführt zum Schlusskurs des Entscheidungstags.
- **Einrichtung:** die eine Config-Seite. Schlüssel gehen direkt in die Windows-Anmeldeinformationsverwaltung; die Seite zeigt keinen Schlüssel, nur seinen Stand (fehlt, hinterlegt, geprüft, abgelehnt) nach dem letzten echten Aufruf (`daten/anbieter.txt`). Verbindungstest der Quellen, Zeitplan (Mo–Fr, Termine 17:40, Kurse 17:45, danach Papiertag; jeder Lauf höchstens einmal am Tag; **ab Werk aus**, er läuft erst, wenn er hier eingeschaltet und gespeichert ist, und einschalten lässt er sich erst, wenn jeder Schlüssel hinterlegt und kein Eingriff mehr nötig ist; abgelehnte Eingaben bleiben im Formular), Autostart mit Windows, geführte Aufhebung der Sperre des Kostenschutzes. Grenzen und Budgets stehen nur zur Ansicht; sie ändern sich nur über eine Grenzfreigabe.
- **Vorbereitung:** Auf Live zeigen Dachs und Maus getrennte Einschätzungen für den nächsten Handelstag, einschließlich Beleg, Wirkung und Gegenargument. Unter Einrichtung → Zeitplan zuerst Schlagzeilen und dann „An börsenfreien Tagen … vorbereiten“ einschalten und speichern; der Zeitplan muss an sein. Die neue Option ist ab Werk aus. Nach einem erfolgreichen Schlagzeilenabruf folgt an börsenfreien Tagen höchstens ein KI-Aufruf je Berater und Tag zwischen 08 und 23 Uhr. Er zählt zum vorhandenen Budget und erzeugt keine Orders. An Handelstagen prüfen die regulären KI-Aufrufe die bisherigen Thesen mit den neuen Daten. Vorbereitung und Abstimmung nutzen einen kleineren Ausgabedeckel und dürfen die Reserve der nächsten Erstmeinung nicht verbrauchen. Manuell lässt sich die Vorbereitung mit Rückfrage auf Live oder per `run.cmd vorbereiten` starten.


- **Diagnoseprotokolle:** Unter Tätigkeit lässt sich je Lauf das gespeicherte Protokoll öffnen, kopieren oder herunterladen, auch nach einem Neustart. Es enthält Zeitstempel und die gestartete Version. In `daten/protokolle/` bleiben höchstens 20 Dateien mit je bis zu 16 MiB; Kürzungen und ausgelassene überlange Zeilen sind gekennzeichnet. Die Live-Vorschau zeigt die letzten 500 Zeilen. Zugangsdaten in Adressen, Headern und Fehlermeldungen werden an der Quelle durch `***` ersetzt. Eine zusätzliche Prüfsummenkontrolle entfernt übersehene Schlüsselzeilen und meldet den Fehler in der Oberfläche; bei nicht verfügbarem Schlüsselschutz wird die Ausgabe unterdrückt. Das Geschäfts- und Kostenjournal bleibt die maßgebliche Aufzeichnung.
- **Update:** „Nach Update suchen“ nennt die neuen Stände auf `main`. „Update einspielen“ prüft den neuen Stand zuerst mit dem ganzen Gate in einem eigenen Ordner und rückt den Projektordner nur dann vorwärts, ohne Lokales zu überschreiben. Danach startet der Dienst neu und die Seite lädt sich selbst neu; kommt der neue Stand nicht hoch, setzt `DepotSchlopf.cmd` den alten zurück. Dasselbe ohne Oberfläche: `run.cmd update pruefen` und `run.cmd update`.

Schutz gegen fremde Webseiten: Jede Anfrage braucht `Host` 127.0.0.1 oder localhost mit dem eigenen Port, jede Änderung zusätzlich die eigene Herkunft und den Zufallswert dieses Dienststarts. Antworten verbieten Einbetten und fremde Skripte.

## Mitmachen

[Beitragsregeln](docs/CONTRIBUTING.md) · [Sicherheit](docs/SECURITY.md) · [Umgang miteinander](docs/CODE_OF_CONDUCT.md) · [Änderungen](CHANGELOG.md)

## Lizenz

[MIT](LICENSE) © 2026 Funcry241

### Update und Browser-Wiederverbindung

Die Oberfläche unterscheidet Einspielen, Neustart zur Probe, bestätigtes Update und Rückfall. Erst nach der Prüfung des neuen Dienstes und der Papierdaten gilt das Update als bestätigt. Höchstens 20 Abschlussbelege mit insgesamt höchstens 128 KiB bleiben unter `daten/update_ergebnisse.txt` erhalten; der vorhandene Verlauf und das kopierbare Protokoll bekommen ebenfalls den endgültigen Ausgang. Bei einem Rückfall stehen Version und Grund in der Einrichtung.

Alle Seiten prüfen die Verbindung, auch Bestätigung und Protokoll. Hängende HTTP-Anfragen und Antwortkörper werden nach fünf Sekunden abgebrochen und erneut versucht. Nach einem Dienstwechsel wartet die Seite auf dessen Bestätigung und lädt genau einmal neu. Die Position `#update` sowie notwendige Protokollparameter bleiben erhalten. Vorläufige Meldungen verschwinden. Eine Browser-Seite aus dem Zurück-Speicher nimmt die Verbindungsprüfung wieder auf.

Der Wächter baut in getrennten, wiederverwendbaren Bauordnern und überschreibt seine laufende Programmdatei nicht. Beim ersten Einspielen dieser Korrektur läuft noch der alte Wächter: Scheitert schon dieser Übergang an einer gesperrten EXE, muss einmal der alte Dienst beendet und im Projektordner `run.cmd update` ausgeführt werden (mit Gate, Datenprobe und Sicherung), danach `DepotSchlopf.cmd` gestartet werden. Der neue Browser-Code kann ebenfalls erst nach dem Laden der neuen Version wirken. Datenprüfungen und Rückfallschutz bleiben bestehen.

Ein weiterer Doppelklick während Bau, Update oder Rückfall übernimmt den laufenden Start nicht. Eine Betriebssystem-Sperre schützt den Wächter für seine gesamte Laufzeit, auch gegenüber einer anderen Projektkopie desselben Benutzers. Nach Prozessende oder Absturz gibt das Betriebssystem sie frei; die leere Sperrdatei im temporären Ordner bleibt liegen. Nach dem ersten Update mit dieser Korrektur den Dienst einmal über `DepotSchlopf.cmd` neu starten, damit auch der bislang laufende Wächter ersetzt wird.

Dachs und Maus können nach unabhängig festgehaltenen Erstmeinungen und den Zweitmeinungen ihre Vorschläge abstimmen. Jede Runde sieht denselben vorher fixierten Stand; eine Änderung braucht eine nachfolgende ausdrückliche Zustimmung zu genau diesem Vorschlag. Bei unklarer Antwort endet die Abstimmung, bei Einigkeit ebenfalls. Der letzte gültige und geprüfte Stand geht an das Regelwerk. Jede Runde, Änderung und Begründung steht getrennt im Journal. Näheres: [KI-Abrechnung und Abstimmung](docs/befunde/ki-dialog.md).

Die Oberfläche und neue Protokollmeldungen verwenden echte Umlaute. Alte Journaltexte und technische Kennungen bleiben unverändert; du musst keine Daten konvertieren.

### Was gerade läuft und ob ein Update bereitliegt

Oben auf **Live** und **Einrichtung** zeigt die Betriebsleiste den laufenden Auftrag und seine Wartehinweise. „Fortschritt ansehen“ führt zum Protokoll. Der Balken zeigt Tätigkeit, keinen geschätzten Prozentwert. Nach dem Abschluss steht dort das Ergebnis; ein wartender Auftrag erscheint beim nächsten Start.

Nach **Nach Update suchen** bleibt ein gefundenes Update auf beiden Seiten als **Update verfügbar** sichtbar. **Update ansehen** führt zu den Änderungen in der Einrichtung. Einspielen braucht weiterhin deine Bestätigung für genau die gefundene Version. Es gibt keine automatische Installation. Die Leiste wird getrennt von deinen Eingabefeldern aktualisiert.

### Änderungen speichern und in Ruhe einrichten

Zeitplanänderungen gelten nach dem Speichern. Ist der Zeitplan aus, übernimmt **Speichern und Zeitplan einschalten** die ausgefüllten Werte und aktiviert die automatischen Läufe gemeinsam. **Nur speichern · Zeitplan bleibt aus** richtet alles für später ein. Beim Ausschalten entfernt DepotSchlopf wartende automatische Aufträge. Laufende Aufträge laufen zu Ende, und manuelle Aufträge bleiben in der Warteschlange. Fehlt eine Voraussetzung, steht der Grund beim Zeitplan; dann lassen sich die Einstellungen zunächst nur speichern.

Beim Bearbeiten zeigt das Formular **Noch nicht gespeichert** und hebt die Speicheraktion hervor. Beim Verlassen warnt der Browser; beim Absenden eines anderen Formulars fragt die Seite nach, bevor sie offene Änderungen verwirft. Nach einem Dienstneustart bleibt eine bearbeitete Einrichtung zunächst stehen. Prüfe dann deine Eingaben und nutze **Einrichtung neu laden**; der Browser fragt vor dem Verwerfen nach. Diese Eingaben werden nicht über den Neustart der Seite hinweg gespeichert. Zugangsdaten landen weder in Browser-Speichern noch in Rückmelde-URLs.
