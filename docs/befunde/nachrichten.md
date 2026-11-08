# Kostenlose RSS-Schlagzeilen (Stand 07.11.2026)

Der Sammler ersetzt die im Gratiszugang abgelehnte Alpha-Vantage-Premiumabfrage durch genau zwei feste, nur lesende RSS-Feeds:

- tagesschau: https://www.tagesschau.de/infoservices/alle-meldungen-100~rss2.xml
- BBC World: https://feeds.bbci.co.uk/news/world/rss.xml

Quellenliste: `crates/depot_sammler/src/rss_quellen.rs`, gegen Änderung und Entfernung durch die Festschreibung geschützt. Kein Schlüssel für Nachrichten, keine Artikelseiten, Bilder oder anderen Links werden abgerufen. RSS-Hinweise: https://www.tagesschau.de/infoservices/rssfeeds und https://support.bbc.co.uk/platform/feeds/NewsFeeds.htm . Die Feedformate wurden am 07.11.2026 über HTTPS abgerufen und mit dem Leser geprüft. Nutzung lokal und privat; Quellen und Originallinks bleiben zugeordnet.

## Ablauf und Grenzen

Ein GET je Quelle, bestehende 30-Sekunden-/1-MiB-Grenze und keine Weiterleitung. Vor dem GET wird ein Abrufversuch dauerhaft im Journal belegt. Derselbe Feed wird frühestens nach 90 Minuten erneut angefragt, auch nach einem Fehler, Doppelklick oder Neustart. Die tagesschau nennt im Feed TTL 90 Minuten. Eine rückwärts gestellte Uhr verkürzt die Pause nicht. Fehlende Journalbestätigung verhindert den Netzabruf.

Der XML-Leser quick-xml (ohne Zusatzfeatures; transitive Abhängigkeit memchr) prüft den ganzen Feed vor Verwendung. DTD/externe Entitäten, unvollständige Dokumente, doppelte Attribute, verschachtelte Schlagzeilen und fremde Dokumentwurzeln verwerfen den Feed. Doppelte Titel-, Link- oder Zeitfelder verwerfen nur den betroffenen Eintrag; er wird einmal als ausgelassen gezählt. Alle anderen Einträge bleiben nutzbar, und auch verworfene Einträge werden weiter auf hinterlegte Schlüssel geprüft. Tiefe maximal 24. Nur unverschachtelte Titel, HTTPS-Originallink der jeweiligen Quelle und eine ausdrücklich mit Zeitzone versehene Veröffentlichungszeit werden verwendet. Zukünftige, über 24 Stunden alte oder unvollständige Meldungen werden ausgelassen und gezählt. Beschreibungen und Volltexte gehen nicht an die KIs.

Vor jeder Ablage werden alle hinterlegten Schlüssel roh, XML-dekodiert (einschließlich ignorierter Felder/Attribute) und im fertigen Nachrichtentext gesucht. Bei einem Treffer bleibt keine Rohantwort oder Nachricht dieses Laufs liegen, auch nicht die erste Quelle nach einem Treffer in der zweiten. Die bereits belegten Abrufversuche enthalten keine Antwort und keinen Schlüssel. Ein normaler Ausfall einer Quelle lässt die andere verwendbar; das Protokoll nennt den Ausfall.

Die Originalfeeds liegen unter `daten/roh/tagesschau` bzw. `daten/roh/bbc-world`, unverändert mit SHA-256-Bezug im bestehenden Nachrichtenbeleg. Insgesamt höchstens 15 Meldungen je Lauf, abwechselnd aus beiden Quellen ausgewählt, maximal 400 Zeichen je Text mit erhaltenem Link. Abrufzeiten fallen im Journal nie zurück, auch wenn die zweite Quelle ältere Meldungen liefert. Bereits bekannte Meldungen werden nicht doppelt belegt. Bestehende Alpha-Vantage-Belege bleiben lesbar.

## Oberfläche und Automatik

Einrichtung → Datenquellen: eigener Prüfknopf. Der Verlauf zeigt Abrufpause, Fehler und neue/bekannte Meldungen. Bei jedem normalen Kursabruf wird RSS ebenfalls versucht, innerhalb derselben Abrufpause. Unter Zeitplan steht eine eigene empfohlene Option für zusätzliche Schlagzeilen: bei aktivem Zeitplan täglich 08 bis vor 23 Uhr Frankfurter Zeit, frühestens alle 90 Minuten, auch am Wochenende. Für alte Installationen bleibt sie aus. Ein gespeicherter Auftragsstart und der Journalvermerk begrenzen Wiederanläufe. Kurse und Papiertag haben Vorrang; Nachrichten lösen selbst keine KI-Anfragen oder Orders aus. Fehlerhafte Formulare behalten auch die neue Auswahl.

