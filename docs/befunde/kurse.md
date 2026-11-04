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
- Allgemeine Regeln: Datum im Kalender, alle Zeilen mit so vielen Feldern wie der Kopf, Tage durch die ganze Datei streng steigend oder streng fallend (Alpha Vantage liefert den neuesten zuerst), Preise positiv. Der Schluss steht in ganzen Cent; ein Schluss feiner als ein Cent wird nicht gerundet, sondern abgelehnt (R4). Tief und Hoch dienen nur dem Vergleich: Sie dürfen feiner sein (bis sechs Nachkommastellen, wie Alpha Vantage sie liefert) und werden exakt mit dem Schluss verglichen. Der Schluss liegt zwischen Tief und Hoch. Jeder Fehler nennt die Zeile.
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
| `preise_in_cent` | ebenda | Preis in ganzen Cent, feiner oder nicht positiv abgelehnt |
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
| Preis feiner als Cent gerundet | `preise_in_cent`, `kursdatei_fehler` |
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

## Offen

- **Abruf:** Der Container für Dachs sperrt alle geprüften Kursquellen (Stooq, Alpha Vantage, Yahoo, Börse Frankfurt). Der Abruf folgt im Crate `depot_sammler` mit einer HTTP-Abhängigkeit, sobald Wolfgang eine Quelle freigibt. Neues Crate und neue Abhängigkeit ändern die Festschreibung und brauchen eine Grenzfreigabe. Dazu gehören die festgeschriebene Quellenliste mit der Zuordnung von ISIN zum Kürzel der Quelle und das Aufbewahren der rohen Antworten in `daten/`.
- **Schlüssel:** Braucht die Quelle einen API-Schlüssel, liest ihn der Sammler nur aus einer Datei außerhalb des Repos. Er steht nie im Journal, nie in einer Meldung und nie im Export.
- **Terminkalender** (EZB, Quartalszahlen): nächster Schritt.
