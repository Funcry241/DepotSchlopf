# Update mit Rückfall

Wolfgang am 05.11.2026: ein geprüfter Update-Ablauf mit Rückfall, ohne Konsole. Maus: Updates wie Schlüssel und Konfiguration gegen fremde Webseiten schützen. Dieser Schritt baut auf dem Dienst auf (`docs/befunde/dienst.md`).

## Ablauf

1. **Prüfen** (Knopf „Nach Update suchen“ oder `run.cmd update pruefen`): `git fetch origin main`, dann die neuen Stände `alt..neu` als Liste. Am Projektordner ändert sich nichts.
2. **Einspielen** (Knopf „Update einspielen“ oder `run.cmd update`):
   - Vorbedingungen: Projektordner auf dem Zweig `main`, keine verfolgte Datei lokal geändert, keine `daten/*.sperre`, kein unbestätigtes Update (`daten/update_offen.txt`).
   - Der lokale Stand muss Vorgänger des neuen sein (`merge-base --is-ancestor`). Eigene lokale Commits blockieren das Update, statt überschrieben zu werden.
   - Ein Stand aus `daten/update_abgelehnt.txt` wird nie wieder eingespielt; erst ein neuerer Stand auf `main`.
   - Der neue Stand wird Byte für Byte aus seinen Blobs in eine eigene Kopie außerhalb des Projektordners geschrieben (wie beim Gate von `run.cmd`, mit eigenem Build-Ordner `bau-update`). Dort läuft das ganze Gate (`--voll`, Basis ist der Vorgänger des neuen Stands, wie die Prüfung von `main` auf GitHub). Danach muss die Kopie unverändert sein.
   - Nur dann: Vorbedingungen und `HEAD` noch einmal prüfen, `daten/update_offen.txt` schreiben (`update-v1`, `vorher=`, `neu=`), `git merge --ff-only`. Scheitert das Vorrücken, verschwindet die Markierung wieder.
3. **Neustart:** Nach dem erfolgreichen Auftrag endet der Dienst mit Code 3, sobald kein anderer Auftrag mehr läuft oder wartet. Ein eingereihter Lauf geht also nicht verloren.
4. **Wächter** (`depot_waechter`, gerufen von `DepotSchlopf.cmd`): baut den Dienst, startet ihn und wartet.
   - Liegt beim Start `daten/update_offen.txt`, ist der neue Stand **auf Probe**. Er gilt erst als hochgekommen, wenn er die Markierung selbst löscht (Schritt 5).
   - Bau scheitert, der Dienst endet vor der Bestätigung (mit jedem Code, auch 0) oder bestätigt sich nicht binnen 120 Sekunden: Der Wächter beendet ihn und macht den **Rückfall.** `git reset --keep vorher`, den neuen Stand in `daten/update_abgelehnt.txt` vermerken, Markierung löschen, alten Stand bauen und starten. Das braucht nur Git, keinen Code des neuen Stands.
   - Steht der Projektordner weder auf `vorher` noch auf `neu`, oder ist die Markierung unlesbar, fasst der Rückfall nichts an und meldet es.
   - Nach der Bestätigung: Code 3 mit neuer Markierung heißt, ein weiteres Update ist eingespielt; neu bauen, ohne Browser neu starten.
5. **Bestätigen:** Läuft der neue Dienst 15 Sekunden, löscht er die Markierung („Update bestätigt“). Erst dann ist ein weiteres Update möglich.
6. **Seite:** Jede Antwort trägt eine nicht geheime Kennung des Dienststarts (`data-start`). Antwortet nach der Unterbrechung ein neuer Start, lädt `/app.js` die Seite neu, damit die Formulare das Token des neuen Starts tragen. Während der Pause zeigt der Hinweis „Verbindung getrennt, etwa beim Neustart nach einem Update“.

Das Update läuft nur allein: Es lässt sich nur einreihen, wenn nichts läuft oder wartet, und nur einmal je Dienststart. Der Knopf ist ein POST wie jeder andere: eigener Host, eigene Herkunft, Token dieses Starts.

## Warum ein Wächter statt einer Schleife in `DepotSchlopf.cmd`

`cmd` liest eine Befehlsdatei während des Laufs Stück für Stück nach. Ein Update, das `DepotSchlopf.cmd` ändert, könnte eine laufende Schleife mitten in einer Zeile weiterlesen lassen. Deshalb steht in `DepotSchlopf.cmd` und `run.cmd` alles nach der Prüfung auf Rust in einem Klammerblock, den `cmd` einmal ganz liest, und die Schleife liegt im Wächter, einem Rust-Programm mit denselben strengen Lints. Der Wächter baut nur `--bin depot_dienst` und startet die gebaute Datei direkt, nicht über `cargo run`; so beendet er bei abgelaufener Frist wirklich den Dienst. Seine eigene, laufende Datei bleibt unberührt und wird erst beim nächsten Start neu gebaut.

## Netz

