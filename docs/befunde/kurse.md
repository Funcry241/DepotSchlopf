# Lesende Datenversorgung, Teil Kurse

Stufe 3, zweiter Schritt, nur lesend und nur Papiergeld. Wolfgang hat am 04.11.2026 die lesende Datenversorgung freigegeben: zuerst eine kostenlose Quelle für Xetra-Schlusskurse mit Herkunft und Abrufzeit im Journal, danach der Terminkalender. Keine Geheimnisse ins Repo; braucht eine Quelle einen API-Schlüssel, liegt er nur lokal außerhalb des Repos.

Ein grüner Test schließt nichts. „Technisch geschlossen“ setzt erst die Prüfung durch Maus.

## Was gebaut ist

Dieser Teil braucht kein Netz. Er legt fest, wie eine Antwort der Quelle gelesen und wie jeder Kurs daraus belegt wird. Den Abruf selbst baut der nächste Schritt im Crate `depot_sammler`, sobald die Quelle für das Netz freigegeben ist. Bis dahin entstehen hier keine echten Kurse; alle Kurse in den Tests sind erfunden und haben nur die Form der Quellen.

**Zeit in Frankfurt** (`crates/depot_kern/src/zeit.rs`):

- `Zeitpunkt::frankfurt` rechnet eine Frankfurter Uhrzeit nach UTC um, nach EU-Recht: Sommerzeit vom letzten Sonntag im März bis zum letzten Sonntag im Oktober, jeweils um 01:00 Uhr UTC. Eine Uhrzeit, die es wegen der Umstellung nicht gibt oder doppelt gibt, ergibt keinen Zeitpunkt. Der Kalender rät nie.
- Ein Zeitpunkt steht als Text immer in UTC, genau als `JJJJ-MM-TTThh:mm:ssZ`, und wird nur in dieser Form gelesen (`Zeitpunkt::lies`, `Display`).

**Tageskurse aus einer Quelle** (`crates/depot_kern/src/quelle.rs`):

- `lies_tageskurse` liest eine CSV-Antwort mit Kopfzeile: genau eine Datumsspalte (`date` oder `timestamp`) und die Spalte `close`; `low` und `high` werden geprüft, wenn es sie gibt. Das ist die Form von Stooq (`Date,Open,High,Low,Close,Volume`) und von Alpha Vantage mit `datatype=csv` (`timestamp,open,high,low,close,volume`).
- Allgemeine Regeln: Datum im Kalender, alle Zeilen mit so vielen Feldern wie der Kopf, Tage durch die ganze Datei streng steigend oder streng fallend (Alpha Vantage liefert den neuesten zuerst), Preise positiv und exakt. Schluss, Tief und Hoch werden mit bis zu sechs Nachkommastellen gelesen, wie Alpha Vantage sie liefert, und exakt verglichen (Fassung 3, siehe unten). Was noch feiner ist, wird nicht gerundet, sondern abgelehnt (R4). Der Schluss liegt zwischen Tief und Hoch. Jeder Fehler nennt die Zeile.
- **Schlusskurs:** der Kurs eines Handelstags zum Ende des Xetra-Handels um 17:35 Uhr in Frankfurt. `schlusskurse` bildet aus den Tageskursen eines Abrufs die Kurse des letzten beendeten Tags (`heute`) und des Tags davor (`vortag`), beide mit dem Abruf als Stand und der neuen Herkunft `Quelle`. Eine Zeile für einen Tag, dessen Handel beim Abruf nicht beendet war, ist kein Schlusskurs und zählt nicht. Fehlt der Vortag, fehlt er; er wird nicht erfunden.

**Belegte Kurse** (`crates/depot_journal/src/kursbeleg.rs`, R12):

- Jeder Kurs von außen steht vor seiner Verwendung im Journal als Eintrag `kurs;quelle;herkunft;abruf;roh;isin;preis;zeitpunkt`. `roh` ist der SHA-256 der rohen Antwort. Wer die Antwort aufbewahrt, kann sie damit dem Kurs zuordnen; im Journal steht nur der Abdruck.
- Allgemeine Regeln, beim Anhängen und beim Öffnen gleich (R20): Der Name der Quelle besteht aus Kleinbuchstaben, Ziffern und `-`. Der Kurs liegt nie nach seinem Abruf. Die Abrufe im Journal fallen nie zurück. Der Eintrag gilt nur in genau der Form, die das Journal selbst schreibt. Kurse gibt es nur im Format 2; ein altes Journal belegt sie erst nach der Umstellung.
- `Stand::kurse` liefert alle belegten Kurse nach einem Neustart unverändert zurück.

