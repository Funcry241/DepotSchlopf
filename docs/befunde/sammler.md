# Lesende Datenversorgung, Teil Abruf

Stufe 3, nur lesend und nur Papiergeld. Wolfgang hat am 04.11.2026 Alpha Vantage als Quelle für Xetra-Schlusskurse gewählt. Der Schlüssel liegt nur bei ihm in der Windows-Anmeldeinformationsverwaltung (Kap. 13, wie die Schlüssel der KI-Anbieter).

Ein grüner Test schließt nichts. „Technisch geschlossen“ setzt erst die Prüfung durch Maus.

## Warum nicht Stooq

Stooq war vorgesehen, ist aber nicht nutzbar. Am 04.11.2026 geprüft: Die Einzelabfrage `q/l/` gibt es nicht mehr, der Download der Historie `q/d/l/` steht hinter einer JavaScript-Prüfung, und `robots.txt` verbietet automatische Abrufe für alle außer Suchmaschinen. Diese Sperre wird nicht umgangen. Kostenlose Quellen ganz ohne Schlüssel gibt es für Xetra-Schlusskurse praktisch nicht mehr. Alpha Vantage gibt einen kostenlosen Schlüssel (25 Abrufe am Tag), führt Xetra-Werte mit der Endung `.DEX` und liefert CSV.

## Was gebaut ist

**Neues Crate `depot_sammler`**, das einzige mit Netz (Kodex `NETZ_ERLAUBT`, eigene Änderung davor). Kein anderes Crate hängt von ihm ab; `run.cmd kurse` startet es als eigenes Programm.

- **Beobachtungsliste** (`liste.rs`): zehn DAX-Werte mit ISIN und Kürzel bei der Quelle. Sie steht im Code, ändert sich nur mit einer geprüften Änderung und ist kein Universum für den Handel. Geprüft: gültige ISIN, Kürzel aus Großbuchstaben und Ziffern mit `.DEX`, nichts doppelt. Zehn Abrufe passen in das Tageskontingent von 25.
- **Schlüssel** (`schluessel.rs`): nur aus der Windows-Anmeldeinformationsverwaltung, generischer Eintrag `DepotSchlopf/alphavantage`. Wolfgang legt ihn mit `cmdkey /generic:DepotSchlopf/alphavantage /user:depotschlopf /pass` an; `cmdkey` fragt verdeckt, der Schlüssel erscheint weder auf dem Bildschirm noch in der Befehlsgeschichte. Erlaubt sind 8 bis 64 Großbuchstaben und Ziffern, so kann er die Abrufadresse nicht verändern. Er hat keine Textform (`Debug` zeigt `***`), und jede Meldung des Sammlers wird vor der Ausgabe von ihm bereinigt, in jeder Schreibweise. Außerhalb von Windows gibt es keinen Ort für ihn; der Sammler bricht dort ab.
- **Abruf** (`netz.rs`): `ureq` mit `rustls`, ohne Kompression. Nur HTTPS, nur `GET`, keine Weiterleitung, 30 Sekunden Zeitgrenze, höchstens 1 MB Antwort, nur Status 200. In Tests ersetzt eine erfundene Quelle das Netz (Trait `Netz`).
- **Ein Lauf** (`sammeln.rs`), je Wert in dieser Reihenfolge (R12):
  1. Abruf der Tagesreihe als CSV (`TIME_SERIES_DAILY`, letzte 100 Handelstage), 15 Sekunden Abstand zwischen zwei Abrufen.
  2. Abrufzeit von der Uhr des Rechners; ohne gültige Uhr hält der Lauf, bevor etwas abgelegt ist.
  3. Enthält die Antwort den Schlüssel, gleich in welcher Schreibweise, wird sie weder abgelegt noch gelesen, und der Lauf endet (K19-01).
  4. Die rohe Antwort liegt unter `daten/roh/alphavantage/<Abruf>_<ISIN>.csv`, nur neu angelegt und synchronisiert. `daten/` ist nie im Repo und nie im Export.
  5. Meldet die Quelle statt Kursen einen Hinweis (JSON): Ein Fehler zu genau diesem Kürzel (`Error Message`) betrifft nur diesen Wert. Jeder andere Hinweis, etwa das erreichte Tageskontingent, beendet den Lauf; die übrigen Werte werden nicht abgerufen.
  6. Lesen mit `lies_tageskurse` und `schlusskurse` (Teil Kurse). Was nicht lesbar ist, wird nicht belegt; die rohe Antwort bleibt zur Durchsicht liegen.
  7. Belegen im Journal, erst der Vortag, dann der letzte beendete Tag, jeweils mit Quelle, Abrufzeit und Abdruck genau dieser Antwort. Lehnt das Journal ab, etwa weil die Uhr zurückging, hält der Lauf.
