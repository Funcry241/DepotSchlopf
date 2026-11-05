# Dienst mit Oberfläche

Wolfgang am 05.11.2026: DepotSchlopf läuft dauerhaft, startet mit Windows und wird über eine lokale Weboberfläche mit genau einer Config-Seite eingerichtet, ohne Konsole. Maus: Einrichtung, Live-Status und Updateablauf zusammen benutzbar; Speichern und Aktivieren sichtbar unterscheiden; Schlüssel, Konfiguration und Updates gegen Aufrufe fremder Webseiten schützen (Host- und Herkunftsprüfung, Token gegen CSRF und DNS-Rebinding). Freigabe Wolfgang: „Ja, nur 127.0.0.1“, eigener kleiner Server aus der Standardbibliothek.

Dieser Schritt liefert Dienst, Live-Seite, Config-Seite, Zeitplan, Starter und Autostart. Das Update mit Rückfall folgt als eigener Schritt.

## Grundsatz: Der Dienst schreibt nie ins Journal

Jede Arbeit ist ein eigener Lauf wie `run.cmd kurse`, `run.cmd termine` oder `run.cmd papier` (`cargo run -p depot_lauf -- …` im Projektordner). Der Dienst startet ihn, liest Ausgabe und Fehlerausgabe ins Protokoll und merkt sich das Ergebnis. Was ein Lauf darf, entscheidet weiter nur der Lauf; der Dienst fügt keinen Weg ins Journal hinzu.

- **Ein Auftrag zur Zeit.** Dieselbe Art läuft oder wartet nie doppelt, höchstens vier warten.
- **Eindeutige Kennung** je Auftrag aus Startzeit in Sekunden und laufender Nummer.
- **Das Lagebild liest nur, wenn kein Auftrag läuft**, und nur im selben Arbeiter, der Aufträge startet. Lagebild und Lauf greifen nie zugleich auf das Journal zu. Das Lagebild ist dasselbe wie `run.cmd lage` (`depot_lage`), ergänzt um den Stand der Schlüssel.

## Zeitplan, einmal je Tag

Montag bis Freitag, Frankfurter Zeit: Termine 17:40, Kurse 17:45 (nach dem Xetra-Schluss um 17:35), nach erfolgreichem Kursabruf der Papiertag. Uhrzeiten und Schalter stehen in `daten/zeitplan.txt`.

- Bevor der Zeitplan einen Auftrag einreiht, steht der Tag in `daten/dienst_erledigt.txt` (Nebendatei, auf den Datenträger, dann umbenennen). Ein Neustart startet an diesem Tag nichts noch einmal. Lässt sich die Datei nicht schreiben, schaltet sich der Zeitplan aus und zeigt den Grund.
- Ist `zeitplan.txt` oder `dienst_erledigt.txt` nicht genau in der eigenen Form, ist der Zeitplan aus und die Seite nennt den Grund. Nichts startet dann von selbst.
- Der Papiertag folgt nur auf einen erfolgreichen Kursabruf des Zeitplans; er selbst läuft ohnehin nur einmal je Handelstag (Journal).

## Schutz gegen fremde Webseiten

Der Dienst lauscht nur auf `127.0.0.1:47410` (Kodex `NETZ_LOKAL`: jedes `bind` nur als direkter Aufruf mit `(::std::net::Ipv4Addr::LOCALHOST, PORT)`, kein Verbindungsaufbau, keine Namensauflösung). Ein Browser kann trotzdem von einer fremden Seite aus Anfragen an 127.0.0.1 schicken. Darum:

| Angriff | Abwehr |
|---|---|
| DNS-Rebinding (fremder Name zeigt auf 127.0.0.1) | jede Anfrage braucht `Host: 127.0.0.1:47410` oder `localhost:47410`, sonst 403 |
| CSRF (fremde Seite schickt ein Formular) | jede Änderung (POST) braucht `Origin` der eigenen Seite und das Feld `t` mit dem Zufallswert dieses Dienststarts (Vergleich ohne frühen Ausstieg), sonst 403; nur `application/x-www-form-urlencoded` |
| Einbetten, fremde Skripte, Zwischenspeicher | `Content-Security-Policy` (`default-src 'none'`, nur eigenes Skript, `frame-ancestors 'none'`, `form-action 'self'`), `X-Frame-Options: DENY`, `nosniff`, `Cache-Control: no-store` |
| Gespiegelte Eingaben | Meldungen nach einem Formular sind feste Texte zu festen Kennungen; alles andere wird nicht angezeigt. Jeder Text ist maskiert |
| Unklare Anfragen | HTTP streng gelesen: Kopf und Rumpf je höchstens 16 KiB, kein `Transfer-Encoding`, genau eine Längenangabe, Rumpf genau so lang; jede Verbindung eine Anfrage; höchstens 32 Verbindungen zugleich, je 10 s Zeit |

`Referrer-Policy: same-origin` statt `no-referrer`: Mit `no-referrer` senden Browser bei Formularen `Origin: null`, und die eigene Seite würde abgelehnt.

## Schlüssel

Die Config-Seite nimmt einen Schlüssel als Passwortfeld entgegen, prüft ihn nach derselben Regel wie der Lauf, der ihn liest, und legt ihn direkt in der Windows-Anmeldeinformationsverwaltung ab (Benutzer `depotschlopf`):