## Nachweise

`depot_sammler/tests/nachrichten.rs`: ohne Schlüssel, zwei feste GETs, Herkunft/Link/Zeit/Hash, dauerhafte Pause und Uhrsprung, gleichbleibende Belege nach Wiederabruf, globaler Sicherheitsabbruch im zweiten Feed einschließlich mehrfacher XML-Escapes, maskierte Netzfehler, Teilverfügbarkeit, gemeinsames Limit und beide Quellen, Trunkierung ohne Teilbelege, Abrufreihenfolge bei verschiedener Veröffentlichungsreihenfolge.

`depot_sammler/tests/rss.rs`: UTC-Umrechnung beider Offsetrichtungen, fehlende/ungültige Zeitzone, CDATA und Referenzen, zukünftige/alte/unvollständige Meldungen, DTD, Trunkierung, doppelte Felder/Attribute, falsche Verschachtelung, mehrere Wurzeln, Nullzeichen und fremde oder aktive Links.

`depot_dienst/tests/dienst.rs`: zusätzliche Abrufe nur nach Einschalten, auch am Wochenende, Pause über Dienstneustart, erneuter Abruf nach 90 Minuten; bestehende Kurs-/Papier- und Fehlerformulartests bleiben erhalten.

## Abgrenzung

Dies liefert zeitgestempelte Belege an das vorhandene Weltmodell. Automatische neue KI-Handelsentscheidungen tagsüber sowie zusätzliche KI-Aufrufe werden hier nicht aktiviert. Fehlende Nachrichten verhindern weiterhin keine ansonsten verwendbaren Kurse. Die Aussagekraft der Prognosen muss im Papierbetrieb gemessen werden.

## Korrektur: RSS-Verwaltung und offener Papiertag (08.11.2026)

Der bisherige Sammler schrieb seine Abrufpause als freie Beobachtung. Dadurch galt bereits ein RSS-Abruf als offener Papiertag und blockierte die Vorbereitung. Neue Versuche verwenden den eigenen Journaltyp `rss-abruf`. Nur exakt kanonische alte Beobachtungen mit `rss-abruf: `, einer der beiden festen Quellen und einem zum Belegdatum passenden Zeitpunkt werden ebenfalls als Verwaltung gelesen. Andere Beobachtungen zählen unverändert als Handelszustand. Keine Datei wird migriert, umgeschrieben oder zurückgesetzt.

Die letzte Abrufzeit je Quelle ist das Maximum der gültigen alten und neuen Belege. Neustart und rückwärts gestellte Uhr verkürzen die bestehende 90-Minuten-Pause nicht. Verwaltungsbelege löschen oder bestätigen keinen offenen Handelslauf.

Regressionen: `depot_journal/tests/rss_abruf.rs` prüft alte Dateien samt unveränderten Bytes und Vermerk, neue Belege nach Neustart sowie echte und nur ähnlich aussehende Beobachtungen. `depot_sammler/tests/nachrichten.rs` prüft weiterhin Abrufpause und Neustart. `depot_sim/tests/vorbereitung.rs` führt die Vorbereitung mit alten und neuen Verwaltungsbelegen durch, ohne Handel oder echte Anbieteraufrufe. Alle Beispieldaten sind synthetisch.

Kompatibilität: Bestehende Journale bleiben unverändert lesbar. Nach dem ersten neuen `rss-abruf`-Beleg können ältere Versionen das Journal nicht lesen; daher dann nicht manuell auf eine ältere Version zurückgehen. Die Updateprüfung und Bestätigung müssen weiterhin vor regulären Laufaufträgen erfolgen. Keine neue Quelle, Bibliothek oder finanzielle Grenze.

Nachprüfung #81: Der Leser des neuen Eintragstyps akzeptiert kanonische Quellkennungen aus 1 bis 32 ASCII-Kleinbuchstaben, Ziffern oder Bindestrichen, auch nach Entfernen einer Quelle. Nur der Sammler legt die erlaubten Netzquellen über seine festgeschriebene Liste fest; das Journal bewahrt historische Verwaltungsbelege. Für freie alte Beobachtungen bleiben ausschließlich die zwei historisch reservierten Quellkennungen erkennbar. RSS-Verwaltungszeilen gehören nicht mehr zu fachlichen Beobachtungen. Belege über einen Tag in der Zukunft melden ausdrücklich „Uhr oder Journal prüfen“ und erlauben keinen Abruf.