- **Prüfen:** Danach prüft `pruefe_kurse` alle Werte der Liste wie jeden Kurs, mit dem festgeschriebenen Höchstalter von 20 Minuten. Das Alter misst gegen den letzten Abruf; für früher abgerufene Werte ist das strenger, nie milder. Kurz nach dem Xetra-Schluss besteht ein Schlusskurs, am nächsten Morgen nicht mehr. Der Sammler meldet das Ergebnis nur; er entscheidet nichts. Verwendbar sind die Kurse eines Laufs nur, wenn Schlusskurse belegt sind und die Prüfung für alle Werte der Liste nichts meldet; ein Wert mit Fehler oder ohne Abruf fehlt dort als `KursFehlt`. Nur dann endet `run.cmd kurse` mit Erfolg, sonst mit Fehlercode, auch wenn belegt wurde (K19-02).
- **`run.cmd kurse`** startet den Sammler, `run.cmd kurse einrichten` legt das Journal `daten/journal.txt` einmal an. Kein Gate, kein Export, keine Aufträge, keine KI.
- **Zeit:** `Zeitpunkt::aus_sekunden` im Kern nimmt die Uhr des Rechners an, die der Aufrufer liest; der Kern selbst liest keine Uhr.

## Nach Maus' Prüfung (K19-01, K19-02)

- **K19-01:** Gab die Quelle den Schlüssel in einer Antwort zurück, lag er unverändert in der Rohdatei; bereinigt wurde nur die Ausgabe. Jetzt wird jede Antwort vor dem Ablegen geprüft (`Schluessel::steht_in`, ohne Rücksicht auf Groß- und Kleinschreibung); eine Antwort mit dem Schlüssel wird nicht abgelegt und nicht gelesen, und der Lauf endet. Auch die Bereinigung der Meldungen erkennt jetzt jede Schreibweise.
- **K19-02:** `run.cmd kurse` meldete Erfolg, wenn die Prüfung zu alte Kurse fand. Jetzt entscheidet `Bericht::verwendbar` über den Ausgang.
- Rote Tests am Stand `3b4ebed` in `9cbe2b9`: `schluessel_erscheint_nirgends` prüft Namen **und Inhalt** jeder abgelegten Datei für drei Fälle (Schlüssel groß im Hinweis, klein im Hinweis, in einer CSV); vier Tests prüfen `verwendbar`.

## Neue Abhängigkeiten (Festschreibung, Grenzfreigabe nötig)

| Abhängigkeit | wo | wofür |
| --- | --- | --- |
| `ureq` 3.5, nur `rustls` | `depot_sammler` | HTTPS-Abruf; zieht `rustls`, `ring`, `webpki-roots` u. a. nach |
| `keyring-core` 1.0 | `depot_sammler`, nur Windows | Zugriff auf die Anmeldeinformationsverwaltung |
| `windows-native-keyring-store` 1.1, ohne Standardfunktionen | `depot_sammler`, nur Windows | Speicher der Anmeldeinformationsverwaltung; zieht `windows-sys` nach |

`ring` übersetzt beim Bauen C- und Assemblerteile. Unter Windows braucht das die Build Tools, die Rust für die MSVC-Zielplattform ohnehin voraussetzt. Clippy läuft unter Linux und für `x86_64-pc-windows-gnu` ohne Warnung.

## Nachweis

Tests in `crates/depot_sammler/tests/sammler.rs`, die Schnittstelle gab es vorher nicht. Die erfundenen Antworten haben die echte Form von Alpha Vantage (am 04.11.2026 mit dem Demo-Schlüssel abgerufen: CRLF, vier Nachkommastellen, neuester Tag zuerst, Tief feiner als ein Cent).