| Wofür | Adresse | Regel |
|---|---|---|
| Kursquelle | `DepotSchlopf/alphavantage` | 8 bis 64 Großbuchstaben und Ziffern |
| Dachs | `DepotSchlopf/anthropic` | 20 bis 256 Zeichen aus `A-Z a-z 0-9 _ -` |
| Maus | `DepotSchlopf/openai` | 20 bis 256 Zeichen aus `A-Z a-z 0-9 _ -` |

Der Dienst liest nie einen Schlüssel aus. Für den Stand fragt er nur die Merkmale des Eintrags ab („hinterlegt“, „fehlt“). Kein Schlüssel steht in einer Antwort, einer Meldung, einer Datei, dem Protokoll oder dem Export.

## Speichern und Aktivieren

| Einstellung | Gespeichert | Aktiv | Änderung |
|---|---|---|---|
| Schlüssel | Anmeldeinformationsverwaltung | ab dem nächsten Lauf | Config-Seite |
| Zeitplan | `daten/zeitplan.txt` | sofort beim Speichern; die Seite zeigt „gespeichert und aktiv“, „Voreinstellung, nicht gespeichert“ oder dass die Datei abweicht | Config-Seite |
| Autostart | Autostart-Ordner des Benutzers | ab dem nächsten Windows-Start | Config-Seite |
| Grenzen und Budgets | Festschreibung | immer | nur über Grenzfreigabe, nie über die Seite |

## Start und Autostart

- `DepotSchlopf.cmd` (Doppelklick) startet den Dienst und öffnet die Seite. Ist der Port belegt, läuft der Dienst schon: Dann öffnet sich nur die Seite.
- Autostart legt `DepotSchlopf.cmd` im Autostart-Ordner des Benutzers an, die den Starter im Projektordner mit `--ohne-browser` in kleinem Fenster ruft. Keine Registry, keine Adminrechte. Ein Projektordner mit `" % ! ^ & | < >` oder Steuerzeichen wird abgelehnt, damit die Befehlsdatei nichts anderes tun kann.

## Live-Seite

Tätigkeit (laufender Auftrag mit Protokoll, Warteschlange, nächster geplanter Lauf, letzte 20 Aufträge mit Protokoll), darunter das Lagebild mit Bereitschaft und Knöpfen (Journal einrichten, Kurse, Termine, Papiertag), KI-Budget je KI, Papierdepot, Gespräch von Dachs und Maus und Grenzen. `/app.js` lädt den Inhalt alle zwei Sekunden neu, lässt aufgeklappte Protokolle offen und zeigt einen Hinweis, wenn der Dienst nicht antwortet; danach versucht es weiter, bis er wieder da ist.

Der Verbindungstest der Quellen ist der Abruf selbst (nur lesend); sein Ergebnis steht unter Tätigkeit und in der Einrichtung.

## Tests

17 Tests in `crates/depot_dienst/tests/dienst.rs`, ohne Netz und ohne echten Schlüsselspeicher (eine Attrappe nimmt Schlüssel und Autostart an):

- HTTP: genau eine Anfrage, Ablehnung von `Transfer-Encoding`, doppelter oder falscher Länge, fremdem Ziel, anderer Version, zu großem Kopf oder Rumpf, abgebrochener Verbindung; strenges Formular.
- Schutz: fremder oder fehlender Host (auch `localhost.boese.example:47410`, falscher Port) → 403; POST ohne oder mit fremder Herkunft (auch `null`), falschem Typ, fehlendem oder falschem Token → abgelehnt, nichts eingereiht; Sicherheitsköpfe; keine gespiegelte Meldung.
- Schlüssel: Regel geprüft, abgelegt ohne Leerraum, nie in Antwort oder Seite.
- Zeitplan: Text streng, fällig erst ab Uhrzeit, Termine vor Kursen, nur Montag bis Freitag, nächster Lauf über das Wochenende; Termine, Kurse und danach Papier; kein Papiertag nach gescheitertem Kursabruf; Neustart startet heute nichts noch einmal; unlesbarer Zeitplan oder unlesbares Erledigt schaltet ihn aus; Speichern über die Seite.
- Werkbank: Auftrag mit Protokoll und Kennung, kein Lagebild während eines Auftrags.
- Autostart: nur mit harmlosem Pfad.

Der Server selbst (Lauschen, Fäden, Browser) steht in `main.rs` und ist dünn; er ruft nur `http::lies` und `behandle`. Probelauf unter Linux: Seite 200, fremder Host 403, Auftrag über das Formular eingereiht und gestartet.

## Gegenproben

Jede Schutzregel ausgeschaltet macht mindestens einen Test rot:

| Ausgeschaltet | Rot |
|---|---|
| Host-Prüfung | `fremder_host_wird_abgelehnt` |
| Herkunftsprüfung | `post_braucht_herkunft_formular_und_token` |
| Token-Prüfung | `post_braucht_herkunft_formular_und_token` |
| Erledigt vor dem Start festhalten | `zeitplan_startet_termine_kurse_und_danach_papier` |
| Papiertag nur nach erfolgreichem Kursabruf mit `papier_danach` | `kein_papiertag_nach_gescheitertem_kursabruf` |
