# Nach einem Neustart: Papiertag nachholen oder schützen

Maus' zweiter von drei Punkten am 05.11.2026, zweiter Teil. Der erste Teil startet den geprüften Programmstand ohne Cargo (`neustart.md`). Hier geht es darum, was der Dienst nach dem Neustart mit dem angefangenen Tag macht und was die Oberfläche dazu sagt. Er baut auf dem Zeitplan je Börse auf (`boersen.md`, Schritt 4).

## Befund vorher

- Ob heute ein Kursabruf des Zeitplans gelungen war, stand nur im Speicher (`Zustand::kurse_erfolg`). Der Papiertag wurde am Ende des letzten Abrufs eingereiht. Endete der Dienst dazwischen, etwa bei einem Windows-Neustart, war beides weg. Der neue Dienst startete den Papiertag nicht mehr, obwohl die Kurse von heute galten.
- Ob der Zeitplan den Papiertag heute schon gestartet hatte, stand nirgends. Den Schutz gegen einen zweiten Start gab nur das Journal: Ein angefangener Tag hält jeden weiteren Papiertag an.
- Die Oberfläche zeigte einen unterbrochenen Lauf im Verlauf, sagte aber nicht, was daraus folgt.

## Regel

`daten/dienst_erledigt.txt` hat jetzt die Form `erledigt-v3` mit zwei weiteren Tagen:

- `kurse_ok`: der Tag, an dem zuletzt ein Kursabruf des Zeitplans erfolgreich endete. Der Dienst schreibt ihn beim Ende des Abrufs.
- `papier`: der Tag, für den der Zeitplan den Papiertag gestartet hat. Der Dienst schreibt ihn unmittelbar vor dem Start, nicht schon beim Einreihen.

Der Papiertag ist fällig (`zeitplan::faellig_fuer`), wenn alle vier Bedingungen gelten:

- „Papiertag nach den Kursen“ ist eingestellt.
- Alle Kursabrufe des Tages sind gestartet.
- `kurse_ok` ist heute.
- `papier` ist nicht heute.

Das ist ein Zustand, kein Ereignis. Darum gilt dieselbe Regel nach einem Neustart. Ein Papiertag, der heute noch nicht begonnen hat, wird mit den Kursen von heute nachgeholt. Ein begonnener wird nie ein zweites Mal von selbst gestartet. Ob die Kurse noch taugen, prüft der Papiertag wie immer selbst: Kurse von heute, Alter am Abruf gemessen. Ein angefangener Tag im Journal hält ihn an. Nach Mitternacht gilt der neue Tag, und nichts wird nachgeholt.

Die frühere Form `erledigt-v2` (und `-v1`) wird gelesen, mit leerem `kurse_ok` und `papier`. Am Tag des Umstiegs folgt deshalb kein Papiertag aus einem Abruf, der vor dem Umstieg lief. Dass er gelang, stand nirgends.

## Anzeige

Unter „Tätigkeit“ steht nach einem Neustart groß, wo der Tag steht (`werkbank::nach_neustart`, `id="neustart"`):

| Lage | Text (gekürzt) | muss Wolfgang etwas tun? |
|---|---|---|
| Neustart zwischen Kursabruf und Papiertag | „Nach dem Neustart holt DepotSchlopf den Papiertag mit den Kursen von heute nach. Du musst nichts tun.“ Danach: „… hat … nachgeholt.“ | nein |
| nachgeholter Papiertag hielt an | „… wollte … nachholen, er hielt an. Den Grund zeigt sein Protokoll im Verlauf.“ | hinsehen |
| Neustart während des Papiertags | „Der Papiertag lief schon, als DepotSchlopf endete … Der heutige Schutz bleibt … bis du ihn geprüft hast. Was zu tun ist, steht im Lagebild unter ‚Papiertag‘.“ | ja, prüfen |
| Neustart während des Kursabrufs | „Der Kursabruf lief, als DepotSchlopf endete. Ohne Kurse von heute folgt heute kein Papiertag von selbst. Du musst nichts tun; morgen geht es nach Zeitplan weiter.“ | nein |

Ohne Neustart steht dort nichts.

## Nachweis

`crates/depot_dienst/tests/dienst.rs`:

- `neustart_zwischen_kursabruf_und_papiertag_holt_ihn_nach` (Abnahmefall 2): Der Papiertag wartet, der Dienst endet. Der neue Dienst startet ihn um 18:20 Uhr und sagt das. Ein weiterer Neustart startet nichts mehr.
- `neustart_waehrend_des_papiertags_startet_ihn_nicht_neu`
- `neustart_waehrend_des_kursabrufs_ohne_papiertag`
- `erledigt_frueherer_form`: v1 und v2 werden gelesen, ohne Papiertag aus der alten Form.
- `zeitplan_startet_termine_kurse_und_danach_papier`, `papiertag_nach_dem_letzten_abruf_des_tages`: Ablauf ohne Neustart wie bisher.

Test unter Windows (Abnahmefall 2): Den Zeitplan mit „Papiertag nach den Kursen“ einschalten. Nach dem Kursabruf um 17:45 Uhr und bevor der Papiertag beginnt, Windows neu starten. Am leichtesten geht das, solange ein Auftrag von Hand läuft und der Papiertag dahinter wartet. Nach dem Start zeigt die Live-Ansicht „Nach dem Neustart holt DepotSchlopf den Papiertag … nach“, danach „nachgeholt“.