| Test | zeigt |
| --- | --- |
| `beobachtungsliste_gueltig` | Liste gültig und im Kontingent; falsche ISIN, falsches Kürzel, Doppelte und leere Liste abgelehnt |
| `schluessel_ohne_textform` | `Debug` ohne Schlüssel, Bereinigung, Form des Schlüssels, Fehlermeldungen ohne Schlüssel |
| `abrufadresse` | HTTPS bei Alpha Vantage, Tagesreihe als CSV für das Kürzel |
| `normaler_lauf_belegt_und_prueft` | Rohdatei byte-gleich mit der Antwort, vier Belege mit Quelle, Abrufzeit, Abdruck und Herkunft; Prüfung kurz nach Schluss bestanden |
| `am_naechsten_morgen_zu_alt` | Abruf am nächsten Morgen: Prüfung meldet Abruf nicht am Handelstag und zu alte Kurse; belegt, aber nicht verwendbar |
| `hinweis_beendet_den_lauf` | Hinweis zum Kontingent: kein weiterer Abruf, nichts belegt |
| `falsches_kuerzel_nur_fuer_seinen_wert` | Fehler zu einem Kürzel: übrige Werte belegt, Prüfung nennt den fehlenden Kurs |
| `unlesbare_antwort_nicht_belegt` | Schluss feiner als ein Cent und HTML: nicht belegt, roh abgelegt |
| `schluessel_erscheint_nirgends` | Netzfehler mit Adresse in beiden Schreibweisen; Antwort mit Schlüssel groß, klein oder in einer CSV: Lauf endet, Bericht, Journal, Namen und Inhalt aller abgelegten Dateien ohne Schlüssel |
| `uhr_haelt_den_lauf` | Uhr geht zurück: Journal lehnt ab, Lauf hält; ohne Uhr nichts abgelegt |

Gegenproben, jeweils neu übersetzt, jede macht mindestens einen Test rot:

| Gegenprobe | rot |
| --- | --- |
| Hinweis beendet den Lauf nie | `hinweis_beendet_den_lauf` |
| jeder Hinweis beendet den Lauf | `falsches_kuerzel_nur_fuer_seinen_wert` |
| Hinweis nicht erkannt | `hinweis_beendet_den_lauf`, `falsches_kuerzel_nur_fuer_seinen_wert` |
| Meldungen nicht vom Schlüssel bereinigt | `schluessel_erscheint_nirgends`, `schluessel_ohne_textform` |
| Kurse nicht belegt | `normaler_lauf_belegt_und_prueft` und drei weitere |
| Vortag nicht belegt | `normaler_lauf_belegt_und_prueft` und drei weitere |
| Rohdatei leer abgelegt | `normaler_lauf_belegt_und_prueft` |
| Antwort mit Schlüssel nicht abgefangen (Stand `3b4ebed`) | `schluessel_erscheint_nirgends` |
| Schlüssel in der Antwort nur in Großschreibung erkannt | `schluessel_erscheint_nirgends` |
| Bereinigung der Meldungen nur in Großschreibung | `schluessel_erscheint_nirgends` |
| `verwendbar` ohne Prüfung (Stand `3b4ebed`: Erfolg trotz zu alter Kurse) | `am_naechsten_morgen_zu_alt`, `falsches_kuerzel_nur_fuer_seinen_wert` |
| `verwendbar` ohne belegte Schlusskurse | `hinweis_beendet_den_lauf`, `schluessel_erscheint_nirgends` |

Ein Abruf über das echte Netz mit `Https` lief am 04.11.2026 aus dem Container gegen Alpha Vantage (Demo-Schlüssel, Wert IBM): 100 Tageszeilen, von `lies_tageskurse` gelesen. Den Lesezugriff auf die Anmeldeinformationsverwaltung prüft erst Wolfgangs erster Lauf unter Windows.

## Offen

- Erster echter Lauf auf Wolfgangs Rechner: Schlüssel aus der Anmeldeinformationsverwaltung, Kürzel der zehn Werte bei der Quelle.
- **Terminkalender** (EZB, Quartalszahlen): nächster Schritt.
- Die Beobachtungsliste steht im Code. Ob sie wie die Quellenlisten aus Kap. 6 in die Festschreibung gehört, entscheidet die Prüfung.
