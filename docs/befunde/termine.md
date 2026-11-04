# Termine im Ereignisregister, Teil EZB

Stufe 3, nur lesend und nur Papiergeld. Wolfgang hat am 04.11.2026 den Terminkalender als nächsten Schritt freigegeben: EZB-Termine und Quartalszahlen erfassen, jeden Termin mit Quelle und Abrufzeit im Journal (R39). Die Domain `www.ecb.europa.eu` hat er für die Cloud-Umgebung freigegeben.

Ein grüner Test schließt nichts. „Technisch geschlossen“ setzt erst die Prüfung durch Maus.

## Quellen

- **EZB:** Die EZB veröffentlicht die Sitzungstermine des EZB-Rats und des Erweiterten Rats selbst auf <https://www.ecb.europa.eu/press/calendars/mgcgc/html/index.en.html>. Kostenlos, ohne Schlüssel. `robots.txt` erlaubt den Pfad (am 04.11.2026 geprüft, `Crawl-delay: 5`; ein Lauf ruft die Seite einmal ab). Die Seite ist eine einfache Liste `<dt>TT/MM/JJJJ</dt><dd>Text<br></dd>`, am 04.11.2026 mit 52 Einträgen vom 25.11.2026 bis 07.12.2028. Vergangene Termine nimmt die EZB von der Seite.
- **Quartalszahlen:** Alpha Vantage `EARNINGS_CALENDAR` reicht nicht. Am 04.11.2026 mit dem Demo-Schlüssel abgerufen: rund 3.000 Einträge, nur in den USA notierte Werte, keiner der zehn DAX-Werte der Beobachtungsliste. Woher die Quartalszahlen kommen, entscheidet Wolfgang; bis dahin gibt es keine, und nichts wird erfunden.

## Was gebaut ist

- **Termin im Kern** (`depot_kern::termin`): Kennung, Titel, Zeitbezug (Tag ohne Uhrzeit oder Zeitpunkt in UTC), Betroffene (ISIN oder Größe wie `eur-leitzins`, aufsteigend, nichts doppelt) und Verlässlichkeit (`bestaetigt`, `angekuendigt`, `vermutet`). Dasselbe Format für jede Quelle, keine Sonderregeln. Ein Termin sagt nichts über den Ausgang (R40).
- **Beleg im Journal** (`depot_journal::TerminBeleg`, Eintrag `termin`): `quelle;herkunft;abruf;roh;kennung;verlaesslichkeit;zeit;betroffen;titel`, also Quelle, Abrufzeit und SHA-256 der rohen Seite wie beim Kurs. Nur im Format 2, nur kanonisch, Abrufe fallen nie zurück (gemeinsam mit den Kursen). Ändert sich ein Termin, wird er unter seiner Kennung fortgeschrieben, und die alte Fassung bleibt. Ein unveränderter Termin wird nicht noch einmal geschrieben (`Fortschreibung::Unveraendert`); steht er doch zweimal hintereinander in der Datei, lehnt das Journal sie ab.
- **Lesen der EZB-Seite** (`depot_sammler::ezb`): genau die eine Liste im Element `main`. Jede Abweichung ist ein Fehler mit der Nummer des Eintrags, und dann gibt es keinen Termin; nichts wird übersprungen, geraten oder umgeschrieben. Die Seite nennt keine Uhrzeit, also hat kein Termin eine.
  - Kennung `ezb-JJJJ-MM-TT-n`. `n` ist die Nummer des Termins an seinem Tag; sie wird beim ersten Erscheinen vergeben und hängt danach nie an der Stelle auf der Seite (K22-01, Zuordnung unten).
  - Titel ist der Text der Seite, Leerraum zusammengefasst. Auszeichnung oder Entitäten im Text sind ein Fehler.
  - Betroffen ist `eur-leitzins` für eine geldpolitische Sitzung („monetary policy meeting“, nicht „non-monetary“), sonst `ezb`.
  - Verlässlichkeit `bestaetigt`: Die EZB veröffentlicht ihre eigenen Termine.
- **Ein Lauf** (`depot_sammler::sammle_termine`, `run.cmd termine`) in dieser Reihenfolge (R12):
  1. Abruf der Seite, Netz wie bei den Kursen (nur HTTPS, keine Weiterleitung, Zeit- und Größengrenze). Scheitert er, wird nichts abgelegt und nichts belegt.
  2. Abrufzeit von der Uhr des Rechners.
  3. Die rohe Seite liegt unter `daten/roh/ezb/<Abruf>.html`, nur neu angelegt und synchronisiert. Scheitert das, hält der Lauf, bevor etwas belegt ist.
  4. Lesen und den bekannten EZB-Terminen im Journal zuordnen. Was nicht lesbar oder nicht eindeutig zuzuordnen ist (auch kein UTF-8), wird nicht belegt, kein einziger Termin; die rohe Seite bleibt zur Durchsicht liegen.
  5. Belegen, Termin für Termin; gezählt wird neu, geändert, unverändert. Lehnt das Journal ab, etwa weil die Uhr zurückging, hält der Lauf.
  6. Gemeldet, nicht gelöscht: Kennungen künftiger EZB-Termine, die im Journal stehen, aber nicht mehr auf der Seite. Vergangene Termine fehlen dort regelmäßig und werden nicht gemeldet, Termine anderer Quellen auch nicht.