`git fetch` holt den Stand von `origin` (GitHub) über Git als eigenen Prozess. Kein Rust-Code des Projekts öffnet dafür eine Verbindung; `depot_lauf` bleibt ohne Netz-Crate. Der Wächter belegt nur kurz `127.0.0.1:47410`, um zu sehen, ob der Dienst schon läuft (wörtlich nach `NETZ_LOKAL`).

## Grenzen

- Das Gate prüft den neuen Stand vollständig und die Grenzfreigabe seines letzten Schritts. Die Freigaben früherer Stände dazwischen hat die Prüfung auf `main` (GitHub) je Stand geprüft; lokal wird sie nicht wiederholt.
- Ein Fehler, der erst nach der Bestätigung (15 Sekunden) auftritt, löst keinen Rückfall mehr aus. Dann bleibt der neue Stand, und der Wächter endet mit Fehler.
- Ändert ein Update den Wächter selbst, gilt das ab dem nächsten Start von `DepotSchlopf.cmd`.

## Tests

- `crates/depot_lauf/src/update.rs` mit echten Git-Ordnern (Ursprung, Projektordner, zweiter Klon): nichts Neues; neuer Stand genannt, Prüfen ändert nichts, Einspielen nur vorwärts mit Markierung; offenes Update, lokale Änderung, fremder Zweig, Sperrdatei blockieren; eigener lokaler Commit wird nicht überschrieben; abgelehnter Stand kommt nicht wieder, ein neuerer schon.
- `crates/depot_dienst/tests/update.rs`: Update nur allein, Neustart erst wenn nichts mehr wartet, nur einmal je Start; Knöpfe und Kennung des Starts auf jeder Seite; Markierung streng gelesen; Rückfall setzt zurück und lehnt ab, lokale Dateien bleiben; fremder Stand und unlesbare Markierung werden nicht angefasst; Bestätigen nur auf dem neuen Stand.

**Gegenproben** (jeweils rot): Update-Sperre in `einreihen` aus; Neustart trotz wartender Aufträge; Rückfall ohne Prüfung des Stands; ohne Vorgänger-Prüfung; ohne Ablehnungsliste; ohne Prüfung lokaler Änderungen.

**Probelauf unter Linux** in einer Kopie des Projekts:
- Ein nicht baubarer neuer Stand mit offener Markierung: Der Wächter setzt auf den alten Stand zurück, vermerkt den neuen als abgelehnt und startet den alten Dienst.
- Ein guter Stand: Der Dienst zeigt „läuft zur Probe“, nach 15 Sekunden „bestätigt“.
- Ein Klick auf „Update einspielen“ im Browser (Einspielen durch eine Attrappe ersetzt, um das Gate zu sparen): Der Dienst endet mit Code 3, der Wächter startet ihn neu, die Seite lädt sich selbst neu und trägt das neue Token.

## Fassung 2 nach Maus' Prüfung

**Befund:** Hing der neue Dienst beim Start, wartete der Wächter unbegrenzt. Endete er mit Code 0 vor der Bestätigung, meldete der Wächter Erfolg, und der unbestätigte Stand blieb aktiv.

**Regel:** Ein Stand auf Probe muss sich selbst bestätigen. Jedes Ende davor fällt zurück, gleich mit welchem Code, und nach 120 Sekunden (`waechter::FRIST`) beendet der Wächter ihn und fällt ebenso zurück. Die Regel steht in `depot_dienst::waechter::begleite`; der Wächter startet die gebaute Datei direkt, damit das Beenden den Dienst trifft und nicht nur Cargo.

**Gegenproben** (jeweils rot): frühes Ende als gewöhnliches Ende gewertet (`ende_vor_der_bestaetigung_faellt_zurueck_auch_mit_code_0`); ohne Frist (`haengender_neuer_stand_wird_nach_der_frist_beendet`).

**Probelauf unter Linux:** Ein neuer Stand, der sofort mit 0 endet, und einer, der beim Start hängt, wurden beide zurückgesetzt und abgelehnt; danach lief der alte Dienst. Der gewöhnliche Weg (Update, Code 3, Neustart, Bestätigung) läuft unverändert.

## Oberflächenupdate: Abschluss, Windows und Browser

