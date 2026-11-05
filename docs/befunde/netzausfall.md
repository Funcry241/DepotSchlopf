# Kurze Netzausfälle beim Kursabruf

Maus' erster von drei Punkten am 05.11.2026 (Wolfgang hat ihn weitergegeben): Scheitert ein Kursabruf an der Verbindung, soll DepotSchlopf nach kurzer Wartezeit erneut versuchen, nur bei vorübergehenden Fehlern, mit wenigen Versuchen und innerhalb des bestehenden Frischefensters. Die Live-Ansicht sagt, was los ist; ist das Zeitfenster vorbei, sagt sie, warum der Papiertag ausfiel.

## Befund vorher

- Ein Abruf, der an der Verbindung scheiterte, wurde als Fehler dieses Werts vermerkt. Der Lauf rief die übrigen Werte trotzdem ab, jeder scheiterte für sich. War ein gehaltener Wert darunter, war der Tag verloren, obwohl die Verbindung oft nach einer Minute wieder da ist (Router-Neustart).
- Die Live-Ansicht zeigte nur das rohe Protokoll; dass die Verbindung fehlte, stand in der Fehlerzeile eines Werts.

## Regel

| Fehler | Wiederholt? |
|---|---|
| Name nicht auflösbar, Verbindung nicht aufgebaut oder abgebrochen, Zeitgrenze, stockende Antwort, Status 5xx | ja, vorübergehend |
| Status 4xx (auch 429), zu große Antwort, Zertifikat, Weiterleitung, Hinweis der Quelle (Kontingent) | nie |

- Höchstens vier Versuche je Abruf, mit 30, 60 und 120 Sekunden Wartezeit dazwischen (`netz::WARTEZEITEN`).
- Ein weiterer Versuch beginnt nur, wenn er spätestens zur Frist beginnt: Schluss der Börse am letzten Handelstag plus Kurs-Höchstalter (20 Minuten) minus Zeitgrenze eines Abrufs (30 Sekunden), für Xetra also 17:54:30 Uhr (`netz::frist`). So liegt auch ein wiederholter Abruf im bestehenden Frischefenster. Das Höchstalter selbst ändert sich nicht.
- Abends, nach der Frist, gibt es keinen zweiten Versuch: Die Kurse wären ohnehin zu alt.
- Unmittelbar vor jedem weiteren Versuch fragt der Sammler die Uhr noch einmal. Dauerte das Warten länger als geplant (Ruhezustand, gestellte Uhr) und liegt der Versuch nach der Frist, unterbleibt er (Maus, Befund 2).
- Bleibt die Verbindung zu einer Quelle weg (`Bericht::verbindung`), ruft der Lauf bei dieser Quelle keinen weiteren Wert ab; das schont ihr Tageskontingent. Die anderen Quellen fragt er weiter (Maus, Befund 1).
- Die Referenzkurse der EZB folgen derselben Regel, mit der Frist der frühesten Börse, deren Werte sie brauchen (nicht in Euro). Fallen sie aus, fehlen nur die Werte in Dollar; die Werte in Euro ruft der Lauf trotzdem ab. Die Termine der EZB werden nicht wiederholt; sie hängen an keinem Frischefenster.
- Die Regel steckt in einer Funktion (`netz::hole_mit_wiederholung`), die jeder Abruf des Sammlers nimmt, gleich welche Quelle.

## Anzeige

Der Sammler schreibt Hinweiszeilen in Alltagssprache (`depot_kern::hinweis`, Anfang `[Hinweis] `). Der Dienst zeigt die jüngste davon groß unter „Tätigkeit“, während der Lauf läuft und am selben Tag danach, und in der Zeile des Verlaufs.

| Lage | Hinweis |
|---|---|
| Verbindung weg | „Verbindung weg. Nächster Versuch um 17:45 Uhr (Versuch 2 von 4). Der Abruf versucht es von selbst noch einmal, du musst nichts tun.“ |
| wieder da | „Verbindung wieder da (Versuch 3). Der Abruf läuft weiter.“ |
| Frist vorbei | „Papiertag heute ausgefallen. Die Verbindung kam bis 17:54 Uhr nicht wieder. Später geholte Schlusskurse wären älter als 20 Minuten. Mit den Kursen dieses Laufs kann heute kein Papiertag laufen. Bitte die Internetverbindung prüfen.“ |

Wie es danach weitergeht, weiß nur der Dienst: Er hängt an einen gescheiterten Lauf einen Satz aus seinem tatsächlichen Zeitplan an, etwa „Nach Zeitplan geht es von selbst weiter: Kurse abrufen am Fr 06.11.2026 um 17:45“ oder, ist der Zeitplan aus, „DepotSchlopf startet nichts von selbst“ (Maus, Befund 3). Der Sammler verspricht nichts, was er nicht weiß.

Der Hinweis entscheidet nichts. Ob der Papiertag läuft, entscheidet weiter der Ausgang des Sammlers (nur verwendbare Kurse) und die Kursprüfung im Papiertag.

## Nachweis

`crates/depot_sammler/tests/wiederholung.rs`:

- `router_neustart_waehrend_des_kursabrufs` (Abnahmefall 1): zwei Abrufe scheitern, der dritte gelingt; Kurse belegt und verwendbar, Hinweise mit Uhrzeit.
- `verbindung_bleibt_weg_bis_zum_ende_der_frist`: kein Versuch nach 17:54:30 Uhr, bei dieser Quelle nichts mehr abgerufen, nichts verwendbar, Hinweis nennt den Grund.
- `nach_langem_warten_kein_versuch_nach_der_frist`: das Warten dauert fünf Minuten länger, der Versuch nach der Frist geht nicht hinaus.
- `hoechstens_vier_versuche`, `ablehnung_wird_nicht_wiederholt`, `nach_der_frist_kein_zweiter_versuch`, `frist_aus_schluss_und_hoechstalter`.

`crates/depot_sammler/tests/tiingo.rs`: `ohne_referenzkurse_kein_dollarwert` (auch mit Status 503 der EZB: Xetra wird abgerufen und ist verwendbar).

`crates/depot_dienst/tests/dienst.rs`: `hinweis_des_laufs_steht_gross_in_der_live_ansicht`, `nach_einem_ausfall_sagt_der_zeitplan_wie_es_weitergeht`.

Für den Test unter Windows: Während `run.cmd kurse` (oder dem Zeitplan um 17:45 Uhr) den Router neu starten. Die Live-Ansicht zeigt „Verbindung weg. Nächster Versuch um …“ und danach „wieder da“; ist die Verbindung bis 17:54 Uhr nicht zurück, steht dort, warum der Papiertag ausfällt.