**Dieselben Prüfungen wie für jeden Kurs:** Ein belegter Kurs ist ein gewöhnlicher `Kurs`. Höchstalter, Handelstag, Vergleichskurs und Kurssprung prüft weiter `pruefe_kurse` im Regelwerk. Mit dem festgeschriebenen Höchstalter von 20 Minuten besteht ein Schlusskurs diese Prüfung nur kurz nach dem Xetra-Schluss seines Handelstags. Schlusskurse taugen damit vor allem zum Beobachten und für den Papierbetrieb am Abend; für Entscheidungen im Handel bleiben die verzögerten Broker-Kurse vorgesehen (Kap. 6).

## Nachweis

Tests, die Schnittstelle gab es vorher nicht:

| Test | Datei | zeigt |
| --- | --- | --- |
| `zeitpunkt_als_text` | `crates/depot_kern/tests/quelle.rs` | Zeitpunkt als UTC-Text, nur genau diese Form lesbar |
| `wochentage` | ebenda | Wochentag nach ISO 8601 |
| `frankfurter_zeit` | ebenda | Winter, Sommer, beide Umstellungstage, ein zweites Jahr, Xetra-Schluss |
| `preise_exakt` | ebenda | Preis exakt bis sechs Nachkommastellen, feiner oder nicht positiv abgelehnt (bis Fassung 2: `preise_in_cent`) |
| `kursdatei_lesen` | ebenda | Form von Stooq und Alpha Vantage ergibt dieselben Kurse, auch in der echten Form von Alpha Vantage (CRLF, vier Nachkommastellen, Tief und Hoch feiner als ein Cent) |
| `kursdatei_fehler` | ebenda | jede Abweichung ein Fehler mit Zeile |
| `schlusskurse_eines_abrufs` | ebenda | offener Tag zählt nicht, Herkunft `Quelle`, Alter, fehlender Vortag |
| `schlusskurse_fehler` | ebenda | Wert doppelt, Tage in falscher Reihenfolge |
| `kurs_mit_herkunft_und_abrufzeit` | `crates/depot_journal/tests/kurse.rs` | Eintrag mit Quelle, Herkunft, Abruf, Abdruck der Antwort; nach Neustart unverändert |
| `belegte_kurse_bestehen_dieselben_pruefungen` | ebenda | nach Neustart dieselben Kurse; `pruefe_kurse`: kurz nach Schluss bestanden, 35 Minuten später zu alt, am nächsten Morgen nicht vom Handelstag |
| `beleg_nur_gueltig` | ebenda | Name der Quelle, Kurs nicht nach dem Abruf |
| `abruf_faellt_nie_zurueck` | ebenda | zurückfallender Abruf abgelehnt, Datei unverändert |
| `kurseintrag_in_der_datei_nur_kanonisch` | ebenda | 14 veränderte Einträge und ein zurückfallender Abruf in der Datei: Halt |
| `kurs_erst_nach_der_umstellung` | `crates/depot_sim/tests/format.rs` | altes Journal aus PR #15: Kurs erst nach `stelle_um` |

Gegenproben, jeweils neu übersetzt, jede macht mindestens einen Test rot:

| Gegenprobe | rot |
| --- | --- |
| immer Winterzeit | `frankfurter_zeit` |
| doppelte Ortszeit geraten | `frankfurter_zeit` |
| Zeitpunkt ohne `Z` gelesen | `zeitpunkt_als_text` |
| Preis feiner als sechs Nachkommastellen gerundet | `preise_exakt`, `kursdatei_fehler` |
| Spanne nicht geprüft | `kursdatei_fehler` |
| Tief und Hoch nur in ganzen Cent gelesen (Stand vor Fassung 2) | `kursdatei_lesen`, `kursdatei_fehler` |
| Tief und Hoch auf ganze Cent abgeschnitten statt exakt verglichen | `kursdatei_fehler` |
| Reihenfolge nicht geprüft | `kursdatei_fehler` |
| absteigende Datei nicht umgedreht | `kursdatei_lesen` |
| offener Tag zählt als Schluss | `schlusskurse_eines_abrufs` |
| Schluss um 17:30 statt 17:35 | `frankfurter_zeit`, `schlusskurse_eines_abrufs` |
| Reihenfolge in `schlusskurse` nicht geprüft | `schlusskurse_fehler` |
| Abruf darf zurückfallen | `abruf_faellt_nie_zurueck`, `kurseintrag_in_der_datei_nur_kanonisch` |
| Eintrag nicht kanonisch geprüft | `kurseintrag_in_der_datei_nur_kanonisch` |
| Kurs im Format 1 | `kurs_erst_nach_der_umstellung` |
| Kurs nach seinem Abruf | `beleg_nur_gueltig`, `kurseintrag_in_der_datei_nur_kanonisch` |
| jeder Name der Quelle | `beleg_nur_gueltig`, `kurseintrag_in_der_datei_nur_kanonisch` |
| Abdruck nicht der rohen Antwort | `kurs_mit_herkunft_und_abrufzeit` |
| Abdruck nicht geprüft | `kurseintrag_in_der_datei_nur_kanonisch` |