- Der Wächter baut außerhalb seines ursprünglichen Bauordners. Zwei wiederverwendbare Plätze verhindern das Überschreiben eines laufenden Wächterprogramms, ohne pro Commit einen neuen vollständigen Cargo-Bauordner anzulegen. Der zweite Platz wird genutzt, wenn der Wächter selbst aus dem ersten läuft. Sicherung und Prüfung des Programmbaums bleiben unverändert.
- `aktualisierung` speichert die letzten 20 Abschlussbelege mit vorherigem/neuem Commit, Auftragskennung, Zeitpunkt, Phase und bereinigtem Grund. Ein erfolgreicher Einspielprozess ist noch kein bestätigtes Update. Tätigkeit und dynamischer Update-Bereich zeigen den Abschluss. Auch das bisherige Verlaufsformat und das bisherige Protokoll bekommen den endgültigen Ausgang, damit ein älterer Rückfallstand den vorläufigen Einspielerfolg nicht weiter grün zeigt.
- Ein fehlender oder fremder Abschlussbeleg wird niemals als bestätigte Aktivierung angezeigt. Ein Schreibfehler verhindert die Bestätigung, aber niemals den Code-Rückfall. Unlesbare Abschlussbelege werden als Diagnosefehler gezeigt. Bestätigung und Rückfall ändern keine Papierdaten.
- `/teil/status` bleibt der abwärtskompatible Verbindungspfad. Neue Stände liefern zusätzlich ihre Bereitschaft nach der Probezeit. Ein alter Rückfallstand ohne Bereitschaftsmarker bleibt erreichbar. Der Browser prüft auch auf Seiten ohne Live-Teilbereiche, bricht Anfrage und Antwortkörper nach fünf Sekunden ab, versucht es erneut und navigiert zentral genau einmal. Er erhält Anker und fachliche Parameter und entfernt nur die vorläufige Meldung `m`. Teilantworten eines neuen Starts werden vor der zentralen Neuladung nicht in die alte Seite eingefügt. Seitenspeicher-Rückkehr startet einen neuen Prüfzyklus; alte Anfragen und Timer werden beim Verlassen beendet.
- Native Nachweise: Update einspielen → noch offen → Dienstwechsel → Bestätigung oder Rückfall → erneuter Start mit korrekter Tätigkeit und Update-Anzeige; Fehler beim Abschlussbeleg; Bereitschaft und Startkennung auf Bestätigungs-/Protokollseiten. Der Windows-Test hält eine echte EXE als ursprünglichen Wächter offen und baut mit dem Produktionsbefehl alle vier Ziele eines kleinen paketfreien Testprojekts, ohne die laufende Datei zu berühren. Acht ausführbare JavaScript-Verhaltensprüfungen simulieren Ausfälle, hängende Antwortkörper, Probezeit, Rückfall auf alte Dienste, Seitenspeicher und parallele Teilantworten. Ein Rust-Integrationstest führt sie als Teil des Gates aus; in CI ist Node dafür Pflicht, außerhalb von CI wird ein fehlendes Node ausdrücklich als ausgelassene Browser-Prüfung gemeldet. Das Programm benötigt Node nicht.
- Übergangsgrenze: Der bereits laufende alte Wächter und das bereits geladene alte Browser-Skript können nicht rückwirkend repariert werden. Ein Windows-Dateilock beim ersten Wechsel kann einen einmaligen Neustart mit dem neuen Quellstand erfordern. Der konkrete Fehler beim Nutzer bleibt ohne dessen Update-Protokoll unbewiesen. Es gibt keine neue Netzwerkquelle, Abhängigkeit oder Handelsgrenze.

## Selbstgegenprüfung nach #79

Zwei zusätzliche Gegenproben scheiterten auf dem gemergten Stand und bestehen nach der Korrektur:

1. Bei freiem Dienst-Port konnte ein zweiter Wächter während Bau oder Neustart starten. Ein Fehler dieses konkurrierenden Starts konnte den Stand des ersten Wächters zurücksetzen. Der Wächter hält jetzt eine exklusive Betriebssystem-Dateisperre über seine gesamte Laufzeit, einschließlich Bau, Probe, Rückfall und Neustart. Sie liegt im temporären Ordner und gilt auch für verschiedene Projektkopien desselben Benutzers. Ein zweiter Aufruf baut nichts und setzt nichts zurück. Nur eine tatsächlich belegte Sperre bedeutet „läuft oder startet bereits“; Zugriffsfehler brechen den Start mit Fehler ab. Die Datei wird nicht gelöscht, und der Kernel gibt die Sperre bei Prozessende frei. Gegenprobe: echte Wächter-Programmdatei bei freiem Port und bereits gehaltener Sperre starten; Commit, offene Markierung und Ablehnungsliste bleiben unverändert. Nach Freigabe kann wieder genau ein Besitzer übernehmen.
2. 2.048 Zeichen je Grund konnten nach JSON-Escaping mehr als 128 KiB für zwanzig Belege ergeben. Der Schreiber erzeugte dadurch eine Datei, die der Leser selbst ablehnte. Jetzt gelten beim Schreiben dieselben Byte-Grenzen wie beim Lesen. Älteste Belege werden nötigenfalls entfernt; der neueste bleibt erhalten. Gegenprobe: 25 lange Gründe mit JSON-Steuerzeichen, nach jedem Schreiben erneut laden und Dateigröße prüfen.

Das ergänzt die bestehenden Browser- und Windows-Prüfungen. Keine Netzwerkquelle, Abhängigkeit oder Handelsgrenze wurde geändert. Ein schon vor dieser Korrektur laufender Wächter kennt die neue Prozesssperre noch nicht. Nach dem ersten Einspielen muss er einmal über `DepotSchlopf.cmd` neu gestartet werden.
