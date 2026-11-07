# Nachprüfung der Issues #56 und #58

Stand: 07.11.2026. Klasse: Funktion. Keine neuen Bibliotheken, Quellen,
Budgets, Freigaberegeln oder Geldgrenzen.

## #56: Update mit echten Daten, ohne Prüfordner

Die Umsetzung ist bereits auf main. Bei der Nachprüfung wurden die folgenden
Stellen und bestehenden Regressionstests kontrolliert; die neue Oberfläche
ändert diesen Ablauf nicht.

| Anforderung | Nachweis |
| --- | --- |
| Neuer Stand liest eine Kopie vor dem Einspielen | `depot_lauf::update::pruefe_datenkopie` und `einspielen_mit_datenprobe`: neues Programm `depot_lage --update-datenprobe`, danach unveränderte Prüfkopie verlangen |
| Originaldateien bis zum Einspielen unverändert | Gemeinsame Journalsperre, Bytevergleich unmittelbar vor dem Vorrücken; Test `update_rueckt_nur_nach_datenprobe_und_gepruefter_sicherung_vor` |
| Geprüfte Sicherung vor der Update-Markierung | `depot_lauf/src/updatedaten.rs`: Journal, Vermerk, Papierkonto, KI-Halt und vorhandene Kassenbuchdateien; create_new, sync_all, SHA-256-Vergleich, erst dann abgeschlossener Sicherungsordner |
| Fehler verhindert das Update | Derselbe Test prüft fehlgeschlagene Datenprobe und blockierte Sicherung: kein Vorrücken, keine offene Update-Markierung |
| Begrenzung und keine automatische Datenrücksetzung | `updatedaten::begrenze`: höchstens zehn eigene abgeschlossene Sicherungen; Code-Rückfall stellt keine Daten zurück |
| Neuer Dienst bestätigt nur lesbare echte Daten | `bestaetigen_lehnt_unlesbares_journal_ab_und_erlaubt_code_rueckfall` |
| Kurze Journalsperre wird erneut versucht | `bestaetigung_wiederholt_eine_voruebergehende_journalsperre` |
| Bestätigung bleibt aus, auch bei Exitcode 0 | `ende_vor_der_bestaetigung_faellt_zurueck_auch_mit_code_0` |
| Dialogtest ohne Schreiben | `testdialog_ist_ohne_sperre_verfuegbar_und_schreibt_nichts`: unveränderte Journal-, Vermerk- und Konto-Bytes |
| Nein als Voreinstellung, Abbruch nach 120 Sekunden | `depot_dienst/src/papierdialog.rs`; der echte Windows-Dialog benötigt weiterhin einen sichtbaren Praxistest nach normalem Start und Autostart |

Eine Sicherung auf einem anderen Datenträger und das Aufräumen unfertiger
Sicherungen nach einem Absturz bleiben spätere Verbesserungen. Sie werden hier
nicht als bereits umgesetzt ausgegeben. Automatisierte Windows-Prüfungen ersetzen
keinen sichtbaren Dialogtest im angemeldeten Benutzerkonto.

## #58: Verständliche Einrichtung und Rückmeldung

| Punkt | Ergebnis / Nachweis |
| --- | --- |
| U1 Arbeitsstand statt wiederholt klicken | Vorhandene Warteschlangen- und Doppelklicksperre; `journal_knopf_zeigt_arbeitsstand`; zusätzlich derselbe Arbeitsstand im aktualisierten Einrichtungsteil |
| U2 Bestätigung und fester Update-Commit | Vorhandene Tests `doppelklick_startet_nichts_doppelt`, `update_bestaetigung_bindet_den_gefundenen_commit`, `update_ohne_commit_startet_auch_bei_fehlerhaftem_warteschlangeneintrag_nicht` |
| U3 Fenster bleibt offen | Dauerhinweis in der Navigation und Fenstertitel in `DepotSchlopf.cmd` bereits vorhanden |
| U4 Fehler mit Rückweg | Vorhandene maskierte HTML-Fehlerseite mit Live- und Einrichtung-Link; Formularfehler mit begrenzten Meldungscodes |
| U5 Alltagssprache | Kurze Erklärung und Empfehlung je Hauptabschnitt; technische Update-Angaben bleiben aufklappbar; Schutzgrenzen verständlich erklärt |
| U6 Du statt Sie | Die zwei verbliebenen Anreden in Sperr-Meldungen korrigiert |
| U7 Status unterscheidbar | Bereitschaft trennt bereit, bitte erledigen, wartet und unbekannt; unbekannt ist grau, keine Änderung der fachlichen Zustände; `bereitschaft_trennt_handlung_warten_und_unbekannt` |
| U8 Wiederholung / Abhilfe | Dienst zeigt eine Gesamtzusammenfassung mit direkter Abhilfe statt einer zweiten Lagezeile; Stand bleibt sichtbar |
| U9 Reihenfolge | Journal direkt auf Einrichtung, danach Checkliste, Schlüssel, Quellen, Zeitplan, Autostart; aktive Sperren bleiben sichtbar; seltene Prüfungen unter Werkzeuge; `einrichten_bleibt_auf_der_config_seite_und_zeigt_den_arbeitsstand` |
| U10 Kleine Bildschirme | Schlüsselzeilen unter 600 px untereinander, Felder mit 100 % Breite und eigenem zugänglichem Namen; kein Abschneiden durch eine breite Tabelle |
| U11 Uhrzeit | Frankfurter Zeit und 24 Stunden direkt an jedem Uhrzeitfeld; natives Eingabeformat folgt weiterhin dem Browser |
| U12 Automatischer Papiertag | Direkt am Häkchen steht, dass es nur bei eingeschaltetem Zeitplan gilt |
| U13 Meldungen | Rücksprungziele für aufgehobene Sperren bleiben unter Werkzeuge vorhanden, keine doppelten IDs bei aktiver KI-Sperre. Erfolgsmeldung bleibt außerhalb der automatisch ersetzten Bereiche erhalten, ist als Status ausgezeichnet und bleibt auch beim Sprung zum Formular oben sichtbar |

Das Journal-Formular akzeptiert als Rückziel nur den festen Wert
`einrichtung`; fremde Werte werden niemals als Adresse verwendet. Die
Zustandsänderung bleibt ein POST mit gleichem Host, Herkunft und Start-Token.
Der neue Diensttest prüft auch zweimaliges Absenden und ein fremdes Rückziel.

Der persönliche Durchgang **Journal → Schlüssel → Zeitplan → Autostart** mit
Wolfgang und der sichtbare Windows-Dialogtest bleiben ehrliche Praxisprüfungen,
keine hier behaupteten Testergebnisse.