## Fassung 3: Schlusskurse feiner als ein Cent

Wolfgangs erster echter Lauf am 04.11.2026 hat bei BASF, Mercedes-Benz (Vortag) und Infineon die ganze Antwort verworfen: Der Schluss lag auf einem halben Cent (49,925 €, 41,155 €, 49,885 €), und der Schluss wurde nur in ganzen Cent angenommen. Xetra notiert solche Preise regulär, der Fehler lag also in der Regel und nicht in den Daten. Die Regel gilt jetzt für jede Preisspalte gleich, ohne Sonderfall.

- **Exakter Preis** (`crates/depot_kern/src/preis.rs`): `Preis` führt einen Preis je Stück in Zehntausendstel Cent, also auf ein Millionstel Euro genau. `Preis::lies_euro` liest höchstens sechs Nachkommastellen ungleich null. `Preis::als_cent` gibt einen Betrag nur, wenn der Preis genau auf einem Cent liegt, und rundet nie. `in_bp_von` rechnet in `i128` und ergibt für ganze Cent dasselbe wie `Cent::in_bp_von`.
- **Kurs:** `Kurs::preis` ist der exakte `Preis`. `Kurs::exakt` nimmt einen `Preis`, `Kurs::neu` wie bisher ganze Cent. `Kurs::preis_cent` gibt ganze Cent oder `None`.
- **Journal:** Das Feld `preis` bleibt in Cent. Ganze Cent stehen wie bisher ohne Punkt (`4148`), ein feinerer Preis steht mit den nötigen Nachkommastellen ohne Null am Ende (`4115.5`, höchstens vier Stellen, `Preis::cent_text`). Gelesen wird nur genau diese Form. Bestehende Journale bleiben damit gültig und kanonisch, und der Abdruck des simulierten Verlaufs bleibt für ganze Cent gleich (K15-01, `befund_k16_02_altes_journal_wird_fortgesetzt`).
- **Kursvergleich:** `pruefe_kurse` vergleicht exakt. Vortag 1,005 € und heute 1,156 € ergeben 1.502 bp, also einen Sprung. Auf ganze Cent gerundet (1,01 €) wären es 1.445 bp, und der Sprung bliebe unentdeckt.
- **Geld:** Mit Fassung 3 fiel jede Geldrechnung mit einem solchen Kurs geschlossen aus. Fassung 4 rundet dort zur sicheren Seite, siehe unten.
- **Sammler:** `Ergebnis::Belegt` trägt den exakten Preis, und `run.cmd kurse` zeigt ihn ungerundet an (`49,925 EUR`).

Tests, an `c9fc313` rot (die echten Zeilen wurden verworfen):

| Test | Datei | zeigt |
| --- | --- | --- |
| `schluss_mit_bruchteil_eines_cents` | `crates/depot_kern/tests/quelle.rs` | Wolfgangs drei Originalzeilen (BASF Zeile 24, Mercedes-Benz Zeile 3, Infineon Zeile 10) werden exakt gelesen, nicht gerundet, bleiben als Kurs exakt und ergeben keinen Cent-Betrag; die echte SAP-Antwort in ganzen Cent bleibt unverändert |
| `kurs_mit_bruchteil_eines_cents_exakt` | `crates/depot_journal/tests/kurse.rs` | `4992.5` im Journal gelesen, sechs andere Schreibweisen Halt; geschrieben wird `4115.5`, nach dem Neustart derselbe Kurs |
| `halber_cent_wird_belegt` | `crates/depot_sammler/tests/sammler.rs` | Lauf mit Mercedes-Benz' echtem Vortag 41,155 €: verwendbar, exakt im Bericht und im Journal |