- `run.cmd termine` braucht keinen Schlüssel, nur das Journal (`run.cmd kurse einrichten`). Kein Gate, kein Export, keine Aufträge, keine KI.

Am 04.11.2026 um 21:12 UTC in der Cloud-Umgebung gegen die echte Seite gelaufen, mit einem leeren Journal außerhalb des Repos: 52 neu, im zweiten Lauf 52 unverändert, im Journal 52 Einträge `termin`.

## Nach Maus' Prüfung (K22-01)

**Befund:** Die Kennung hing an der Reihenfolge auf der Seite. Fiel von zwei Sitzungen am selben Tag die erste weg, bekam die zweite die Kennung der ersten: Sie wurde als Änderung der ersten ins Journal geschrieben und selbst als fehlend gemeldet.

**Regel jetzt** (`ezb::ordne_zu`): Die Einträge der Seite werden je Tag den bekannten EZB-Terminen aus dem Journal zugeordnet, jeweils der letzten Fassung je Kennung.

1. Ein Eintrag mit genau Titel und Betroffenen eines bekannten Termins dieses Tags ist dieser Termin, unverändert.
2. Die übrigen werden je **Art** verglichen: Gremium (EZB-Rat „Governing Council of the ECB“ oder Erweiterter Rat „General Council meeting of the ECB“, am Anfang des Titels) und Betroffene (geldpolitisch oder nicht). Bleiben von einer Art genau ein Eintrag und genau ein bekannter Termin ohne Gegenstück, ist der Eintrag dieser Termin, geändert, und wird fortgeschrieben. Das ist der übliche Fall „in Frankfurt“ zu „virtual“. Die alte Fassung bleibt im Journal. Ein anderes Gremium oder ein anderer Gegenstand ist nie eine Änderung (K22-02).
3. Bleibt von einer Art kein bekannter Termin ohne Gegenstück, sind ihre Einträge neu, ebenso jeder Eintrag ohne erkennbares Gremium. Neue Einträge bekommen in der Reihenfolge der Seite die nächsten Nummern nach der höchsten bekannten des Tags. Eine Nummer wird nie neu vergeben.
4. Sonst ist die Zuordnung mehrdeutig, etwa wenn ein bekannter Termin einer Art zwei neuen Einträgen derselben Art gegenübersteht oder umgekehrt, oder wenn zwei Einträge eines Tags gleich sind. Dann meldet der Lauf `Mehrdeutig` mit dem Tag und belegt nichts, auch nicht die eindeutigen Termine der Seite. Die rohe Seite liegt zur Durchsicht.

Bekannte Termine ohne Gegenstück bleiben, wie sie sind, und werden gemeldet, solange ihr Tag nicht vorbei ist. Grenze der Regel 2: Fällt an einem Tag ein Termin weg und kommt zugleich ein anderer **derselben Art** dazu, ist das von einer Änderung nicht zu unterscheiden. Die Seite gibt dafür nichts her, und es wird als Änderung fortgeschrieben, mit beiden Fassungen im Journal.

Rote Tests am Stand `6bad595` in `a5363a5`: `kennung_bleibt_bei_anderer_reihenfolge` (Maus' Gegenprobe zuerst), `mehrdeutige_zuordnung_wird_gemeldet`, `wegfall_am_selben_tag_im_lauf` (Maus' Gegenprobe im ganzen Lauf) und `mehrdeutig_im_lauf_nichts_belegt`. Danach kam `nummer_nie_neu_vergeben` dazu, weil die Gegenprobe Z5 grün blieb.

**Nach Maus' Nachprüfung (K22-02):** Regel 2 galt für den ganzen Tag. Stand am Tag statt einer geldpolitischen Sitzung des EZB-Rats nur noch eine des Erweiterten Rats, wurde sie unter der alten Kennung fortgeschrieben (1 geändert, 0 fehlend). Jetzt gelten die Regeln 2 bis 4 je Art. Rote Tests am Stand `fcdcbc3` in `e4bb668`: `anderes_gremium_ist_ein_anderer_termin` (Maus' Gegenprobe zuerst) und `gremiumwechsel_im_lauf` (im ganzen Lauf: 1 neu, alter Termin gemeldet, beide im Journal). Dazu kommt der Fall „anderes Gremium, gleiche Betroffene“, weil die Gegenprobe W1 ohne ihn grün blieb.

## Offen

- **Verschobene EZB-Sitzung:** Die Seite hat keine festen Kennungen, deshalb hängt die Kennung am Tag. Verschiebt die EZB eine Sitzung, entsteht eine neue Kennung; die alte bleibt im Journal und wird als „nicht mehr auf der Seite“ gemeldet, solange ihr Tag nicht vorbei ist. Den neuen Eintrag dem alten zuzuordnen hieße raten. Fortgeschrieben unter derselben Kennung wird nur, was sich am selben Tag ändert (Zuordnung oben). Ob eine gemeldete Lücke ein abgesagter Termin ist, steht damit nicht im Journal; das ist eine eigene Art Eintrag und kommt, wenn das Lagebild Termine liest.
- **Uhrzeit:** Die Seite nennt keine. Die Pressekonferenz hat eine übliche Uhrzeit, aber sie wird nicht eingesetzt, solange keine freigegebene Quelle sie zum Termin nennt.
- **Kosten der Quelle (R39):** Die EZB-Seite kostet nichts. Eine Quellenliste mit Kosten je Quelle kommt mit der zweiten Quelle für Termine.
- **Quartalszahlen:** Quelle offen (siehe oben).

## Nachweis

Rote Tests zuerst: Termin in Kern und Journal in `d513a5f`, Lesen der EZB-Seite in `fd93b05`, der Lauf im Sammler in `772b9f5`, jeweils gegen eine leere Schnittstelle. Die Seite in `crates/depot_sammler/tests/ezb.rs` ist der Inhalt von `main` der echten Seite vom 04.11.2026; nur Kopf und Fuß der Seite sind weggelassen.

| Test | zeigt |
| --- | --- |
| `depot_kern/tests/termin.rs` (3) | Zeitbezug als Text nur in genau den zwei Formen; gültige Termine, Betroffene geordnet; ungültige Kennung, Titel und Betroffene abgelehnt |
| `depot_journal/tests/termine.rs` `termin_mit_quelle_und_abrufzeit` | Beleg mit Quelle, Herkunft, Abruf, Abdruck; übersteht das Wiederöffnen |
| `verschiebung_wird_fortgeschrieben` | geänderter Termin unter derselben Kennung, alte Fassung bleibt; unverändert wird nicht geschrieben |
| `abruf_faellt_nie_zurueck` | Abruf vor dem letzten Kurs oder Termin abgelehnt |
| `beleg_nur_gueltig` | ungültige Quelle abgelehnt; Termin nur im Format 2 |
| `termineintrag_in_der_datei_nur_kanonisch` | jede Abweichung in der Datei abgelehnt, auch ein unveränderter Termin zweimal |
| `depot_sammler/tests/ezb.rs` `echte_seite` | 52 Termine, Kennungen, Titel, Betroffene, alle bestätigt, ohne Uhrzeit |
| `zwei_eintraege_am_selben_tag` | fortlaufende Kennungen am selben Tag |
| `abweichungen` | jede Abweichung der Form ist ein Fehler mit Nummer |
| `erster_lauf_belegt_jeden_termin` | rohe Seite unverändert abgelegt, 52 Termine mit Quelle, Abruf und Abdruck belegt |
| `zweiter_lauf_schreibt_nichts_doppelt` | gleiche Seite: 52 unverändert, Journal gleich lang |
| `geaenderter_termin_wird_fortgeschrieben` | 1 geändert, 51 unverändert, beide Fassungen im Journal |
| `fehlender_termin_wird_nur_gemeldet` | künftiger fehlender Termin gemeldet, vergangener und fremder nicht; nichts geschrieben |
| `unlesbare_seite_wird_nicht_belegt` | falsches Datum und kein UTF-8: nichts belegt, rohe Seite liegt |
| `netzfehler_legt_nichts_ab` | nichts abgelegt, nichts belegt |
| `uhr_zurueck_haelt_an` | Journal lehnt ab, Lauf hält, nichts belegt |
| `ohne_ablage_nichts_belegt` | Ablage scheitert: Lauf hält, bevor etwas belegt ist |
| `kennung_bleibt_bei_anderer_reihenfolge` | Wegfall, Vertauschen, neue Einträge in Reihenfolge der Seite, Änderung eines oder beider Termine eines Tags: jede Kennung bleibt |
| `nummer_nie_neu_vergeben` | neue Nummer nach der höchsten bekannten, auch mit Lücke |
| `anderes_gremium_ist_ein_anderer_termin` | anderes Gremium (mit gleichen und mit anderen Betroffenen), anderer Gegenstand, unbekanntes Gremium: neu, nie Änderung |
| `gremiumwechsel_im_lauf` | Maus' zweite Gegenprobe im Lauf: 1 neu, 0 geändert, alter Termin gemeldet |
| `mehrdeutige_zuordnung_wird_gemeldet` | drei mehrdeutige Fälle: kein Termin |
| `wegfall_am_selben_tag_im_lauf` | Maus' Gegenprobe im Lauf: zweite Sitzung unverändert, erste gemeldet, nichts geschrieben |
| `mehrdeutig_im_lauf_nichts_belegt` | gemeldet, auch der eindeutige Termin nicht belegt, rohe Seite liegt |