Dazu kommen, mit der neuen Schnittstelle: `kurssprung_exakt_mit_bruchteil_eines_cents` (`crates/depot_regelwerk/tests/regelwerk.rs`) und die Einheitstests in `preis.rs` (Lesen, Journalform hin und zurück, nur ganze Cent sind Cent, Basispunkte gleich wie bei `Cent`, Anzeige).

| Gegenprobe | rot |
| --- | --- |
| Schluss wieder nur in ganzen Cent | `schluss_mit_bruchteil_eines_cents`, `preise_exakt`, `kursdatei_lesen`, `kursdatei_fehler` |
| `Kurse::kurs` rundet auf ganze Cent ab | `schluss_mit_bruchteil_eines_cents`, `kurssprung_exakt_mit_bruchteil_eines_cents` |
| Journal schreibt abgeschnittene Cent | `kurs_mit_bruchteil_eines_cents_exakt` |
| Kursvergleich mit abgeschnittenem Vortag | `kurssprung_exakt_mit_bruchteil_eines_cents` |
| Abdruck des Verlaufs in neuer Einheit | `befund_k16_02_altes_journal_wird_fortgesetzt` |

Die Journalform prüft das Journal doppelt: `Preis::lies_cent_text` liest nur die kanonische Form, und das Journal vergleicht jeden Eintrag zusätzlich mit seiner eigenen Schreibweise. Eine Gegenprobe, die nur die erste Prüfung abschaltet, bleibt deshalb grün.

## Fassung 4: Geld aus exakten Kursen, zur sicheren Seite gerundet

Vorgabe von Maus am 04.11.2026, von Wolfgang weitergegeben: Eingelesene Kurse bleiben exakt. Gerundet wird erst dort, wo wirklich ganze Cent gebraucht werden, und jedes Mal zur sicheren Seite. Die Regel gilt für jeden Kurs, auch für einen in ganzen Cent.

- **Kern** (`crates/depot_kern/src/preis.rs`): `Rundung::Ab` und `Rundung::Auf`. `Preis::mal_stueck(stueck, rundung)` und `Preis::mal_bp(bp, rundung)` rechnen exakt in `i128` und runden erst am Schluss, in der Richtung, die der Aufrufer nennt. `Preis::stueck_fuer(betrag)` ergibt die ganzen Stücke, die höchstens `betrag` kosten. `Kurse::kurs` liefert jetzt den exakten `Preis`, `Kurse::kurs_cent` ganze Cent nur dann, wenn der Preis genau auf einem Cent liegt.
- **Depotbewertung abgerundet:** `Topf::wert` rundet den Gesamtwert auf ganze Cent ab. Das Depot rechnet sich dadurch nie reicher.
- **Grenzen exakt (K21-01):** Jede Anteilsgrenze misst an exakten Werten (`Topf::wert_exakt`, `Preis::mal_stueck_exakt`), nie an gerundeten, denn eine Rundung kann einen Grenzübertritt verdecken oder vortäuschen. Das gilt beim Auslösen des Abbaus, für den verbleibenden Bestand, für die Kaufgrenze, für die Umschichtung und für die Reihenfolge des Positionsabbaus. Ganze Cent entstehen erst dort, wo ein Betrag in Cent gebraucht wird, und zwar zur sicheren Seite: Der freie Umschlag wird abgerundet, Umschlag und Minderung eines Auftrags werden aufgerundet.
- **Wächter:** Tagesverlust und Notbremse messen den heutigen Wert (abgerundet) an Vortag und Höchststand (beide aufgerundet). So verdeckt die Rundung keinen Verlust. Beide abgerundet hätten bei 49,929 € gegen 47,43255 € statt exakt 5 % nur 4,988 % ergeben. Exakt ließe sich das erst prüfen, wenn das Journal Höchststände exakt führt. Die gewählte Richtung kann einen Halt höchstens etwas früher auslösen, nie verdecken.
- **Kauflimit als Höchstpreis abgerundet:** Kurs × (1 + Abstand), abgerundet auf ganze Cent. **Verkaufslimit als Mindestpreis aufgerundet:** Kurs × (1 − Abstand), aufgerundet. Das gilt in der Entscheidung und in der Notbremse. Kein Auftrag kauft damit teurer oder verkauft billiger, als Kurs und Limit-Abstand vorsehen.
- **Deckung:** Reserviert wird wie bisher Limit × Menge plus höchste Gebühr. Das Limit hat ganze Cent und liegt beim Kauf über jedem möglichen Ausführungspreis. Die Umschlaggrenze und die höchstmögliche Wertminderung eines Auftrags rechnen aufgerundet.
- **Änderung bei ganzen Cent:** Beim Verkaufslimit ändert sich für ganze Cent nichts, denn Kurs − abgerundeter Abstand ist dasselbe wie das aufgerundete Produkt. Das Kauflimit war bisher Kurs + aufgerundeter Abstand und konnte deshalb einen Cent über dem Höchstpreis liegen: Bei 99,99 € war es 100,99 €, obwohl der Höchstpreis bei 100,9899 € liegt. Jetzt ist es 100,98 €. Alle bisherigen Tests und der Abdruck des simulierten Verlaufs bleiben grün.