### Gegenproben

Jede Gegenprobe ändert den Code an einer Stelle (Z1–Z6 nach K22-01, W1–W5 nach K22-02) und muss mindestens einen Test rot machen. Jede ist neu übersetzt worden („Compiling …“ in der Ausgabe).

| Gegenprobe | rot |
| --- | --- |
| K1 doppelte Betroffene angenommen | `termin_fehler` |
| K2 `;` im Titel angenommen | `termin_fehler` |
| K3 Kennung mit `-` am Anfang angenommen | `termin_fehler` |
| J1 unveränderter Termin trotzdem geschrieben | `verschiebung_wird_fortgeschrieben` |
| J2 Abruf darf zurückfallen | `abruf_faellt_nie_zurueck` |
| J3 nicht kanonischer Eintrag angenommen | `termineintrag_in_der_datei_nur_kanonisch` |
| J4 unveränderter Termin zweimal in der Datei angenommen | `termineintrag_in_der_datei_nur_kanonisch` (zuerst grün geblieben; der Fall ist danach als Test dazugekommen) |
| G1 rohe Seite erst nach dem Belegen abgelegt | `ohne_ablage_nichts_belegt` (zuerst ohne Test; dieser ist danach dazugekommen) |
| G2 vergangene Termine als fehlend gemeldet | `fehlender_termin_wird_nur_gemeldet` |
| G3 Termine anderer Quellen als fehlend gemeldet | `fehlender_termin_wird_nur_gemeldet` |
| G4 fehlende Termine nicht gemeldet | `fehlender_termin_wird_nur_gemeldet` |
| G5 unverändert als neu gezählt | `zweiter_lauf_schreibt_nichts_doppelt`, `geaenderter_termin_wird_fortgeschrieben`, `fehlender_termin_wird_nur_gemeldet` |
| G6 bei Netzfehler leere Datei abgelegt | `netzfehler_legt_nichts_ab` |
| G7 unlesbaren Eintrag übersprungen statt Fehler | `abweichungen`, `unlesbare_seite_wird_nicht_belegt` |
| Z1 kein Abgleich nach Titel | `kennung_bleibt_bei_anderer_reihenfolge`, `wegfall_am_selben_tag_im_lauf` |
| Z2 keine Fortschreibung bei genau einem übrigen | `kennung_bleibt_bei_anderer_reihenfolge`, `geaenderter_termin_wird_fortgeschrieben`, `uhr_zurueck_haelt_an` |
| Z3 mehrdeutige Einträge als neu genommen | `mehrdeutige_zuordnung_wird_gemeldet`, `mehrdeutig_im_lauf_nichts_belegt` |
| Z4 gleiche Einträge eines Tags zugelassen | `mehrdeutige_zuordnung_wird_gemeldet` |
| Z5 neue Nummer nach Anzahl statt nach höchster | `nummer_nie_neu_vergeben` (zuerst grün geblieben; der Test ist danach dazugekommen) |
| Z6 Lauf ohne die bekannten Termine des Journals | `wegfall_am_selben_tag_im_lauf`, `mehrdeutig_im_lauf_nichts_belegt` |
| W1 Gremium nicht verglichen | `anderes_gremium_ist_ein_anderer_termin` (zuerst grün geblieben; der Fall mit gleichen Betroffenen ist danach dazugekommen) |
| W2 Betroffene nicht verglichen | `anderes_gremium_ist_ein_anderer_termin` |
| W3 unbekanntes Gremium als eigene Art | `anderes_gremium_ist_ein_anderer_termin` |
| W4 mehrdeutige Art als neu genommen | `mehrdeutige_zuordnung_wird_gemeldet`, `mehrdeutig_im_lauf_nichts_belegt` |
| W5 neue Einträge nicht in der Reihenfolge der Seite nummeriert | `kennung_bleibt_bei_anderer_reihenfolge`, `anderes_gremium_ist_ein_anderer_termin`, `wegfall_am_selben_tag_im_lauf` |
| G8 rohe Seite nicht abgelegt | `erster_lauf_belegt_jeden_termin`, `zweiter_lauf_schreibt_nichts_doppelt`, `unlesbare_seite_wird_nicht_belegt` |

Keine neuen Grenzen, keine neuen Abhängigkeiten.