Tests, an `451ca7c` rot (`crates/depot_regelwerk/tests/rundung.rs`, mit den Kursen aus Wolfgangs Lauf):

| Test | zeigt |
| --- | --- |
| `bewertung_abgerundet` | 3 × 49,925 € zählen als 149,77 € |
| `kauflimit_hoechstpreis_abgerundet` | 99,99 € ergibt das Limit 100,98 € (vorher 100,99 €), 49,925 € ergibt 50,42 € |
| `verkaufslimit_mindestpreis_aufgerundet` | 41,155 € ergibt das Limit 40,75 € |
| `notbremse_mit_feinem_kurs` | Die Notbremse verkauft mit 41,155 € und 49,925 € zu 40,75 € und 49,43 € (vorher kein Verkauf) |

Nach Maus' Prüfung (K21-01: Der Pflichtabbau maß an abgerundeten Werten. 1 BASF zu 49,925 € und 134,98 € Bargeld sind exakt 27,00035 %, abgerundet 26,99838 %, und der Abbau blieb aus). Rot an `13dffce`:

| Test | zeigt |
| --- | --- |
| `abbau_prueft_den_anteil_exakt` | Maus' Gegenprobe: Abbau. 134,99 €: kein Abbau. 3 BASF und 404,95 € (exakt 26,99999 %): kein Abbau, mit abgerundetem Topfwert wären es 27,00024 % |
| `abbau_genau_dann_und_rest_exakt_im_rahmen` | Über rund 160.000 Fälle gilt: Abbau genau dann, wenn der exakte Anteil über der Schwelle liegt, und danach liegt der Rest exakt im Höchstanteil des Topfwerts nach der eigenen Minderung |
| `waechter_rundung_verdeckt_keinen_verlust` | exakt 5 % Tagesverlust bleibt ein Halt, auch wenn der Kurs feiner als ein Cent ist |

Dazu kommt der Einheitstest `auf_ganze_cent` in `preis.rs`: beide Richtungen, negative Beträge, gleiches Ergebnis wie `Cent::mal_stueck` bei ganzen Cent, Überlauf, Stücke für einen Betrag.

| Gegenprobe | rot |
| --- | --- |
| Bewertung aufgerundet | `bewertung_abgerundet` |
| Kauflimit aufgerundet | `kauflimit_hoechstpreis_abgerundet` |
| Verkaufslimit abgerundet | `verkaufslimit_mindestpreis_aufgerundet` |
| Notbremse rundet ab | `notbremse_mit_feinem_kurs` |
| `Auf` rundet auch glatte Beträge auf | `auf_ganze_cent` und weitere |
| Abbau löst an abgerundetem Positionswert aus | `abbau_prueft_den_anteil_exakt` |
| Restbestand an abgerundetem Wert gemessen | `abbau_genau_dann_und_rest_exakt_im_rahmen` (3 Stück, 52,41 €) |
| Topfwert für Grenzen abgerundet | `abbau_prueft_den_anteil_exakt` |

Die Verdrahtung des Wächters in der Simulation (`depotwert` mit `Rundung::Auf` für Vortag und Höchststand) prüft kein eigener Test: Die Simulation hat nur erfundene Kurse in ganzen Cent, und dort sind beide Richtungen gleich.

## Offen

- **Papierausführung mit feinem Kurs:** Das erfundene Papierkonto der Simulation führt nur zu Kursen in ganzen Cent aus (`Kurse::kurs_cent`), bei einem feineren Kurs verfällt die Order. Solange die Simulation nur erfundene Kurse in ganzen Cent hat, spielt das keine Rolle. Für den Papierbetrieb mit Kursen aus der Quelle braucht die Ausführung eine Regel: Kaufbetrag aufgerundet, Verkaufserlös abgerundet.


- **Abruf:** gebaut, siehe `docs/befunde/sammler.md`.
- **Terminkalender** (EZB, Quartalszahlen): nächster Schritt.
