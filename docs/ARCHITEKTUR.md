# DepotSchlopf – Architektur

Stand 01.11.2026 · Wolfgang Mair

*Fassung 4.8. Dieses Dokument beschreibt das Ziel. Verbindlich wird es mit Wolfgangs Zustimmung. Danach gilt allein das Repository: Kodex, Festschreibung, Typen und Tests.*

## 1 Zweck und Leitsatz

DepotSchlopf ist ein Rust-Programm, das auf Wolfgangs dauerhaft laufendem Rechner ein kleines Depot bei Interactive Brokers (IBKR) verwaltet. Es beobachtet Kurse, Konto und Nachrichten ständig und ohne Kosten. Nur wenn etwas Wesentliches passiert, fragt es zwei KIs um Rat: Dachs (Claude) und Maus (GPT). Die KIs schlagen vor und begründen. Ein festes Regelwerk entscheidet, ein Kontowächter prüft die Deckung, und nur eine einzige Stelle darf Aufträge an den Broker senden. Jeder Schritt hinterlässt einen Beleg.

**Ziel:** ein Plus des Gesamtvermögens gegenüber dem tatsächlich eingesetzten Geld, nach allen Kosten. Dazu zählen KI-Kosten, Gebühren, Spread und laufende Fondskosten, auch wenn sie außerhalb des Brokerkontos anfallen. Das Ziel ist der Maßstab jeder Bewertung, keine Gewinngarantie und keine Forderung nach einem Plus an jedem Tag. Ist der Ansatz nach allen Kosten nicht tragfähig, wird er angepasst oder gestoppt.

**Budget:** Der Rahmen arbeitet mit genau dem Betrag, den Wolfgang bereitstellt und freigibt, und setzt keine weiteren Mittel voraus. Er funktioniert mit jedem freigegebenen Betrag ab der Mindestgröße, die sich aus Mindestorder und Gebühren ergibt. Alle Euro-Beträge in diesem Dokument, die auf 3.000 € beruhen, sind Beispiele.

Das freigegebene Geld liegt in drei Bereichen: **Sicher** (geringstes Risiko, ohne KI), **Langfrist** (Vermögensaufbau über Jahre) und **Kurzfrist** (Chancen über Stunden bis Wochen, mit begrenztem Risiko). Die Aufteilung ist keine feste Voraussetzung, sondern Teil der Startwerte.

> Essenz lenkt. Sammelstelle beobachtet. KI schlägt vor. Regelwerk entscheidet. Kontowächter deckt. Auftragsstelle schreibt. Journal belegt. Abgleich prüft. Kodex wacht.

Die Architektur folgt dem Aufbau von EtuSchlopf: Zuerst stehen allgemeine Regeln. Jeder Baustein ergibt sich aus ihnen. Alle Zahlen stehen an genau einer Stelle, in der Festschreibung (Kapitel 18).

## 2 Allgemeine Regeln

Diese Regeln gelten für jeden Baustein und jede spätere Änderung. Die Regeln R1 bis R18 entsprechen der Essenz (`docs/essenz.md`, D00 bis D17) und ergänzen sie dort, wo Geld, Konto und Nachrichten hinzukommen.

| Nr. | Regel | Aussage | Folge für die Architektur |
| --- | --- | --- | --- |
| R1 | Repository vor Erinnerung | Was gilt, steht im Repository, nicht im Chat und nicht im Gedächtnis einer KI. | Architektur (`docs/ARCHITEKTUR.md`), Grenzen und Festschreibung stehen im Repository. Der Einstieg ist `docs/kaltstart.md`. |
| R2 | Rollen sind getrennt | Manche Teile dürfen sehen, andere vorschlagen, wenige entscheiden, einer schreibt. Wer schreibt, hinterlässt einen Beleg. | Sammelstelle sieht, KI schlägt vor, Regelwerk entscheidet, Auftragsstelle schreibt, Journal belegt, Abgleich prüft. |
| R3 | Schreibrecht ist ein Typ | Was nicht erlaubt ist, soll sich gar nicht bauen lassen. | Nur das Regelwerk erzeugt einen `GenehmigterAuftrag`, nur der Budgetwächter eine `AufrufErlaubnis`. Der Compiler setzt das durch. |
| R4 | Im Zweifel nichts tun | Ein verpasster Handel kostet Chancen, ein falscher Handel kostet Geld. | Jeder unklare Zustand führt zu Halt: fehlende oder unplausible Kurse, Abweichung zum Broker, kaputte Dateien, Rechenüberlauf. |
| R5 | Kein Verlust über den Einsatz | Kein Produkt und kein Geschäft darf mehr kosten als das eingesetzte Geld. | Nur Aktien und physische UCITS-ETFs aus einer Positivliste. Kein Hebel, kein Leerverkauf, kein Kredit, keine Derivate. |
| R6 | Nie ins Minus | Das Konto darf zu keinem Zeitpunkt negativ werden. | Reines Cash-Konto. Jeder Kauf ist durch selbst berechnetes, vorsichtiges verfügbares Geld gedeckt. |
| R7 | Schutz braucht keine KI | Sicherheit darf nie davon abhängen, dass eine KI antwortet oder richtig liegt. | Stopps liegen beim Broker. Halt und Notbremse löst Rust selbst aus. Zwingende Aktionen brauchen keine KI-Zustimmung. Die KI wird nur informiert. |
| R8 | Kosten sind vorher bekannt und hart begrenzt | Jede Kostenart wird berechnet, bevor sie entsteht, gebucht und gedeckelt. | Erst buchen, dann fragen (KI). Jeder Auftrag muss seine Gebühren erkennbar schlagen können. Für jede Kostenart gibt es eine Obergrenze. |
| R9 | Daten sind keine Anweisungen | Alles von außen ist unzuverlässig: Nachrichten, Kurse, KI-Antworten. | Nachrichten gehen nur gekürzt und markiert an die KI. KI-Antworten zählen nur, wenn sie zum Schema passen. Freitext löst nie etwas aus. |
| R10 | Ereignis statt Takt | Beobachten ist billig, Fragen ist teuer. | Der Rechner beobachtet ständig. Die KI wird nur gefragt, wenn Ereignisse eine Schwelle erreichen. Ein ruhiger Tag kostet nichts. |
| R11 | Begründung ist Pflicht | Jeder Kauf und Verkauf muss erklärbar sein. | Ohne Begründung mit Beleg aus der Eingabe gibt es keinen Auftrag. Jede Begründung landet im Journal. |
| R12 | Gespeicherte Bytes sind die Wahrheit | Was nicht auf der Platte steht, ist nicht geschehen. Der Broker schlägt das eigene Gedächtnis. | Erst Journal schreiben, dann handeln. Vor jedem Auftrag wird mit dem Broker abgeglichen. |
| R13 | Geld ist ganzzahlig | Rundungsfehler dürfen kein Geld erzeugen oder vernichten. | Cent als Ganzzahl, Anteile in Basispunkten, keine Gleitkommazahlen. |
| R14 | Grenzen ändern sich nie nebenbei | Eine Grenze ist nur so gut wie der Weg, sie zu ändern. | Grenzen stehen in der Festschreibung und ändern sich nur mit der Klasse `FestschreibungAenderung` und Wolfgangs Freigabe. Das Gate ändert sich nie zusammen mit Produktivcode. |
| R15 | Wolfgang hält die Schlüssel | Über Geld und Grenzen entscheidet der Eigentümer. | Dachs und Maus arbeiten Aufteilung, Strategie, Ordergrößen, Kostenansatz und Startwerte aus, begründen und prüfen sie. Wirksam werden sie nur als ganzes Paket mit Wolfgangs Freigabe; Einzelwerte muss er nicht festlegen. Keine KI setzt sich ihre Grenzen selbst. Nur Wolfgang stellt den Betrag bereit, verschiebt Geld zwischen Bereichen, ändert Grenzen, gibt Echtgeld frei, hebt einen Halt auf, wählt den Notbremsen-Modus und bestätigt im Modus „Bestätigung“ den Notbremsen-Verkauf. |
| R16 | Ehrlicher Maßstab | Ein Ergebnis zählt erst nach allen Kosten und Steuern und im Vergleich mit dem einfachsten Weg. | Jeder Bereich wird mit einem zu seinem Ziel passenden einfachen Maßstab verglichen: Sicher mit dem Euro-Geldmarktzins, Langfrist mit einem breiten Welt-ETF, Kurzfrist mit Bargeld und dem Welt-ETF. Regeln werden nie nachträglich an gespeicherte Ergebnisse angepasst. |
| R17 | Billige Prüfungen zuerst, Beweise synthetisch | Struktur vor Verhalten, erfundene Lagen vor echten Kursen. | Der Kodex prüft erst Deklaration, Pfade, Festschreibung und Quellen, dann Format, Clippy und Tests. Grenzen werden mit erfundenen Zahlen bewiesen. |
| R18 | Kleinster korrekter Abschluss, Kaltstart | Eine Stufe wird fertig, bevor die nächste beginnt. Jeder neue Bearbeiter muss allein weiterarbeiten können. | Ausbau in Stufen (Kapitel 19). Jede Stufe endet mit grünem Kodex und Assistenten-Export. |

**Regeln für Vertrauensgrenzen.** Die folgenden Regeln legen fest, wie die Regeln oben technisch durchgesetzt werden. Eine Garantie gilt nur so weit, wie eine dieser Regeln sie erzwingt und ein Negativtest das zeigt.

| Nr. | Regel | Aussage | Folge für die Architektur |
| --- | --- | --- | --- |
| R19 | Autorität aus der Festschreibung | Wer eine Grenze als Parameter übergeben kann, kann sie lockern. | Grenzen, KI-Deckel, Preise und Beraterzuordnung entstehen nur beim Laden der hash-geprüften Festschreibung, als Typ, den sonst niemand bauen kann. Frei einstellbare Werte gibt es nur in Tests. |
| R20 | Geprüfte Typen an jeder Grenze | Rohe Werte tragen keine Zusage. | Kurse (positiv, mit Handelstag, Währung, Herkunft und Alter), Kalenderdaten, Preise, Beträge und Zuordnungen werden beim Eintritt geprüft. Schreiber und Leser akzeptieren denselben Zustandsraum. Fehlende Daten sind ein eigener Zustand, nie ein bestandener Test. |
| R21 | Ungünstigster Fall als Prüfbasis | Eine Grenze, die am Schlusskurs hält, kann am Limit brechen. | Alle Geld- und Mengengrenzen werden am Limit mit der höchstmöglichen Gebühr gerechnet. Gerundet wird immer zugunsten der Sicherheit: Reservierungen und Kosten auf, verfügbares Geld ab. |
| R22 | Ziel, Kauf und Bestand unterscheiden | Eine Zusage über Zielgewichte ist keine Zusage über den Bestand. | Jede Anteils- und Anzahlgrenze gibt es als Zielgrenze, Kaufgrenze und Bestandsgrenze mit Toleranz und Abbaupflicht. Garantien werden genau so formuliert. |
| R23 | Zwang vor Sparsamkeit, Ergebnis vor Plan | Ein Ausstieg darf nicht an einer Umschlaggrenze scheitern, und ein Beleg darf nichts Erledigtes behaupten. | Ausstieg aus der Positivliste, Notbremse und Bereichs-Stopp gehen vor Umschlag- und Orderzahlgrenzen. Das Journal beschreibt das endgültig Genehmigte. |
| R24 | Identität ist eindeutig | Gleiche Namen für verschiedene Dinge machen Abgleich und Sperren wertlos. | Auftragskennungen vergibt eine zentrale Stelle. Dateien werden über ihre Identität gesperrt, Links sind verboten, Pfadlisten werden NUL-getrennt gelesen. |
| R25 | Kein Zähler ohne Anker | Was unbemerkt gelöscht werden kann, ist keine Grenze. | Kassenbuch und Journal sind Hash-Ketten mit ausdrücklicher Einrichtung und bestätigten Ankern nach einem festen Commitprotokoll (R38). Fehlt oder schrumpft eine Datei unter ihren bestätigten Stand, gilt Halt. Gegen vollen Schreibzugriff auf den Rechner schützen nur Grenzen außerhalb des Rechners. |
| R26 | Prüfen heißt verstehen | Eine Textsuche beweist weder Reinheit noch Lint-Treue. | Der Kodex wertet Code syntaktisch und Manifeste semantisch aus. Wo der Compiler eine Regel durchsetzen kann, tut er es (forbid, Crate-Grenzen), und ein Negativtest zeigt, dass er es tatsächlich tut. Wo eine Prüfung an Grenzen stößt (Makros, Reexporte), steht das ausdrücklich dabei. |
| R27 | Nachweise sagen die Wahrheit | Ein grüner Status ohne ausgeführte Prüfung ist schlimmer als keiner. | Jede Einzelprüfung meldet bestanden, fehlgeschlagen oder nicht ausgeführt. Build-Freigabe und Betriebsbereitschaft sind getrennt. Testnamen versprechen nur, was sie prüfen. |
| R28 | Export nach Positivliste | Eine Liste verbotener Namen kennt nie alle Geheimnisse. | Exportiert werden nur von Git verfolgte Dateien erlaubter Pfadrollen, byte-gleich mit dem geprüften Stand, ohne Links und ohne Benutzerpfade. Zusätzlich wird jeder Inhalt auf Geheimnismuster geprüft; ein Treffer bricht den Export ab. Geheimnisse liegen nie in verfolgten Dateien. |
| R29 | Fehler sperren | Wer nach einem Schreibfehler weiterschreibt, baut auf kaputtem Grund. | Nach Schreib-, Sync- oder Lesefehlern ist das betroffene Objekt bis zum geprüften Wiederanlauf gesperrt. Lesefehler beim Prüfen sind Fehler, nie ein Überspringen. |
| R30 | Freigabe ist an einen Stand gebunden | Eine Zustimmung ohne Bezug kann für etwas anderes benutzt werden. | Jede Freigabe hat einen Typ, einen Gegenstand und eine Gültigkeit (Kapitel 10) und ist maschinenprüfbar abgelegt. Ändert sich ihr Gegenstand, erlischt sie; keine Freigabe erbt eine andere. |
| R31 | Schweigen ist nie Zustimmung | Eine Prüfung, die bei Ausfall durchwinkt, ist keine Prüfung. | Ein neuer Auftrag entsteht nur, wenn der Prüfer dem konkreten Vorschlag ausdrücklich zustimmt und das Regelwerk danach genehmigt. Veto, Zeitüberschreitung, ungültige Antwort, Fehler oder fehlendes Budget für die Prüfung bedeuten: kein neuer Auftrag. Zwingende Aktionen (R23) brauchen keine KI-Zustimmung. |
| R32 | Prüfkriterien setzen keinen Anreiz | Wer weiß, wonach er bewertet wird, optimiert das Kriterium statt des Ziels. | Freigabekriterien belohnen nie Aktivität, etwa eine Mindestzahl an Trades. Die Berater kennen weder die Kriterien noch den Stand der Prüfung. Kosten zählen in jede Bewertung voll hinein. |
| R33 | Schwellen lösen aus, sie garantieren nicht | Ein Markt kann über jede Grenze springen. | Auch eine absolute Verlustobergrenze sagt die Architektur nicht zu. Cash-Konto ohne Margin, kein Leerverkauf, gedeckte Käufe und Gebührenpuffer machen eine Forderung über den Einsatz unwahrscheinlich, schließen sie vertraglich aber nicht aus (Kapitel 14). Stopps, Bereichs-Stopps, Tagesverlust und Notbremse lösen bei einer Schwelle aus. Der tatsächliche Verlust kann darüber liegen. Die Architektur schreibt deshalb „löst aus bei“, nie „begrenzt auf“. |
| R34 | Jede Order hat ein festes Verfahren | Ein Auftrag ohne festgelegten Ablauf wird im Ernstfall improvisiert. | Für jeden Aktionsgrund sind Ordertyp, Auslöser, Limitbildung, Handelszeit, Teilausführung und Restbehandlung festgeschrieben (Kapitel 10). |
| R35 | Schutz gilt erst nach Bestätigung | Ein gesendeter Schutzauftrag ist noch kein aktiver Schutz. | Bestand ohne vom Broker bestätigten Schutz für genau die gehaltene Menge ist ein eigener Zustand mit Frist. Läuft die Frist ab, wird der Rest storniert und die ungeschützte Menge zwingend verkauft. Ist der Broker nicht erreichbar, gilt der Fehlerzustand „Schutz ungewiss“ mit Halt, Nachricht und Auflösung nach der Wiederverbindung; Schutz in der Zwischenzeit ist nicht garantiert. Ein abgesendeter Stornowunsch ist keine Stornierung. |
| R36 | Zwang und Wahl sind getrennt | Ein Ausstieg, der an einer Sparregel scheitert, ist keiner. | Eine Matrix legt fest, welche Prüfung für freiwillige Käufe, freiwillige Verkäufe und zwingende Ausstiege gilt (Kapitel 8). Halt sperrt freiwillige Aufträge, nie die eigenen Schutzaktionen. Kein Leerverkauf, eindeutiger Bestand und plausible Kurse gelten immer. |
| R37 | Jeder Auftrag hat einen Zustand | Wer nicht weiß, ob er gesendet hat, sendet doppelt. | Aufträge durchlaufen eine feste Zustandsmaschine (Kapitel 10). Geld und Stücke reserviert eine einzige Stelle für Bereich und Gesamtkonto zugleich. Ein unbekannter Sendestatus führt nur zum Abgleich, nie zu einem zweiten Versand. |
| R38 | Anker zeigen nur auf Bestätigtes | Zwei Dateien lassen sich nicht gleichzeitig schreiben. | Änderungen über mehrere Dateien folgen einem Commitprotokoll mit Wiederherstellung. Ein Anker zeigt nur auf einen schon bestätigten Stand, nie auf einen gerade entstehenden. Gegen das gemeinsame Zurückrollen aller Dateien hilft nur ein Anker außerhalb des Rechners. |
| R39 | Was kommt, zählt schon heute | Märkte reagieren auf angekündigte Ereignisse oft vor und an ihrem Termin, nicht erst danach. | Ein Ereigniskalender führt künftige Termine mit Zeitpunkt, Betroffenen, Quelle und Verlässlichkeit. Termine im Horizont stehen im Lagebild, lösen gebündelte Anfragen aus und müssen in jeder Begründung berücksichtigt sein. Ein Termin ändert keine Grenze und hebt kein Veto auf; es gibt keine Sonderregeln für einzelne Ereignisse. |
| R40 | Beobachten statt raten | Den Ausgang eines Ereignisses vorherzusagen ist unsicher; die Reaktion des Markts danach lässt sich messen. | Zu jedem Termin hält ein Beobachtungsjournal Ereignis, vorherige Erwartung, tatsächliches Ergebnis und gemessene Marktreaktion fest. Ein Kauf, der sich auf ein Ereignis stützt, braucht eine bestätigte Beobachtung als Beleg, keine Vorhersage. Wiederkehrende Zusammenhänge zählen erst ab einer Mindestzahl gleichartiger Fälle und nur als Hinweis (R33). |
| R41 | Wirkungen folgen Ketten | Ein Ereignis trifft selten direkt den gehaltenen Wert, sondern über Größen wie Rohstoffe, Zinsen, Währungen oder Branchen. | Jede Begründung mit Ereignisbezug nennt ihre Wirkungskette: Ereignis, betroffene Größe, handelbarer Wert im Universum, Richtung und Ordnung. Jedes Glied stützt sich auf die festgeschriebene Wirkungskarte oder eine bestätigte Beobachtung. Wirkungen erster Ordnung sind meist eingepreist, bevor der Rahmen handeln kann; das gilt als bekannte Grenze, nicht als Chance. |

## 3 Rollen und Rechte

| Baustein | Rolle | Netz | Dateien |
| --- | --- | --- | --- |
| Sammelstelle | sieht: Kurse, Kontostand, offene Orders, Schlagzeilen, Termine | ja, nur lesend (Broker, freigegebene Quellen) | schreibt Rohdaten in `daten/` |
| Lagebild | rechnet, vergibt Punkte, bündelt, schreibt das Lagebild für die KI | nein | liest Rohdaten |
| Budgetwächter | erlaubt oder verweigert jeden KI-Aufruf | nein | Kassenbuch |
| Berater Dachs und Maus | schlagen vor und begründen | nur zum KI-Anbieter, nur mit `AufrufErlaubnis` | nein |
| Regelwerk | entscheidet; einziger Erzeuger von `GenehmigterAuftrag` | nein | nein (reine Funktion) |
| Kontowächter | berechnet das verfügbare Geld, prüft Deckung und Abgleich, löst Halt aus | liest beim Broker | Journal |
| Auftragsstelle | schreibt als einzige an den Broker, nimmt nur `GenehmigterAuftrag` an | ja | Journal |
| Journal | belegt jeden Schritt in einer Hash-Kette | nein | nur anhängen |
| Berichte | Tagesbericht, Monatsbericht, Steuer-Export | nein | nur lesen |
| Kodex | wacht über jede Änderung am Code | nein | liest das Repository |
| Lauf (`run.cmd`) | einziger Einstieg: Dienst, Prüfung, Befehle, Assistenten-Export | je nach Befehl | je nach Befehl |

Der Code kennt keine Überweisungen, keine Ein- und Auszahlungen und keine Änderung von Kontoeinstellungen. Das bleibt bei Wolfgang im IBKR-Kundenportal.

## 4 Datenfluss

1. **Sammeln:** Die Sammelstelle holt in der Handelszeit regelmäßig Kurse, Kontostand und offene Orders, außerdem Schlagzeilen und Termine. Alles wird mit Zeitstempel gespeichert.
2. **Rechnen und Bewerten:** Das Lagebild berechnet Kennzahlen und vergibt für jedes Signal Punkte. Die Signale sammeln sich in einem Eingangskorb je Bereich.
3. **Schützen ohne KI:** Verletzt ein Wert eine Schutzgrenze (Tagesverlust, Kontoabweichung, Minus-Gefahr), löst Rust sofort Halt aus. Stopps beim Broker arbeiten ohnehin selbstständig.
4. **Bündeln:** Erreicht der Korb seine Schwelle oder tritt ein dringendes Ereignis ein, entsteht genau eine Anfrage mit allen gesammelten Signalen.
5. **Buchen:** Der Budgetwächter bucht die Höchstkosten des Aufrufs. Erst dann gibt es eine `AufrufErlaubnis`.
6. **Vorschlagen:** Dachs bekommt das Lagebild und antwortet mit Zielgewichten, Begründungen, Risiken und Belegen.
7. **Prüfen:** Schlägt Dachs einen Handel vor, prüft Maus als Zweitmeinung. Nur eine ausdrückliche Zustimmung von Maus lässt den Vorschlag weiter. Veto, Schweigen, Zeitüberschreitung, ungültige Antwort oder fehlendes Prüfbudget beenden ihn (R31).
8. **Entscheiden:** Das Regelwerk prüft Schema, Universum, Grenzen, Kosten und Deckung und erzeugt genehmigte Aufträge oder begründete Ablehnungen.
9. **Reservieren und Belegen:** Der Kontowächter reserviert Geld und Stücke, und der Auftrag steht dauerhaft im Journal, bevor er den Rechner verlässt (R12, R37).
10. **Schreiben:** Die Auftragsstelle sendet die genehmigten Aufträge nach dem festen Verfahren an IBKR (R34).
11. **Abgleichen:** Jede Antwort des Brokers ändert den Zustand des Auftrags im Journal. Nach jeder Ausführung gleicht der Kontowächter mit dem Broker ab.

## 5 Bereiche

Ein Bereich ist ein virtueller Teil desselben IBKR-Kontos mit eigenem Geld, eigenen Regeln und eigenem Budget. Die KI sieht und verwaltet nur ihren Bereich. Geld wandert zwischen Bereichen nur durch einen Befehl von Wolfgang, der im Journal steht.

|  | Sicher | Langfrist | Kurzfrist |
| --- | --- | --- | --- |
| Zweck | Sockel und Reserve | Vermögensaufbau über Jahre | Chancen über Stunden bis Wochen |
| Anlagen | Bargeld, später höchstens ein ETF auf kurz laufende Euro-Staatsanleihen | breite ETFs und große Aktien | liquide Aktien und große ETFs |
| Entscheidung | Wolfgang, keine KI | Dachs schlägt vor, Maus prüft | Dachs schlägt vor, Maus prüft |
| Anstoß | nur durch Wolfgang | Ereignisse, wöchentlicher Pflichtblick, monatliche gründliche Prüfung | Ereignisse |
| Schutz | geringes Kursrisiko, kein Hebel, keine Aktien | Verlustgrenzen für das Gesamtdepot | Stopp beim Broker, sobald bestätigt; eigene Bereichs-Stopps |
| Schlimmster Fall | Ausfallrisiko des Brokers; mit ETF zusätzlich Zins-, Kredit-, Liquiditäts- und Fondsrisiken | Teilverlust bei einem Marktabsturz | Verlust auch über die Auslöseschwellen hinaus (Kurslücke, Ausführung), nie mehr als das Geld des Bereichs |

**„Kurzfrist“ heißt kurz halten, nicht leer verkaufen.** Auch im Kurzfrist-Bereich wird nur gekauft und später verkauft. Ein Leerverkauf würde R5 verletzen, weil sein Verlust nicht begrenzt ist.

**Allgemeine Regeln je Bereich:**

- Jeder Bereich hat eine Positivliste (Universum), eine Höchstzahl von Positionen, einen Höchstanteil je Wert, eine Mindestorder und eine Höchstzahl an Orders im Monat.
- Im Langfrist-Bereich wird nur umgeschichtet, wenn die Abweichung vom Zielgewicht die Umschichtungsschwelle und die Mindestorder erreicht oder ein begründetes Risiko vorliegt. Bei kleinen Beträgen ist er damit bewusst fast ein Kaufen-und-Halten.
- Im Kurzfrist-Bereich gehört zu jedem Kauf ein Schutzstopp beim Broker für genau die gefüllte Menge. Ob er aktiv ist, belegt allein die Bestätigung des Brokers. Bis dahin gilt die Menge als ungeschützt (R35, Kapitel 10). Ein bestätigter Stopp arbeitet weiter, auch wenn der Rechner ausfällt. Die KI wählt den Stopp-Abstand nur innerhalb des festgeschriebenen Spielraums.
- Ein Kurzfrist-Handel ist nur erlaubt, wenn der erwartete Gewinn ein festes Vielfaches der Kosten für Kauf und Verkauf erreicht (Kostenregel). Positionen haben eine Höchst-Haltedauer. Ist sie erreicht, ist der Verkauf ein zwingender Ausstieg (R36).
- Bereichs-Stopps: Erreicht der Verlust des Kurzfrist-Bereichs im Monat die Monatsschwelle, pausiert er bis zum Monatsende. Erreicht er die Endschwelle, wird er beendet: Seine Positionen werden zwingend verkauft, und sein Geld bleibt Bargeld, bis Wolfgang entscheidet.
- Der Bereich Sicher wird nie von einer KI berührt. Er ist zugleich die Reserve, aus der Wolfgang andere Bereiche auffüllen kann.

**Bereiche und Berater:** Die drei Bereiche ersetzen das frühere Modell zweier Töpfe, in dem Dachs Topf A und Maus Topf B mit je der Hälfte des Geldes verwaltete. Geld, Risikogrenzen und Bereichs-Stopps gehören jetzt zum Bereich, nicht zum Berater. In jedem KI-Bereich schlägt Dachs vor, und Maus muss jedem Handel ausdrücklich zustimmen (R31, Kapitel 7). Das KI-Budget wird je Bereich und je Berater gezählt und zusätzlich für beide zusammen begrenzt. Der Vergleich der beiden KIs läuft über das Schattendepot von Maus, nicht über getrenntes Geld. Der Code aus Stufe 1 kennt noch die zwei Töpfe und wird in Stufe 2 auf Bereiche umgestellt.

**Was „Sicher“ halten darf und welche Risiken bleiben:** „Sicher“ ist ein Ziel, keine Garantie. Zum Start hält der Bereich nur Bargeld. Dann bleibt das Ausfallrisiko des Brokers, das die gesetzliche Anlegerentschädigung nur begrenzt deckt. Später darf er höchstens einen physischen UCITS-ETF auf Euro-Staatsanleihen mit Restlaufzeit bis zu einem Jahr halten, und nur einen, den Wolfgang nach Prüfung von Struktur, Basisinformationsblatt und Kosten namentlich in die Positivliste aufnimmt. Kurze Laufzeiten verringern die Zinsabhängigkeit, schließen aber Kursverluste durch Zins-, Kredit-, Liquiditäts- und Fondseffekte nicht aus. Eine feste Verlustgrenze gibt es dafür nicht. Die Schwankungsgrenze der Festschreibung ist eine Melde- und Zukaufsregel: Fällt der ETF darunter, gibt es keine Zukäufe, und Wolfgang bekommt eine Nachricht. Verkauft wird nur auf seinen Befehl. Aktien, Unternehmensanleihen, Fremdwährung und Hebel sind ausgeschlossen. Das Geld des Kurzfrist-Bereichs bleibt bis zu dessen Echtgeld-Freigabe uninvestiert als Bargeld.

**Handelskosten:** Drei Größen sind getrennt. Das **Handelsbudget** ist die verbindliche Obergrenze für die Gebühren freiwilliger Aufträge je Zeitraum, im Kurzfrist-Bereich je Monat, im Langfrist-Bereich je Jahr, als Anteil am Bereichswert zu Beginn des Zeitraums. Die **Reservierung** rechnet je Order mit der festgeschriebenen Höchstgebühr aus der Gebührentabelle des tatsächlichen Kontomodells, samt Börsen- und Clearinggebühren. Die **tatsächlichen Kosten** werden nach der Ausführung gebucht. Der **Spread** ist keine Gebühr und zählt nicht gegen das Handelsbudget. Der Limit-Abstand begrenzt ihn nicht, denn ein Limit am letzten Kurs sagt nichts über den Abstand zwischen Bid und Ask. Deshalb gilt eine eigene **Spreadgrenze**: Ein freiwilliger Auftrag ist nur erlaubt, wenn aktuelle Bid- und Ask-Kurse vorliegen, nicht älter als das Kurshöchstalter, und der Spread höchstens die Spreadgrenze der Festschreibung erreicht, gemessen an der Mitte zwischen Bid und Ask. Fehlen Bid oder Ask, gibt es keinen freiwilligen Auftrag. Zwingende Ausstiege prüfen die Spreadgrenze nicht (R36). Der tatsächliche Spread wird bei jeder Ausführung gemessen, steht in der Kostenregel und zählt voll in jede Bewertung (R16).

- **Formel für einen freiwilligen Kauf:** Genehmigt wird nur, wenn der Rest des Handelsbudgets mindestens so groß ist wie die Höchstgebühr des Kaufs plus die Höchstgebühr des späteren Verkaufs. Beide Beträge belasten den Zeitraum des Kaufs. Ein Hin-und-zurück belastet das Budget also einmal, beim Einstieg. Mit 2,00 € Höchstgebühr sind das 4,00 €, die in 6,00 € passen; ein zweiter Einstieg im selben Monat passt nicht mehr.
- Die Höchstgebühr des Verkaufs wird zur **Ausstiegsreserve** der Position. Sie bezahlt den späteren Verkauf, freiwillig oder zwingend. Ein Rest verfällt und fließt nicht ins Budget zurück.
- Zwingende Ausstiege scheitern nie am Budget (R36). Kostet ein Ausstieg mehr als seine Ausstiegsreserve, etwa durch mehrere Teilaufträge, wird der Mehrbetrag im laufenden Zeitraum gebucht.
- Liegen die tatsächlichen Gebühren über dem Budget, ruhen freiwillige Aufträge des Bereichs bis zum Ende des Zeitraums, und der Bericht nennt die Überschreitung. Die Überschreitung wird nicht ins nächste Budget verschoben.
- Alle Bereiche buchen in ein gemeinsames Laufzeit-Kostenbuch. Das **Jahresbudget für KI und Gebühren** wird bei jeder Reservierung geprüft, nicht nur beim Laden der Festschreibung. Die Summe der Bereichsbudgets darf es nicht übersteigen. Es ist bewusst keine Obergrenze aller wirtschaftlichen Kosten: Spread und laufende Fondskosten kommen hinzu, werden gemessen und berichtet, aber nicht vorab gedeckelt (Kapitel 21).

## 6 Sammelstelle und Ereignislogik

Wolfgangs Rechner läuft dauerhaft und ist die Sammelstelle. Er kanalisiert Welt, Kurse und Konto in wenige, sinnvolle Anfragen an die KIs (R10).

**Sammeln:** Kurse und Kontostand in der Handelszeit in festem Abstand, Schlagzeilen aus freigegebenen RSS-Quellen in festem Abstand, ein Terminkalender (Quartalszahlen, Dividendentermine, Zinsentscheide). Kursdaten kommen zunächst als kostenlose, verzögerte IBKR-Kurse. Das reicht für Entscheidungen über Stunden und Tage, weil die Stopps beim Broker in Echtzeit arbeiten. Ein Echtzeit-Abo ist eine eigene Kostenart und braucht Wolfgangs Zustimmung (R8, R15).

**Ereigniskalender (R39):** Die Sammelstelle führt angekündigte Ereignisse als eigene Eingabe, getrennt von den Schlagzeilen: Zinsentscheide und Sitzungen der Zentralbanken, Konjunkturdaten, Quartalszahlen, Hauptversammlungen, Dividenden- und Indextermine, Gipfel, Wahlen und Treffen von Entscheidern. Jeder Eintrag hat eine Kennung, Zeitpunkt samt Unsicherheit, Art, betroffene Werte, Märkte und Bereiche, Quelle und eine von drei Verlässlichkeitsstufen:

- **bestätigt:** vom Veranstalter selbst veröffentlicht, etwa im Kalender der Zentralbank, des Unternehmens oder der Börse;
- **angekündigt:** von einer freigegebenen Nachrichtenquelle gemeldet, aber nicht vom Veranstalter bestätigt;
- **vermutet:** nur erschlossen, etwa aus dem üblichen Rhythmus. Ein Gerücht ist kein Termin.

Kalenderquellen stehen wie die Nachrichtenquellen auf einer festgeschriebenen Liste und sind Daten, nie Anweisungen (R9). Verschiebt sich ein Termin, wird der Eintrag fortgeschrieben; die alte Fassung bleibt im Journal. Die KI erfindet keine Termine: Sie kann nur Einträge mit Kennung aus dem Kalender als Beleg verwenden.

**Beobachten statt raten (R40):** Der Rahmen muss dem Markt nicht voraus sein. Er verfolgt die Welt und die Termine, misst nach dem Ereignis, wie der Markt tatsächlich reagiert, und reagiert dann auf erkennbare Zusammenhänge. Dafür führt die Sammelstelle ein **Beobachtungsjournal**, einen Eintrag je Termin:

- **vorher:** die Erwartung, soweit eine freigegebene Quelle sie nennt (etwa die Konsensschätzung), und die Einschätzung der KI, beide mit Zeitstempel und vor dem Termin festgehalten;
- **Ergebnis:** was tatsächlich verkündet wurde, aus einer Quelle der Stufe „bestätigt“;
- **Reaktion:** Kurs, Volumen und Spread der betroffenen Werte und Märkte in festen Fenstern nach der Veröffentlichung, deterministisch aus den Kursen gerechnet.

Eine Beobachtung ist **bestätigt**, wenn das Ergebnis aus bestätigter Quelle vorliegt und das Beobachtungsfenster im fortlaufenden Handel abgelaufen ist. Erst dann darf ein Vorschlag sich auf das Ereignis stützen. Aus vielen Einträgen rechnet Rust Zusammenhänge, etwa wie ein Bereich auf eine bestimmte Art von Ergebnis reagiert hat, und zeigt sie im Lagebild mit der Zahl der Fälle. Unterhalb der Mindestfallzahl gibt es keinen Zusammenhang. Ein Zusammenhang ist ein Hinweis, keine Zusage.

Nicht jedes Ereignis steht vorher im Kalender. Ein unangekündigtes Ereignis, etwa ein militärischer Angriff, ein Anschlag oder eine plötzliche Sanktion, kommt über die Schlagzeilen und die Stichwortliste. Sobald eine freigegebene Quelle es meldet, legt die Sammelstelle dafür einen Beobachtungseintrag an, ohne Erwartung vorher, aber mit Ergebnis und gemessener Reaktion wie bei jedem Termin.

**Wirkungsketten (R41):** Ein Ereignis wirkt selten direkt auf einen gehaltenen Wert. Es wirkt über **Größen**: Rohstoffpreise wie Öl und Gas, Zinsen, Währungen, Frachtraten oder ganze Branchen. Erst von dort führt der Weg zu einem handelbaren Wert im Universum des Bereichs, etwa einem Branchen-ETF oder einer Aktie. Eine festgeschriebene, versionierte **Wirkungskarte** ordnet jede Größe den Werten im Universum zu, mit erwarteter Richtung, zum Beispiel „Ölpreis steigt: Energiewerte eher auf, Fluglinien und Chemie eher ab“. Sie enthält keine Sonderregel für ein bestimmtes Land oder einen bestimmten Rohstoff, sondern nur Zuordnungen, die für jedes Ereignis gleich gelten.

- **Erste Ordnung:** die unmittelbare Wirkung auf die Größe und die direkt abhängigen Werte. Sie ist an liquiden Märkten oft binnen Minuten eingepreist.
- **Zweite Ordnung:** Folgewirkungen, etwa höhere Energiekosten in den Margen einer Branche, Zinserwartungen oder Währungsbewegungen. Sie wirken oft über Tage und Wochen.
- **Reaktionszeit als Grenze:** Verzögerte Kurse, Bündelung, Mindestabstand zwischen Anfragen, KI-Laufzeit, Prüfung durch Maus und das Beobachtungsfenster summieren sich auf mindestens eine halbe Stunde, oft mehr. Der Rahmen ist damit für Wirkungen erster Ordnung fast immer zu spät. Seine Stärke liegt in beobachteten Wirkungen zweiter Ordnung und in der Risikominderung bestehender Positionen.
- **Prüfung:** Jede Kette wird im Beobachtungsjournal geprüft: Hat sich die Größe tatsächlich bewegt, und folgten die Werte der Karte? Wo die Beobachtung der Karte widerspricht, zeigt der Monatsbericht das. Die Karte ändert sich nur per Festschreibung (R14).
- **Was nicht handelbar ist:** Eine Größe wie der Ölpreis selbst ist im Universum nicht handelbar, weil Rohstoff-Zertifikate und Termingeschäfte ausgeschlossen sind (R5). Der Rahmen handelt nur die zugeordneten Werte im Universum.

**Rechnen:** Bewegung seit der letzten KI-Sicht, Abstand zum Stopp, Volumen im Vergleich zum Durchschnitt, Abstand zum Höchststand, Rendite über 1, 3 und 12 Monate, Schwankung. Schlagzeilen werden entdoppelt, einem Wert zugeordnet und über feste Stichwortlisten eingestuft (z. B. Gewinnwarnung, Prognose, Übernahme, Klage, Rücktritt, Betrugsverdacht, Kapitalerhöhung). Alles ist deterministisch und damit wiederholbar.

**Punkte und Korb:** Jedes Signal bekommt Punkte aus einer festgeschriebenen Tabelle und landet im Eingangskorb seines Bereichs. Signale ohne Bezug zum Bereich bekommen keine Punkte.

**Bündeln:** Eine Anfrage entsteht, wenn der Korb die Schwelle erreicht oder ein dringendes Ereignis eintritt (Kurs nahe am Stopp, Stopp oder Order ausgeführt). Ein Termin mit Bezug zu einem Bereich gibt Punkte, sobald er in dessen Terminhorizont tritt. Alle Termine im Horizont reisen in der nächsten Anfrage mit. Eine eigene Anfrage vor einem Termin entsteht nur, wenn bis zu seinem Beginn keine Anfrage mehr ansteht. Nach dem Termin gelten Ergebnis und Kursreaktion als gewöhnliche Signale. Jede dieser Anfragen zählt gegen alle Grenzen und Budgets. Zwischen zwei Anfragen eines Bereichs liegt ein Mindestabstand. Was in dieser Zeit eintrifft, kommt in dieselbe Anfrage. Jede Anfrageart hat eine Tageshöchstzahl. Der Langfrist-Bereich bekommt zusätzlich einen wöchentlichen Pflichtblick und eine monatliche gründliche Prüfung, damit schleichende Entwicklungen nicht übersehen werden.

**Lagebild:** Die KI bekommt keinen Datenberg, sondern ein kurzes Lagebild in festem Aufbau: was sich seit ihrer letzten Sicht geändert hat, welche Signale ausgelöst haben, welche Termine im Horizont liegen (mit Kennung, Verlässlichkeit und betroffenen Positionen und Stopps), Konto, Positionen und Stopps, die Kosten möglicher Aufträge, ihre eigene letzte Begründung und höchstens eine festgelegte Zahl gekürzter Schlagzeilen, jede mit Kennung. Der feste Teil (Regeln, Grenzen, Universum, Antwortschema) steht immer gleich am Anfang. Das senkt die Kosten über die Zwischenspeicherung der Anbieter.

**Lernen:** Das Journal zählt, wie oft eine Anfrage zu einem Handel führte. Führen viele Anfragen zu nichts, schlägt der Monatsbericht eine andere Schwelle vor. Ändern darf sie nur Wolfgang (R14, R15). Der Monatsbericht vergleicht außerdem Erwartung und Beobachtung und zeigt, ob Reaktionen auf beobachtete Zusammenhänge nach allen Kosten etwas gebracht haben (R16).

**Ausfall:** Fällt der Rechner aus, schützen die Stopps beim Broker weiter, und es entstehen keine neuen Aufträge. Nach dem Start wird zuerst abgeglichen (Kapitel 11). Veraltete Signale werden nie nachträglich zu Aufträgen.

## 7 Entscheidung

**Wer beantwortet welche Frage:**

| Frage | Wer | Wie |
| --- | --- | --- |
| Was kaufen, was verkaufen? | Dachs, geprüft von Maus | Zielgewichte nur aus dem Universum des Bereichs |
| Warum? | Dachs, Pflichtfeld | Begründung je Wert mit Belegen aus dem Lagebild |
| Was kostet es? | Regelwerk | Gebühr und Spread je Auftrag aus der festgeschriebenen Gebührentabelle, vorher im Lagebild gezeigt |
| Was ist riskant? | KI und Regelwerk | Die KI nennt Risiken je Wert, das Regelwerk setzt die harten Grenzen durch |
| Was steigt, was fällt? | Lagebild | Kennzahlen aus den Kursen |
| Was meldet die Presse? | Sammelstelle | Gekürzte Schlagzeilen aus freigegebenen Quellen, als unzuverlässig markiert |
| Was steht bevor? | Sammelstelle | Termine aus dem Ereigniskalender mit Verlässlichkeit (R39) |
| Wie hat der Markt reagiert? | Sammelstelle | Beobachtungsjournal: gemessene Reaktion nach dem Termin und frühere Reaktionen auf gleichartige Ereignisse mit Fallzahl (R40) |

**Dachs und Maus:** Dachs schlägt vor. Maus prüft jeden vorgeschlagenen Handel und gibt dabei immer einen eigenen Gegenvorschlag ab. Ein Handel findet nur statt, wenn Maus ihm ausdrücklich zustimmt und das Regelwerk ihn danach genehmigt (Vier-Augen-Prinzip, R31). Die Zustimmung nennt eine unveränderliche Vorschlagskennung (Hash über Bereich, Lagebild-Stand und Aufträge) und gilt nur für genau diesen Vorschlag und nur für die festgeschriebene Gültigkeitsdauer. Das Regelwerk darf einen zugestimmten Vorschlag danach nur kürzen, nie erweitern, umkehren oder auf andere Werte übertragen. Veto, Schweigen, Zeitüberschreitung, ungültige Antwort, Fehler oder ein erschöpftes Prüfbudget bedeuten: kein neuer Auftrag. Eine Wiederholung der Prüfung zählt gegen dieselben Grenzen wie jeder andere Aufruf. Die Gegenvorschläge laufen ohne Geld in einem Schattendepot mit. So bleibt messbar, welche KI besser liegt, ohne das kleine Geld zwischen zwei KIs aufzuteilen. Beide bekommen dasselbe Lagebild.

**Antwortformat (Schema, strukturierte Ausgabe des Anbieters):**

| Feld | Inhalt | Pflicht |
| --- | --- | --- |
| lage | Einschätzung der Lage in wenigen Sätzen | ja |
| zielgewichte | je Wert: ISIN, Gewicht in Basispunkten, Aktion (kaufen, halten, verkaufen) | ja |
| begruendung | je geändertem Wert: warum, in wenigen Sätzen | bei jeder Änderung |
| risiken | je geändertem Wert: was dagegen spricht | bei jeder Änderung |
| belege | je geändertem Wert: Kennungen aus dem Lagebild (Kennzahl, Schlagzeile, Termin oder Beobachtung) | bei jeder Änderung |
| termine | je betroffenem Wert: Kennungen der Termine im Horizont und wie sie berücksichtigt sind (halten, verkleinern, abwarten, bewusst eingehen) | wenn Termine im Horizont liegen |
| kette | je Wert mit Ereignisbezug: Ereignis, Größe, Wert, Richtung, Ordnung (erste oder zweite) und Kennung des Kartenglieds oder der Beobachtung | wenn ein Ereignis die Begründung trägt |
| stopp | Stopp-Abstand je Kurzfrist-Kauf | im Kurzfrist-Bereich |
| erwartung | erwartete Kursbewegung je Kurzfrist-Kauf (für die Kostenregel) | im Kurzfrist-Bereich |
| verzicht | warum nichts geändert wird | wenn keine Änderung |
| urteil | nur Maus: Zustimmung oder Veto zur Kennung des geprüften Vorschlags, mit Begründung | bei jeder Prüfung |

**Regeln zur Antwort:**

- Kaufen und Verkaufen brauchen gleichermaßen Begründung, Risiko und Beleg (R11). Halten braucht nichts.
- Verweist ein Beleg auf eine Kennung, die nicht im Lagebild stand, wird der Eintrag verworfen. So fallen erfundene Quellen auf.
- Passt eine Antwort nicht zum Schema, wird die Anfrage genau einmal wiederholt. Danach gilt „kein Vorschlag“, und nichts ändert sich.
- **Positionen über einen Termin hinweg:** Ein Termin kann eine Kurslücke über den Stopp hinaus auslösen (R33). Deshalb muss ein Vorschlag, der einen betroffenen Wert kauft oder hält, den Termin im Feld „termine“ abwägen; fehlt das, wird der Eintrag verworfen. Der Höchstanteil vor Terminen ist eine Anteilsgrenze mit allen drei Stufen (R22). Als Kaufgrenze kürzt er Käufe im Terminhorizont eines betroffenen Werts. Als Bestandsgrenze gilt er für den gehaltenen Bestand, sobald ein Termin in dessen Horizont tritt: Liegt der Bestand mehr als die Toleranz darüber, sind Zukäufe gesperrt, und der Überhang wird vor Beginn des Termins abgebaut, als zwingender Ausstieg mit dem Verfahren aus Kapitel 10. Gelingt der Abbau nicht rechtzeitig, bleibt der Rest stehen, Wolfgang bekommt eine Nachricht, und die kleinere Position gilt nicht als erreicht. Diese Grenze gilt für jedes Ereignis gleich. Zwingende Ausstiege bleiben von der Kaufgrenze unberührt (R36).
- **Käufe mit Ereignisbezug (R40):** Stützt sich ein Kauf auf ein Ereignis, muss unter den Belegen eine bestätigte Beobachtung stehen. Eine Vorhersage des Ausgangs genügt nicht; ein solcher Eintrag wird verworfen. Verkäufe zur Risikominderung vor einem Termin brauchen keine Beobachtung.
- **Wirkungsketten (R41):** Trägt ein Ereignis die Begründung, muss das Feld „kette“ vollständig sein, und jedes Glied muss auf die Wirkungskarte oder eine bestätigte Beobachtung verweisen. Eine Kette, deren Größe sich laut Beobachtung nicht bewegt hat, trägt keinen Kauf. Ein Kauf auf eine Wirkung erster Ordnung braucht zusätzlich eine bestätigte Beobachtung, dass der Wert ihr noch nicht gefolgt ist; sonst wird der Eintrag verworfen.
- Das Regelwerk bewertet nicht, ob eine Begründung klug ist. Es prüft, dass sie vollständig ist, dass die Belege stimmen und dass alle Grenzen eingehalten sind.

**Nachrichten als Daten (R9):** Schlagzeilen stehen in einem eigenen, als nicht vertrauenswürdig markierten Block. Rust lässt nur Titel, Datum, Quelle, Link und einen kurzen Auszug durch, entfernt Steuerzeichen und eingebettete Links und ruft nur freigegebene Domains ab. Die KI hat kein Werkzeug, das etwas auslöst. Selbst eine vollständig manipulierte Antwort bleibt in den harten Grenzen. Der schlimmste Fall ist ein schlechtes, aber erlaubtes Depot in einem Bereich. Stützt sich eine große Änderung nur auf eine einzige Schlagzeile, wird sie im Journal zur Durchsicht markiert.

Die Quellenliste ist festgeschrieben. Bis Wolfgang eine konkrete Liste freigegeben hat, bleibt die Nachrichtenanbindung abgeschaltet, und das Lagebild stützt sich nur auf Kurse und Kennzahlen. Jede Schlagzeile wird mit Quelle, Abrufzeit und Hash im Journal belegt. Text aus einer Schlagzeile wird nie als Anweisung ausgeführt. Schlagzeilen sind eine schmale Informationsbasis. Das Lagebild stützt sich deshalb vor allem auf Kurse und Kennzahlen.

**Nachrichtenquellen:**

| Quelle | Inhalt | Kosten | Hinweis |
| --- | --- | --- | --- |
| RSS-Schlagzeilen seriöser Medien | Titel, Datum, Link | kostenlos | nur privat; gespeichert werden Titel und Link, nie Volltext |
| Websuche der KI-Anbieter | aktuelle Treffer weltweit | rund 1 Cent je Suche plus gelesener Text als Eingabe | zunächst aus; später nur mit Domain-Liste und Höchstzahl |
| GDELT | weltweite Nachrichten-Metadaten | kostenlos | offen, aber sehr verrauscht; höchstens später |
| Finnhub oder Alpha Vantage (Gratisstufe) | Firmennachrichten, Kennzahlen | kostenlos mit Abruflimits | Nutzungsbedingungen vor Einsatz prüfen |

**Ehrliche Einschätzung:** Öffentliche Nachrichten stecken nach Sekunden bis Minuten im Kurs. Ein verlässlicher Vorsprung durch Schlagzeilen ist für ein privates Programm nicht zu erwarten. Wer dem Trend hinterherkauft, kauft oft teuer. Nachrichten dienen deshalb vor allem drei Zwecken: Risiken meiden (Gewinnwarnung, Betrugsverdacht, Klage), Entscheidungen nachvollziehbar machen und Ereignisse erkennen, die die Kurse allein nicht erklären. Sie sind ein Filter, kein Kaufsignal.

## 8 Regelwerk

Das Regelwerk ist eine reine Funktion. Es bekommt Bereich, Bestand, Kurse, verfügbares Geld, Vorschlag und Festschreibung und liefert genehmigte Aufträge oder begründete Ablehnungen. Es hat keinen Netz- und Dateizugriff und ist vollständig testbar (R3, R17).

**Kein Risiko über dem Einsatz auf drei Ebenen (R5):**

- **Instrumente:** Nur Werte aus der Positivliste des Bereichs. Optionsscheine, Zertifikate, CFDs, Futures, Hebel- und Short-ETFs gibt es im Datentyp gar nicht.
- **Aufträge:** Nur Kauf mit gedecktem Geld und Verkauf vorhandener, nicht gebundener Stücke. Nur Limit-Aufträge, im Kurzfrist-Bereich mit angehängtem Stopp.
- **Konto:** Beim Broker ein reines Cash-Konto ohne Margin und nur mit Handelsrechten für Aktien und ETFs.

**Prüfungen je Auftrag, in dieser Reihenfolge:** Schema und Vollständigkeit, Universum, Kursplausibilität, Limit-Abstand zum letzten Kurs, Höchstanteil je Wert, Höchstzahl der Positionen, Mindestorder, Gebührenquote, Kostenregel (Kurzfrist), Orderzahl und Handelskosten im Monat, Deckung durch den Kontowächter, Spreadgrenze, Bereichs-Stopps, Halt-Zustand. Für freiwillige Aufträge führt jede verletzte Grenze zu Ablehnung oder Kürzung, nie zu einer Ausnahme.

**Aktionsgrund und Prüfung (R36):** Zwingende Ausstiege sind der Ausstieg eines Werts aus der Positivliste, der Notbremsen-Verkauf, das Bereichsende, die erreichte Höchst-Haltedauer, die Abbaupflicht über einer Bestandsgrenze und der Verkauf einer ungeschützten Menge nach Ablauf der Schutzfrist. Alles andere ist freiwillig.

| Prüfung | freiwilliger Kauf | freiwilliger Verkauf | zwingender Ausstieg |
| --- | --- | --- | --- |
| Zustimmung von Maus | ja | ja | nein |
| Universum | ja | nein | nein |
| Kurs plausibel und frisch genug | ja | ja | ja; sonst nur Halt, kein Verkauf |
| Limit-Abstand | ja | ja | Verfahren aus Kapitel 10 |
| Höchstanteil, Positionen, Deckung | ja | entfällt | entfällt |
| Mindestorder | ja | ja, außer beim Verkauf des ganzen Bestands | nein |
| Gebührenquote, Kostenregel | ja | ja | nein |
| Spreadgrenze, Bid und Ask vorhanden | ja | ja | nein; Spread wird gemessen |
| Ausgabenbudget, Orderzahl, Umschlag | ja | ja | nein; Kosten aus der Ausstiegsreserve |
| Bestand frei und eindeutig, kein Leerverkauf | entfällt | ja | ja |
| Halt | sperrt | sperrt | sperrt nicht |
| Bereichs-Stopp (Pause) | sperrt | erlaubt | sperrt nicht |

**Eingaben und Prüfbasis (R19 bis R23):**

- Das Regelwerk bekommt seine Grenzen nur als `FreigegebeneKonfiguration`. Diesen Typ erzeugt allein der Ladevorgang aus der Festschreibung nach Hash-Prüfung. Frei einstellbare Grenzen gibt es nur in Tests.
- Alle Eingaben sind geprüfte Typen. Kurse sind positiv und tragen Handelstag, Währung, Herkunft und Alter. Ein fehlender Vergleichskurs ist ein eigener Zustand (erster Handelstag) und kein bestandener Sprungtest. Bargeld ist nie negativ. Ein Vorschlag trägt seinen Bereich und seinen Berater, und das Regelwerk lehnt einen Vorschlag für einen fremden Bereich ab.
- Jede Geld- und Mengengrenze (Anteil, Mindestorder, Gebührenquote, Umschlag, Deckung) wird am Limit mit der höchstmöglichen Gebühr gerechnet, also im ungünstigsten Fall.
- **Drei Stufen jeder Anteils- und Anzahlgrenze:** Die Zielgrenze begrenzt, was die KI vorschlagen darf. Die Kaufgrenze prüft den Bestand nach dem Kauf im ungünstigsten Fall; ein Kauf, der sie verletzt, wird gekürzt. Die Bestandsgrenze gilt für den tatsächlichen Bestand, den auch Marktbewegungen verändern. Liegt er mehr als die Toleranz über dem Höchstanteil oder sind es mehr Positionen als erlaubt, sind Zukäufe gesperrt, und beim nächsten Anlass wird abgebaut, unabhängig von der Umschichtungsschwelle.
- Zwingende Aktionen haben Vorrang vor allen freiwilligen Schranken, wie die Matrix oben zeigt. Das Journal beschreibt immer das endgültig genehmigte Ergebnis, nie den Plan.
- Auftragskennungen vergibt eine zentrale Stelle fortlaufend aus dem Journal, über alle Planer hinweg. Zwei verschiedene Aufträge tragen nie dieselbe Kennung.
- Geldrechnungen sind geprüft. Ein Überlauf ist ein Fehler und führt zu Halt, nie zu einem gekappten Wert.

**Halt und Notbremse:**

- **Halt** heißt: keine neuen Aufträge, offene Kauforders werden storniert, bestehende Stopps und offene Verkaufsorders bleiben, Positionen werden nicht zusätzlich verkauft. Halt sperrt nur freiwillige Aufträge, nie die eigenen Schutzaktionen (R36). Halt lösen aus: eine Datei `HALT` im Datenverzeichnis, der Tagesverlust der KI-Bereiche, unplausible Kurse, eine Abweichung zum Broker und Minus-Gefahr.
- **Notbremse** heißt: Fallen die KI-Bereiche um die Notbremsen-Grenze unter ihren Höchststand, gilt sofort und ohne Bestätigung Halt, mit allem, was dazugehört: Offene Kauforders werden storniert, Stopps und offene Verkaufsorders bleiben, Wolfgang bekommt sofort eine Nachricht. Ob die Positionen danach verkauft werden, regelt der festgeschriebene Notbremsen-Modus. Im Modus „Bestätigung“ verkauft Rust erst nach Wolfgangs Bestätigung mit Limit. Bleibt sie aus, bleibt es beim Halt; Schweigen ist nie Zustimmung (R31). Der Verlust ist dann nicht automatisch begrenzt: Kurzfrist-Positionen schützt weiter ihr Stopp, Langfrist-Positionen haben keinen. Im Modus „automatisch“ sendet Rust sofort Verkaufslimits für alle Positionen der KI-Bereiche. Das senkt das Risiko weiterer Verluste, garantiert aber weder die Ausführung noch eine Verlusthöhe (R33), verkauft womöglich am Tiefpunkt, realisiert Verluste und kann in einem hektischen Markt schlecht ausgeführt werden. Unplausible Kurse lösen in beiden Modi nur Halt aus, nie einen Verkauf. Der Bereich Sicher bleibt unberührt.
- Wieder anlaufen geht nur mit dem bewussten Befehl `freigeben`, der im Journal steht (R15).

## 9 Kontowächter

Der Kontowächter ist reiner Rust-Code ohne KI. Er läuft in der Handelszeit in festem Abstand sowie vor und nach jedem Auftrag. Er liest bei IBKR Bargeld, Bestand, offene Orders, Gebühren und Zinsen und gleicht alles mit dem Journal ab (R12). Passt etwas nicht zusammen, gilt sofort Halt, und Wolfgang bekommt eine Nachricht.

**Verfügbares Geld je Bereich** wird nicht vom Broker übernommen, sondern selbst und vorsichtig berechnet (R6):

| Posten | Wirkung |
| --- | --- |
| Bargeld des Bereichs laut Journal, gegengeprüft mit IBKR (der kleinere Wert zählt) | Ausgangswert |
| minus offene Kauforders (Limit × Stückzahl plus höchstmögliche Gebühr) | schon verplant |
| minus Verkaufserlöse, die noch nicht abgerechnet sind | zählen erst nach Gutschrift |
| minus Steuerrücklage auf realisierte Gewinne und Zinsen des Jahres | IBKR führt keine deutsche Steuer ab |
| minus Gebührenpuffer | Schutz gegen Rundungen und Nebenkosten |
| **= verfügbar** | nur dieser Betrag darf für Käufe verwendet werden |

**Regeln gegen das Minus:**

- Ein Kauf wird nur genehmigt, wenn Stückzahl × Limit mit Kaufpuffer plus höchstmögliche Gebühr das verfügbare Geld nicht übersteigt. Die Prüfung steckt im versiegelten `GenehmigterAuftrag`.
- Verkauft wird nur, was im Bestand liegt. Stücke in einem Schutzstopp oder einer offenen Verkaufsorder sind gebunden. Ein Leerverkauf ist damit auch versehentlich unmöglich.
- Ein Verkauf einer geschützten Position läuft mit ihrem Stopp als eine Gruppe, die zusammen nie mehr als den Bestand verkaufen kann: brokerseitig als OCA-Gruppe mit Mengenreduktion bei Ausführung, sonst durch bestätigtes Verkleinern des Stopps vor dem Verkauf. Erst die Bestätigung des Brokers gibt gebundene Stücke frei. Löst der Stopp während des Verkaufs aus, gilt, was der Broker zuerst ausführt; die Gruppe verhindert den zweiten Verkauf.
- Sinkt das Bargeld unter den Gebührenpuffer, gibt es keine Käufe mehr, und Wolfgang bekommt eine Nachricht. Nachgeschossen wird nie automatisch.
- Laufende Kosten außerhalb von Orders, etwa ein Kursdaten-Abo, werden vorab als Monatsbetrag eingeplant. Ohne Deckung wird nichts gebucht.
- Geld und Stücke reserviert allein der Kontowächter, nacheinander und für Bereich und Gesamtkonto in einem Schritt (R37). Zwei Aufträge können nie dieselbe freie Ressource sehen, auch nicht beim selben Wert in zwei Bereichen. Der Broker kennt die Bereiche nicht. Deshalb wird die Summe der Bereiche laufend gegen Gesamtbestand und abgerechnetes Geld beim Broker abgeglichen.
- Beim Start prüft der Kontowächter, dass das Konto ein Cash-Konto ohne Margin ist und nur die erlaubten Handelsrechte hat. Sonst startet der Dienst nicht.
- **Restrisiko Forderungen:** Der Kontowächter gleicht Bargeld je Währung mit dem Broker ab. Ein Saldo unter null, ein Saldo in fremder Währung und jede Belastung, die keinem eigenen Auftrag oder eingeplanten Posten zugeordnet ist, führen zu Halt und einer Nachricht. Automatische Währungsumrechnung bleibt ausgeschaltet, soweit der Broker das erlaubt. Einen Fehlbetrag gleicht kein Automatismus aus, weder durch Verkauf noch durch Nachschuss; das entscheidet Wolfgang.

**Kassenbuch und Journal (R24, R25, R29):**

- Beide sind Hash-Ketten. Die erste Zeile entsteht nur durch einen ausdrücklichen Einrichtungsbefehl. Fehlt eine Datei später, ist sie leer oder kürzer als der letzte bekannte Stand, gilt Halt und kein Neustart.
- **Commitprotokoll (R38):** Das Journal ist die führende Datei. Ein Vorgang, der beide Dateien betrifft, schreibt zuerst den Eintrag ins Kassenbuch und synchronisiert ihn. Danach bestätigt ein Journaleintrag den Stand des Kassenbuchs (Zeilenzahl und Hash) und wird synchronisiert. Erst damit gilt der Vorgang. Das Kassenbuch verweist nur auf bereits bestätigte Journalstände, nie auf den gerade entstehenden. So entsteht keine zirkuläre Abhängigkeit.
- **Datensatzformat:** Jeder Eintrag ist eine Zeile mit Längenangabe, Prüfsumme und Abschlusszeichen. **Unvollständig** ist ein Eintrag, der kürzer ist als seine Längenangabe oder kein Abschlusszeichen hat. **Beschädigt** ist ein vollständiger Eintrag mit falscher Prüfsumme. Die beiden Fälle werden nie verwechselt.
- **Bestätigungsgrenze:** Für das Kassenbuch steht sie im Journal (bestätigter Stand mit Zeilenzahl und Hash). Für das Journal führt ein eigener Anhangsvermerk die Grenze: Vor jedem Anhängen wird darin der bestätigte Stand (Länge und Hash) und die Länge des geplanten Eintrags vermerkt und synchronisiert; nach der Synchronisation des Eintrags rückt der bestätigte Stand vor. Gehandelt wird erst danach (R12). Die Form einer Zeile beweist nichts: Ein abgebrochenes Schreiben und eine später gekürzte, bereits bestätigte Zeile können dieselben Bytes hinterlassen. Nachweislich unbestätigt ist deshalb nur, was jenseits eines verlässlich bestätigten Stands liegt und genau dem vermerkten Anhang entspricht.
- **Wiederherstellung:** Automatisch behandelt wird nur, was nachweislich unbestätigt ist. Ein unvollständiger letzter Eintrag wird nur dann gekürzt, wenn der Anhangsvermerk ihn als unbestätigten Anhang ausweist. Er wird in eine Quarantänedatei kopiert; diese wird synchronisiert und byte-gleich mit dem abgeschnittenen Rest geprüft, auch wenn sie schon existiert. Erst dann wird abgeschnitten und im Journal belegt. Danach darf das Kassenbuch höchstens um einen vollständigen, unbestätigten Eintrag über den bestätigten Stand hinausgehen. Er wird geprüft und dann bestätigt oder als verworfen markiert. Jeder beschädigte Eintrag, auch der letzte, jeder unvollständige Eintrag vor dem Ende, innerhalb des bestätigten Stands oder ohne passenden Anhangsvermerk, ein fehlender oder unstimmiger Vermerk, ein Kassenbuch kürzer als sein bestätigter Stand und ein abweichender Hash führen zu Halt. Diese Fälle repariert kein Automatismus, sondern ein geprüftes Reparaturverfahren mit Wolfgangs Freigabe. Weder Budget noch Vorgänge werden dabei still zurückgesetzt. Mehrdeutige Fälle bleiben gesperrt; eine automatische Reparatur jeder denkbaren letzten Zeile ist kein Ziel.
- **Absturz während der Wiederherstellung:** Jeder Schritt ist wiederholbar und führt beim zweiten Mal zum selben Ergebnis: Abschneiden auf denselben gültigen Stand, Quarantäne erst nach Prüfung ihres Inhalts als gesichert, Bestätigen oder Verwerfen nur, wenn das Journal es noch nicht enthält. Ein Absturz mitten in der Wiederherstellung führt beim nächsten Start zu ihrer Wiederholung, nie zu einem anderen Ergebnis.
- **Anker außerhalb des Rechners:** Der Abendbericht geht aufs Handy und nennt den bestätigten Stand beider Dateien. Nur dieser Anker zeigt, wenn beide Dateien gemeinsam auf einen alten Stand zurückgesetzt wurden.
- Beim Öffnen wird die ganze Historie geprüft: Reihenfolge der Daten und Zyklen, gültige Beträge und Summen. Schreiber und Leser akzeptieren genau denselben Zustandsraum.
- Gesperrt wird die geöffnete Datei selbst über das Betriebssystem, nicht ein abgeleiteter Pfadname. Links im Datenverzeichnis sind verboten.
- Nach einem Schreib- oder Synchronisationsfehler ist das Objekt bis zum geprüften Wiederanlauf gesperrt. Neue Dateien werden samt Verzeichniseintrag synchronisiert.
- **Grenze dieses Schutzes:** Er hilft gegen Fehler, Abstürze und versehentliche Änderungen. Gegen jemanden mit vollem Schreibzugriff auf Wolfgangs Rechner hilft keine Datei. Dagegen stehen die Grenzen außerhalb des Rechners: das Prepaid-Guthaben beim KI-Anbieter und das Cash-Konto bei IBKR.

## 10 Ausführung beim Broker

**Broker:** Interactive Brokers Ireland, reines Cash-Konto, Basiswährung Euro, Handel nur in Euro an Xetra. Die Verbindung läuft über das IB Gateway auf Wolfgangs Rechner. Es verlangt in Abständen eine neue Anmeldung mit Zwei-Faktor-Bestätigung auf dem Handy.

**Adapter:** Eine Schnittstelle `Broker` mit drei Umsetzungen: Simulation (für Tests und einen späteren Backtest), IBKR-Papierkonto und IBKR-Echtkonto. Lesen dürfen alle. Senden nimmt ausschließlich `GenehmigterAuftrag` an (R3).

**Aufträge:** Jeder Auftrag trägt eine eindeutige Kennung aus Bereich, Anfrage und laufender Nummer, damit doppeltes Senden erkennbar ist. Kauf- und Verkaufslimits gelten für den Tag und verfallen danach. Stopps gelten bis zur Aufhebung. Nach jeder Ausführung folgt der Abgleich.

**Was ohne Wolfgang läuft und was nicht:** Innerhalb der Grenzen handelt der Rahmen selbst, sonst wäre der Kurzfrist-Bereich nicht möglich. Diese Automatik gilt erst nach der technischen Abnahme (Kapitel 19). Neue Aufträge zu stoppen braucht nie eine Bestätigung. Wolfgang bestätigt die regelmäßige Anmeldung des Gateways, den Notbremsen-Verkauf (im Modus „Bestätigung“), das Aufheben eines Halts, jede Geldverschiebung zwischen Bereichen, jede Änderung der Festschreibung und die Freigabe von Echtgeld. Jede Freigabe ist eine Datei nach R30:

| Freigabe | gebunden an | erlischt, wenn | erteilt |
| --- | --- | --- | --- |
| Technische Abnahme | Commit, Hash des gebauten Programms, Festschreibung | sich eins davon ändert | unabhängige Prüfung, bestätigt von Wolfgang |
| Grenzfreigabe (`FestschreibungAenderung`) | Diff und neuer Hash der Festschreibung | die Festschreibung erneut geändert wird | Wolfgang |
| Echtgeld-Freigabe | Bereich, technische Abnahme, Festschreibung | sich eins davon ändert | Wolfgang |
| Handelszustimmung | Vorschlagskennung (Bereich, Lagebild-Stand, Aufträge) | die Gültigkeitsdauer abläuft oder ein neues Lagebild entsteht | Maus |
| Notbremsen-Verkauf (Modus „Bestätigung“) | Notbremsen-Ereignis | der Halt aufgehoben wird | Wolfgang |

**Auftragsverfahren (R34):**

| Aktionsgrund | Ordertyp | Auslöser und Limit | Handelszeit | Teilausführung und Rest |
| --- | --- | --- | --- | --- |
| freiwilliger Kauf | Limit, gültig für den Tag | Limit höchstens um den Limit-Abstand über dem letzten Kurs | fortlaufender Handel an Xetra | gefüllte Menge bekommt sofort ihren Schutzstopp; der Rest verfällt am Tagesende oder wird storniert, wenn der Schutz nicht fristgerecht bestätigt wird |
| Schutzstopp Kurzfrist | Stopp-Order, die bei Auslösung zur Marktorder wird, gültig bis zur Aufhebung | Stopp-Abstand aus dem Spielraum der Festschreibung | wie vom Broker unterstützt | immer genau die gehaltene Menge; jede Mengenänderung wird bestätigt |
| freiwilliger Verkauf | Limit, gültig für den Tag, mit dem Stopp in einer Gruppe | Limit höchstens um den Limit-Abstand unter dem letzten Kurs | fortlaufender Handel | Rest verfällt; der Stopp deckt weiter genau den Restbestand |
| zwingender Ausstieg | marktnahes Limit | Limit um den Limit-Abstand unter dem letzten Kurs; ein Rest wird in festen Schritten nachgezogen, höchstens bis zur Schrittgrenze | fortlaufender Handel, nur mit plausiblen und frischen Kursen | nach der Schrittgrenze bleibt der Rest stehen, und Wolfgang bekommt eine Nachricht |

Der Schutzstopp ist bewusst eine Stopp-Order, die bei Auslösung zur Marktorder wird, und damit die einzige Ausnahme von „nur Limit“. Sie wird ausgeführt, kann aber deutlich unter dem Stopp liegen. Eine Stopp-Limit-Order hält den Preis, kann aber unausgeführt bleiben, und ein nicht ausgeführter Schutz wiegt schwerer als ein schlechter Kurs. Die Wahl liegt bei Wolfgang (Kapitel 21).

**Schutz bei Teilausführung (R35):** Für jede gefüllte Menge muss innerhalb der Schutzfrist ein vom Broker bestätigter Stopp über genau diese Menge bestehen. Ein angehängter Stopp wird erst als Schutz gezählt, wenn der Broker ihn für die tatsächlich gefüllte Menge aktiv meldet. Fehlt die Bestätigung nach Ablauf der Frist und ist der Broker erreichbar, wird der Kaufrest storniert und die ungeschützte Menge zwingend verkauft, und es gilt Halt. Beim Start prüft der Kontowächter, dass jede Kurzfrist-Position einen bestätigten Stopp über ihre ganze Menge hat.

**Verbindungsabbruch (R35):** Ohne Verbindung zum Broker kann der Rahmen weder Schutz bestätigen noch verkaufen. Das ist ein eigener Fehlerzustand, **„Schutz ungewiss“**: Halt, keine neuen Aufträge und sofort eine Nachricht an Wolfgang aufs Handy mit Wert und möglicher ungeschützter Menge, damit er über die IBKR-App eingreifen kann. Die Verbindung wird in festen Abständen neu versucht. Nach der Wiederverbindung kommt zuerst der Abgleich. Besteht ein bestätigter Stopp über die ganze Menge, endet der Zustand. Sonst wird sofort ein Stopp für die ungeschützte Menge gesendet; ist er nicht innerhalb der Schutzfrist bestätigt, wird die Menge zwingend verkauft. Der Halt bleibt, bis Wolfgang ihn aufhebt. Zugesagt ist nur, dass der Zustand erkannt, gemeldet und nach der Wiederverbindung aufgelöst wird, nicht, dass in der Zwischenzeit Schutz besteht.

**Zustände eines Auftrags (R37):**

| Zustand | Bedeutung | weiter nach |
| --- | --- | --- |
| geplant | vom Regelwerk genehmigt | reserviert oder verworfen |
| reserviert | Geld und Stücke reserviert, dauerhaft im Journal | gesendet |
| gesendet | Versand im Journal begonnen, eindeutige Kundenkennung vergeben | angenommen, abgelehnt oder Sendestatus unbekannt |
| Sendestatus unbekannt | keine Antwort des Brokers | nur über den Abgleich mit der Kundenkennung, nie durch neuen Versand |
| angenommen | vom Broker bestätigt | teilgefüllt, gefüllt, storniert, verfallen |
| teilgefüllt | ein Teil ist ausgeführt | gefüllt, storniert, verfallen |
| gefüllt, storniert, verfallen, abgelehnt | Endzustand laut Broker | abgeglichen |
| abgeglichen | Journal, Kassenbuch, Konto und Broker stimmen überein; Orderreservierung aufgelöst | Ende |

**Reservierungen:** Es gibt drei Arten, und jede hat ihren eigenen Anfang und ihr eigenes Ende.

| Reservierung | entsteht | ändert sich | endet |
| --- | --- | --- | --- |
| Orderreservierung Kauf: Limit × Menge mit Kaufpuffer plus Höchstgebühr | im Zustand „reserviert“ | bei Teilfüllung wird der gefüllte Teil durch die tatsächliche Buchung ersetzt; reserviert bleibt nur der offene Rest | im Endzustand laut Broker, auch nach einem Storno; frei wird nur der nicht gefüllte Rest, nie der gefüllte Teil |
| Stückreservierung Verkauf | im Zustand „reserviert“ | bei Teilfüllung um die verkaufte Menge kleiner | im Endzustand laut Broker; nicht verkaufte Stücke werden frei |
| Ausstiegsreserve: Höchstgebühr des Verkaufs, aus dem Handelsbudget | mit der Reservierung des Kaufs | bleibt bestehen, sobald etwas gefüllt ist; wird nichts gefüllt, endet sie mit dem Kauf | wenn die Position vollständig verkauft und abgeglichen ist; ein Rest verfällt |

Nach einem Neustart wird jeder offene Zustand samt seinen Reservierungen aus dem Journal wiederhergestellt und zuerst abgeglichen.

## 11 Journal, Wiederanlauf und Berichte

**Journal:** Nur anhängen. Jede Zeile ist JSON mit dem SHA-256 der Vorzeile. Belegt werden: Rohdaten-Hash je Lagebild, auslösende Signale, KI-Anfrage (Prompt-Version, Modell, Token, Kosten), Antwort mit allen Begründungen, Prüfung durch Maus, Urteil des Regelwerks je Auftrag samt Ablehnungsgrund, Deckungsrechnung, gesendeter Auftrag mit Broker-Kennung, Ausführung, Abgleich, Halt, Freigabe und Geldverschiebungen. Ein Prüfbefehl rechnet die Kette nach.

**Wiederanlauf:** Der Zustand wird nur aus dem Journal rekonstruiert, einschließlich des Zustands jedes Auftrags (R37) und des Commitprotokolls (R38). Findet der Start einen unvollständigen Vorgang, wird nichts neu gefragt und nichts neu gesendet. Zuerst holt der Kontowächter offene Orders und Ausführungen beim Broker und gleicht ab. Erst wenn alles stimmt, läuft die Sammelstelle weiter (R4, R12).

**Berichte:**

- **Tagesbericht** (abends, eine Seite): Bargeld, investiert, offene Orders, Steuerrücklage, verfügbar je Bereich, Gewinn und Verlust je Bereich, Kosten bisher im Vergleich zum Budget.
- **Monatsbericht:** Was wurde warum gekauft und verkauft, Kosten je Kostenart, Vergleich mit dem Maßstab des Bereichs und dem Schattendepot von Maus, Statistik der Signale mit Vorschlag zur Schwelle.
- **Steuer-Export** (Kapitel 15).
- **Assistenten-Export** wie in EtuSchlopf: `run.cmd` schreibt nach jedem Lauf `assistant_export_latest.zip` mit Prüfergebnis, Übergabetext, Konsole, Manifest und Quellstand. Er enthält nie `daten/`. Schlüssel und Kontonummern liegen nie in verfolgten Dateien, und eine Inhaltsprüfung auf Geheimnismuster bricht den Export bei einem Treffer ab (R28).

**Nachweise und Export (R27, R28):**

- Jede Einzelprüfung meldet „bestanden“, „fehlgeschlagen“ oder „nicht ausgeführt“. Der Gesamtstatus ist nur „offen“, wenn alle Pflichtprüfungen dieses Laufs bestanden haben. Ein Befehl, der nur schreibt (etwa die Festschreibung berechnen), meldet nie ein offenes Gate.
- Build-Freigabe (Kodex) und Betriebsbereitschaft (Sperren, Kassenbuch, Konto) werden getrennt ausgewiesen.
- Der Export enthält nach Positivliste nur Dateien, die Git verfolgt und die zu einer erlaubten Pfadrolle gehören. Links und Dateien außerhalb der Projektwurzel werden nie gelesen. Pfadlisten werden NUL-getrennt gelesen, damit Umlaute und Sonderzeichen nicht verloren gehen.
- Der exportierte Quellstand ist byte-gleich mit dem geprüften. Das Manifest belegt das mit Hashes. Bei rotem Gate enthält der Export nur Prüfergebnis und Konsole, keinen Quellstand mit unbekannten Dateien.
- Konsolenausgaben werden maskiert, zum Beispiel Benutzerpfade als %USERPROFILE%.

**Nachrichten aufs Handy** (E-Mail oder Push): Halt, Notbremse, Kontoabweichung, Budget bei 80 %, Anmeldung des Gateways nötig. Eine Nachricht gilt erst als zugestellt, wenn der Dienst die Annahme bestätigt. Fällt der Meldeweg aus, wird das nie als Zustellung gewertet: Der Versuch steht im Protokoll, wird in festen Abständen wiederholt und im nächsten Bericht genannt. Ein Halt hängt nie davon ab, dass die Nachricht ankommt.

## 12 KI-Kosten und Budgetwächter

**Erst buchen, dann fragen (R8):** Vor jedem Aufruf bucht der Budgetwächter die Höchstkosten (gezählte Eingabe plus höchste Ausgabe mal Preis) dauerhaft ins Kassenbuch. Erst dann gibt es eine `AufrufErlaubnis`. Nach der Antwort werden die echten Kosten gebucht. Fehlt die Kostenangabe, zählt der Höchstbetrag. Die automatischen Wiederholungen der Programmbibliotheken sind abgeschaltet. Jeder kostenpflichtige Aufruf zählt, auch eine Wiederholung, ein Fehlversuch und ein Abbruch. Ein Neustart setzt keinen Zähler zurück.

**Sperren, jede für sich ausreichend:**

| Ebene | Grenze | Durchgesetzt durch |
| --- | --- | --- |
| 1 Aufrufart | eine Frage, eine Antwort; keine Agenten-Schleifen, keine Werkzeuge, kein Verlauf | Code kennt nur diesen Aufruf |
| 2 je Aufruf | Höchstzahl an Eingabe- und Ausgabe-Token je KI | Zählung vor dem Senden, `max_tokens` |
| 3 je Anfrage | höchstens eine Wiederholung ohne verwertbare Antwort | Kassenbuch |
| 4 je Tag | Höchstzahl an Anfragen je Bereich | Kassenbuch |
| 5 je Monat | Euro-Grenze je Bereich und Berater und für Dachs und Maus zusammen, harter Stopp | Budgetwächter |
| 6 je Jahr | eine gemeinsame Euro-Grenze für Dachs und Maus | Budgetwächter |
| 7 beim Anbieter | Prepaid-Guthaben je Anbieter ohne automatisches Aufladen | Anthropic- und OpenAI-Konto, außerhalb des Programms |

Ist das Monatsbudget eines Bereichs oder eines Beraters erreicht, wird in diesem Bereich bis Monatsende nicht mehr gefragt. Fehlt das Budget für die Prüfung durch Maus, gilt der Vorschlag als nicht geprüft (R31). Es entstehen dann keine neuen Aufträge. Stopps, Kontowächter und Halt arbeiten weiter (R7). API-Schlüssel liegen in der Windows-Anmeldeinformationsverwaltung, nie in Konfiguration, Journal oder Export.

**Gebundene Budgetkonfiguration (R19, R20):** Grenzen, Preise und die Zuordnung von Modell zu Berater stammen nur aus der Festschreibung und werden beim Öffnen des Kassenbuchs fest gebunden. Preise und Token-Grenzen sind positiv, eine Reservierung ist nie negativ, Datumswerte sind gültige Kalenderdaten. Die Tokenzählung umfasst die gesamte abrechenbare Anfrage, nicht nur den eigentlichen Text. Gezählt und begrenzt wird je Bereich und je Berater und zusätzlich für beide zusammen. Ein Monatsdeckel ist nur stimmig, wenn er die erwarteten Kosten und eine Reservierung für den teuersten zulässigen Einzelaufruf deckt. Sonst sperrt die Vorab-Buchung den Berater schon nach wenigen Aufrufen. Das prüft der Ladevorgang der Festschreibung.

## 13 Kosten und Gewinnschwelle

**Preise** (Stand November 2026, vor dem Start prüfen):

| Posten | Preis | Quelle |
| --- | --- | --- |
| IBKR Ordergebühr Deutschland, Tiered | 0,05 % vom Volumen, mindestens 1,25 € plus Börsengebühr; gerechnet mit 1,50 € je Order | [IBKR Europa](https://www.interactivebrokers.ie/en/pricing/commissions-stocks-europe.php) |
| Spread | gerechnet mit 0,05 % je Seite | Annahme |
| Dachs (Claude Opus 5.5) | 4 $ je Mio. Eingabe-Token, 20 $ je Mio. Ausgabe-Token | Anthropic-Preisliste |
| Maus (OpenAI-Spitzenmodell) | 10 $ / 50 $ je Mio. Token | [benchlm.ai](https://benchlm.ai/openai/api-pricing), Drittquelle |
| ETF-Kosten | etwa 0,1 % im Jahr | Annahme |

Dollar wird vorsichtig 1:1 als Euro gerechnet. Die Gebühr von 1,50 € und der Spread sind Annahmen für die Erwartung. Für Reservierungen zählt die festgeschriebene Höchstgebühr je Order. Den Spread begrenzt je freiwilligem Auftrag die Spreadgrenze, nicht der Limit-Abstand (Kapitel 5).

**KI-Aufrufe im Monat (erwartet):**

| Anfrage | Token (Ein/Aus) | je Aufruf | Anzahl | im Monat |
| --- | --- | --- | --- | --- |
| Kurzfrist, Dachs auf Ereignis | 8.000 / 1.500 | 0,06 € | etwa 25 | 1,50 € |
| Kurzfrist, Maus prüft Handel | 8.000 / 1.000 | 0,13 € | etwa 8 | 1,04 € |
| Langfrist, wöchentlicher Pflichtblick und Ereignisse | 8.000 / 1.000 | 0,05 € | etwa 6 | 0,30 € |
| Langfrist, monatlich gründlich (Dachs und Maus) | 28.000 / 8.000 und 8.000 / 1.000 | 0,40 € | 1 | 0,40 € |
| **Summe** |  |  |  | **etwa 3,25 €** |

**Beispiel: Jahreskosten bei 3.000 € und gleicher Aufteilung (alle Bereiche mit echtem Geld):**

| Bereich | KI erwartet | Handel erwartet | Summe erwartet | Anteil am Bereich | Obergrenze | Anteil am Bereich |
| --- | --- | --- | --- | --- | --- | --- |
| Sicher (1.000 €) | 0 € | 2 € (ein ETF-Kauf) | 2 € | 0,2 % | 5 € | 0,5 % |
| Langfrist (1.000 €) | 9 € | 12 € (Erstkauf 6 €, etwa 4 Orders) | 21 € | 2,1 % | 36 € (KI 18 €, Handel 12 €, Erstkauf 6 €) | 3,6 % |
| Kurzfrist (1.000 €), etwa 1 Trade im Monat | 30 € | 42 € | 72 € | 7,2 % | 108 € (KI 36 €, Handel 72 €) | 10,8 % |
| **Gesamt** | **39 €** | **56 €** | **95 €** | **3,2 % des Startkapitals** | **150 € Budget für KI und Gebühren** | **5,0 % des Startkapitals** |

Die erwarteten Werte folgen aus der Aufruftabelle oben und den Gebühren. Die Obergrenzen sind die festgeschriebenen Budgets: KI-Monatsdeckel mal zwölf, Gebühren als Anteil am Bereichswert. Sie begrenzen KI und Gebühren, nicht alle wirtschaftlichen Kosten. Spread und laufende ETF-Kosten von etwa 0,1 % im Jahr kommen hinzu; sie werden gemessen und berichtet, aber nicht vorab gedeckelt. Bezogen auf das Startkapital kostet die KI im Beispiel erwartet etwa 39 € im Jahr (1,3 %) und höchstens 54 € (1,8 %), für Dachs und Maus zusammen. Bei einem kleineren Betrag steigen diese Anteile, weil KI-Kosten und Gebühr je Order fast gleich bleiben. Die Tragfähigkeit wird deshalb für den tatsächlich freigegebenen Betrag gerechnet und mit dem Paket vorgelegt.

Solange Kurzfrist nur auf Papier läuft, entstehen dort keine Handelskosten, aber echte KI-Kosten von erwartet etwa 30 € im Jahr. Sie sind der Preis des Nachweises.

**Weitere Kostenarten:**

| Posten | Kosten | Wie gelöst |
| --- | --- | --- |
| Kursdaten | 0 € | kostenlose verzögerte IBKR-Kurse; Echtzeit-Abo nur mit Zustimmung |
| Ereigniskalender | 0 € | öffentliche Kalender von Zentralbanken, Unternehmen und Börsen; eine kostenpflichtige Quelle nur mit Zustimmung |
| Nachrichten | 0 € | RSS; KI-Websuche aus |
| Rechner und Strom | etwa 0 € zusätzlich | Wolfgangs Rechner läuft ohnehin |
| Währungstausch | 0 € geplant | nur Euro an Xetra; eine Umrechnung durch den Broker kostet Gebühr, wird als eigene Kostenart gebucht und gemeldet |
| Fehlbetrag auf dem Konto | Zinsen und mögliche Beitreibungskosten laut Kundenvereinbarung | darf nicht entstehen; wenn doch: Halt, Nachricht, Ausgleich durch Wolfgang; die Kosten zählen voll in die Bewertung |
| Kontoführung, Papierkonto | 0 € | bei IBKR derzeit kostenlos |
| Nicht ausgeführte Limits | 0 € | verfallen am Tagesende |
| Fehlgeschlagene KI-Aufrufe | im Budget | eine Wiederholung, zählt gegen alle Grenzen |
| Steuern | 26,375 % plus ggf. Kirchensteuer auf Gewinne | Steuerrücklage, Anlage KAP |

**Ehrliche Folgerung:** Die Mindestgebühr je Order macht kleine Kurzfrist-Trades teuer. Im Beispiel mit 1.000 € muss der Kurzfrist-Bereich erwartet gut 7 % im Jahr verdienen, bei ausgeschöpften Deckeln knapp 11 %, nur um seine Kosten zu decken. Dazu kommt der Geldmarktzins, den das Geld als Bargeld ohnehin bekäme. Das gelingt den meisten Kurzfrist-Händlern nicht. Deshalb bekommt er echtes Geld erst nach einem Nachweis auf dem Papierkonto (Kapitel 19). Mit mehr Geld sinkt der Anteil deutlich, weil die Gebühr je Order fast gleich bleibt. Zum Vergleich: Alles in einen Welt-ETF zu legen kostet etwa 5 € im Jahr. Daran misst sich DepotSchlopf (R16).

## 14 Sicherheit

Gegen einen Verlust über den Einsatz stehen konkrete Schutzmaßnahmen: ein reines Cash-Konto ohne Margin, kein Leerverkauf, nur gedeckte Käufe, ein Gebührenpuffer und Handel nur in Euro. Eine Garantie ist das nicht. Die Kundenvereinbarung des Brokers regelt auch für Cash-Konten Fehlbeträge samt Zinsen und Beitreibungskosten sowie eine kostenpflichtige automatische Währungsumrechnung. Ein Fehlbetrag kann etwa durch Gebühren, Datenabos, Zinsen, Steuerabzüge in fremder Währung, Kapitalmaßnahmen oder Fehlbuchungen entstehen. Dieses Restrisiko behandeln Kontowächter (Kapitel 9) und Kostenmodell (Kapitel 13). Der Code senkt das praktische Risiko zusätzlich über Stopps, Bereichs-Stopps, Tagesverlust und Notbremse. Diese lösen bei Schwellen aus und garantieren keine Verlusthöhe (R33).

| Was schiefgehen kann | Schutz | Folge |
| --- | --- | --- |
| KI schlägt Unsinn vor oder erfindet Werte und Quellen | Schema, Positivliste, Beleg-Prüfung, Prüfung durch Maus, alle Grenzen im Regelwerk | Vorschlag verworfen oder gekürzt |
| Maus antwortet nicht, falsch oder zu spät | ausdrückliche Zustimmung als Pflicht (R31) | kein neuer Auftrag |
| KI wird über Nachrichten manipuliert | Nachrichten nur gekürzt und markiert, keine Werkzeuge, nur Schema-Antworten, harte Grenzen | schlimmstenfalls ein schlechtes, aber erlaubtes Depot in einem Bereich |
| Aufrufer übergibt gelockerte Grenzen, Preise oder einen fremden Berater | Grenzen nur als `FreigegebeneKonfiguration` aus der Festschreibung, geprüfte Zuordnung von Vorschlag und Bereich | lässt sich nicht bauen oder wird abgelehnt |
| Ungültige Eingaben (Kurs null, falsches Datum, negativer Preis) | geprüfte Typen an jeder Grenze | Eingabe abgelehnt, Halt |
| Programmfehler sendet zu viele Orders | Orderzahl je Monat, nur Limit, Deckungsprüfung, zentrale Auftragskennung, Cash-Konto | Broker lehnt ab, Halt |
| Konto droht ins Minus zu gehen | Kontowächter, Prüfung am Limit, Gebührenpuffer, Steuerrücklage, Cash-Konto | keine Käufe, Nachricht an Wolfgang |
| Absturz mitten im Vorgang | Wiederanlauf aus dem Journal, Abgleich, eindeutige Auftragskennung | keine doppelten Orders |
| Kassenbuch oder Journal wird gelöscht, gekürzt oder über einen Link doppelt geöffnet | Hash-Kette, bestätigte Anker mit Commitprotokoll, Betriebssystem-Sperre, Link-Verbot | Halt statt Neustart der Zähler |
| Falsche oder fehlende Kurse | Plausibilitätsprüfung, fehlende Vergleichsdaten als eigener Zustand, Limit nahe am letzten Kurs | Halt, kein Handel auf falschen Daten |
| Markt bricht ein | Stopps beim Broker, Tagesverlust, Notbremse mit sofortigem Halt und Vorrang vor allen Sparregeln | Kurzfrist durch Stopps begrenzt; Langfrist im Modus „Bestätigung“ erst durch Wolfgangs Verkauf; Neustart nur durch Wolfgang |
| Kurslücke über Nacht | Stopp wird zum nächsten Kurs ausgeführt | Verlust kann über dem Stopp-Abstand liegen, nie über der Position |
| Kurslücke an einem angekündigten Termin | Ereigniskalender, Pflichtabwägung im Feld „termine“, Höchstanteil vor Terminen als Kauf- und Bestandsgrenze mit Abbau vor dem Termin | Verlust kann über dem Stopp-Abstand liegen; er trifft eine kleinere Position, sofern der Abbau rechtzeitig gelang |
| Auf einen Ausgang wetten, der nicht eintritt | Käufe mit Ereignisbezug nur nach bestätigter Beobachtung (R40) | kein Kauf auf Vorhersage |
| Ein scheinbarer Zusammenhang ist Zufall | Mindestfallzahl, Zusammenhang nur als Hinweis, Auswertung nach allen Kosten im Monatsbericht | schlimmstenfalls ein erlaubter, aber erfolgloser Handel |
| Wirkung ist schon eingepreist, wenn der Rahmen handelt | Reaktionszeit als bekannte Grenze, Kauf auf erste Ordnung nur mit Beobachtung, dass der Wert noch nicht gefolgt ist | kein Hinterherkaufen einer fertigen Bewegung |
| Eine Wirkungskette stimmt nicht | jedes Glied mit Kartenverweis oder Beobachtung, Prüfung im Beobachtungsjournal, Karte nur per Festschreibung | Kette trägt keinen Kauf; Monatsbericht zeigt den Widerspruch |
| KI erfindet oder verwechselt Termine | nur Kalendereinträge mit Kennung als Beleg, Verlässlichkeit sichtbar, Gerüchte sind kein Termin | Eintrag verworfen |
| Kauf wird nur teilweise ausgeführt oder die Verbindung reißt ab | Schutz erst nach Bestätigung, Schutzfrist, Zwangsverkauf der ungeschützten Menge (R35) | keine Menge bleibt über die Frist hinaus unbemerkt ungeschützt |
| Stopp löst während eines freiwilligen Verkaufs aus | Gruppe aus Stopp und Verkauf mit Mengenreduktion, Freigabe erst nach Bestätigung | höchstens der Bestand wird verkauft |
| Antwort des Brokers bleibt aus | Zustand „Sendestatus unbekannt“, Abgleich über die Kundenkennung (R37) | keine doppelte Order |
| Absturz zwischen Kassenbuch und Journal | Commitprotokoll mit Wiederherstellung (R38) | unterscheidbar von einer Kürzung |
| KI-Kosten laufen weg | sieben Sperren, zuletzt Prepaid-Guthaben ohne Aufladen | höchstens das Guthaben beim Anbieter |
| API-Schlüssel werden gestohlen | Windows-Anmeldeverwaltung; IBKR-Auszahlungen nur mit Wolfgangs Zwei-Faktor-Freigabe | kein Geldabfluss |
| Export nimmt fremde oder geheime Dateien mit | Positivliste, keine Links, Quellstand gleich geprüftem Stand, maskierte Konsole | nur verfolgte Projektdateien verlassen den Rechner |
| Kodex wird über eine Schreibweise umgangen | syntaktische Prüfung, `forbid`-Lints, Crate-Grenzen, Negativtests | Umgehung fällt im Kodex auf |
| Ein Status behauptet eine Prüfung, die nicht lief | Status je Einzelprüfung, getrennte Betriebsbereitschaft | kein falsches „offen“ |
| Rechner aus oder Internet weg | Stopps beim Broker; keine Nachholung mit alten Signalen | es wird einfach nicht gehandelt |
| Jemand lockert die Grenzen im Code | Festschreibung, Kodex, an einen Stand gebundene Freigabe, Hash in der Echtgeld-Freigabe | Kodex rot, Dienst startet nicht |
| Broker-Schnittstelle ändert sich | Abgleich nach jeder Order | Abweichung führt zu Halt |

## 15 Steuern

Das ist eine sorgfältige Zusammenstellung, keine Steuerberatung. Für das erste Steuerjahr empfiehlt sich eine Prüfung durch einen Steuerberater oder Lohnsteuerhilfeverein.

**Grundsatz:** IBKR führt keine deutsche Abgeltungsteuer auf Veräußerungsgewinne ab. Wolfgang gibt jedes Jahr die Anlage KAP ab. IBKR Ireland meldet Erträge und Verkaufserlöse über den automatischen Informationsaustausch an das Bundeszentralamt für Steuern. Wer vollständig angibt, hat nichts zu befürchten.

**Einmal beim Einrichten:** Basiswährung Euro und nur Euro-Handel an Xetra (keine Währungsgewinne). Kostenbasis-Methode bei IBKR auf FIFO stellen, wie im deutschen Recht. Wer kirchensteuerpflichtig ist, zahlt sie über die Erklärung.

**Was wohin gehört:**

- **Anlage KAP:** Gewinne und Verluste aus Aktienverkäufen jeweils in eigener Zeile, Zinsen, Kapitalerträge ohne inländischen Steuerabzug. Gewinn ist Verkaufserlös minus Anschaffungskosten minus Gebühren.
- **Dividenden deutscher Aktien:** Die Kapitalertragsteuer wird meist schon einbehalten. Sie wird mit angegeben, damit sie angerechnet wird.
- **Anlage KAP-INV:** für alle ETFs. Dazu gehören Ausschüttungen, Verkaufsgewinne und die jährliche Vorabpauschale. Aktien-ETFs haben eine Teilfreistellung von 30 %, Anleihen-ETFs keine.
- Die Bereiche sind nur virtuell. Steuerlich zählt das Konto als Ganzes.

**Verlustverrechnung:** Verluste aus Aktienverkäufen werden nur mit Gewinnen aus Aktienverkäufen verrechnet. Ein Rest wird vom Finanzamt festgestellt und vorgetragen. Verluste bei IBKR dürfen mit Gewinnen bei deutschen Banken verrechnet werden. Für dort liegende Verluste braucht es eine Verlustbescheinigung bis zum 15.12.

**Sparer-Pauschbetrag:** 1.000 € (zusammen veranlagt 2.000 €) über alle Banken. Bei IBKR gibt es keinen Freistellungsauftrag, der Betrag wird in der Erklärung berücksichtigt.

**Rücklage und Vorauszahlungen:** Der Kontowächter hält die Steuer auf realisierte Gewinne und Zinsen zurück. Nach Jahren mit hohen Gewinnen kann das Finanzamt Vorauszahlungen festsetzen.

**Von IBKR jährlich aufbewahren (zehn Jahre):** Jahres-Aktivitätsbericht, Übersicht der realisierten Gewinne und Verluste, Dividenden- und Zinsaufstellung mit einbehaltener Steuer.

**Steuer-Export aus dem Journal** (`run.cmd steuer <Jahr>`): Gewinne und Verluste nach FIFO je ISIN über das ganze Konto, Summen in der Gliederung der Anlagen KAP und KAP-INV, Vorabpauschale, Gebühren, Aufteilung je Bereich zur eigenen Auswertung und ein Abgleich mit den IBKR-Berichten mit Warnung bei Abweichungen über 1 €. Der Export ist eine Rechenhilfe. Maßgeblich bleiben die IBKR-Berichte und der Bescheid.

Quellen: [Ausländische Broker und Anlage KAP](https://www.capitalo.de/depot/ratgeber/auslaendische-broker-steuer), [CapTrader Steuerinformationen](https://www.captrader.com/steuern/steuerinformationen-fuer-private-depots/), [Forum Portfolio Performance](https://forum.portfolio-performance.info/t/deutsche-steuererklarung-fur-interactive-brokers/23691).

## 16 Kodex, Essenz und Werkzeuge

Übernommen aus EtuSchlopf ist alles Allgemeine. Nichts davon hängt an Etüden oder einem Takt, es gilt für Aktien genauso.

| Muster aus EtuSchlopf | Umsetzung in DepotSchlopf |
| --- | --- |
| Essenz lenkt, Authority schreibt, Gate entscheidet | KI berät, Regelwerk entscheidet, Auftragsstelle schreibt, Kodex wacht |
| Essenz als beratende Erfahrung | `docs/essenz.md` mit Aussage, Warum, Fehlform, Lösungsraum, Prüffrage, hartem Bezug |
| Kodex als ausführbares Gate | Crate `depot_kodex`, `run.cmd kodex`, CI auf jedem Pull Request |
| Deklarierte Änderungen | `kodex/aenderung.txt` mit Klasse, Zweck und Pfaden; Deklaration und echter Diff müssen passen |
| Festschreibung | `kodex/festschreibung.txt`: Grenzen, KI-Budget, Produktarten, Abhängigkeiten, Lints |
| Fail-closed | jeder unbekannte Zustand führt zu Halt |
| Offene Transaktion wird vor jedem Lauf wiederhergestellt | Wiederanlauf aus dem Journal mit Abgleich |
| Typisiertes Laufergebnis | jeder Vorgang endet mit einem typisierten Ergebnis im Journal |
| Ein Einstieg `run.cmd` | Dienst, Prüfung, Berichte und Export über einen Befehl |
| Assistenten-Export | `assistant_export_latest.zip` nach jedem Lauf |
| Kaltstart-Doku | `docs/kaltstart.md` |
| Normative Tests synthetisch | Grenzen mit erfundenen Zahlen, echte Kurse nur zur Bewertung |
| Beobachtende Oberfläche ohne Schreibrecht | später eine lokale Statusseite auf 127.0.0.1 |
| Feste Toolchain und strenge Lints | Rust 1.98.0, Edition 2024, Profil strict mit overflow-checks, Strict-Clippy, `--locked` |

Bewusst nicht übernommen ist die Menge an Verträgen und textbasierten Gates. Was der Rust-Typcheck verhindert, braucht kein Gate.

**Wie der Kodex prüft (R26):**

- Rust-Code wird syntaktisch ausgewertet, nicht als Text durchsucht. Attribute zählen samt `cfg_attr`, Importe werden aufgelöst, auch gruppierte und umbenannte. Eine syntaktische Auswertung löst Makros und Reexporte nicht vollständig auf. Wo das zählt, ersetzt eine Crate-Grenze die Suche.
- Die harten Lints stehen auf `forbid`, im Kopf jedes Crates. Damit lehnt schon der Compiler jede lokale Ausnahme ab, auch eine versteckte. Ein Negativtest baut je relevantem Target einen bewussten Verstoß und zeigt, dass der Compiler ihn ablehnt.
- `cargo metadata` dient nur zur Ermittlung von Paketen und Targets; die wirksamen Lints liefert es nicht. Die Manifeste werden deshalb semantisch geprüft, samt Vererbung aus dem Workspace, und Compileraufrufe und Flags werden einbezogen. Den Beweis liefert der Negativtest oben.
- Abhängigkeiten werden mit Paketname, Version, Quelle, Features und dem Hash von `Cargo.lock` festgeschrieben.
- Die Essenz hat ein festes Register der Regelnummern. Fehlt eine Regel, ist der Kodex rot.
- Reinheit und Netzfreiheit werden möglichst durch Crate-Grenzen erzwungen: Reine Crates hängen von keinem Crate mit Netz- oder Dateizugriff ab. Weil `std` selbst Datei- und Netzzugriff enthält, prüft der Kodex in reinen Crates zusätzlich die Verwendung von `std::fs`, `std::net`, `std::process` und `std::env`.
- Lesefehler beim Durchsuchen sind Fehler, kein Überspringen.
- Jede Kodex-Regel hat einen Negativtest, der den Bruch zeigt. Die Gegenbeispiele aus der Prüfung durch Maus (Stand 383ee05) werden zu solchen Tests.

## 17 Bausteine im Code

| Crate | Inhalt | Netz | Datei | Stand |
| --- | --- | --- | --- | --- |
| depot\_kern | Geld in Cent, ISIN, Instrument, Bereich, Bestand, Festschreibung | nein | nein | vorhanden |
| depot\_regelwerk | Prüfungen, Aufträge, einziger Erzeuger von `GenehmigterAuftrag` | nein | nein | vorhanden, wird auf Bereiche erweitert |
| depot\_ki | Berater Dachs und Maus, Budgetwächter, Kassenbuch, Prompt-Vorlagen | ja | Kassenbuch | Budgetwächter vorhanden |
| depot\_sammler | Kurse, Konto, Schlagzeilen, Termine | ja, lesend | Rohdaten | geplant |
| depot\_lagebild | Kennzahlen, Punkte, Korb, Bündeln, Lagebild | nein | liest | geplant |
| depot\_konto | Kontowächter, verfügbares Geld, Abgleich | liest Broker | Journal | geplant |
| depot\_broker | Schnittstelle `Broker`, Simulation, IBKR-Papier, IBKR-Echt | ja | Journal | geplant |
| depot\_journal | Hash-Kette, Prüfung, Wiederanlauf | nein | ja | geplant |
| depot\_bericht | Tages- und Monatsbericht, Steuer-Export | nein | liest | geplant |
| depot\_kodex | Gate | nein | liest Repository | vorhanden |
| depot\_lauf | `run.cmd`, Dienst, Befehle, Assistenten-Export | je nach Befehl | ja | vorhanden |

Die Abhängigkeiten zeigen nur nach innen: Der Kern kennt nur sich selbst, das Regelwerk nur den Kern.

## 18 Festschreibung: Startwerte

Alle Zahlen dieser Architektur stehen hier und später nur in `kodex/festschreibung.txt` (R1, R14). Die Kapitel oben verweisen auf diese Namen. Die Werte sind Vorschläge von Dachs und Maus; Euro-Beträge in Klammern beziehen sich auf das Beispiel mit 3.000 €. Festgeschrieben werden sie für den tatsächlich freigegebenen Betrag, als ein begründetes Paket mit Wolfgangs Freigabe (R15).

| Bereich | Wert | Startwert |
| --- | --- | --- |
| Geld | Startbetrag gesamt | der von Wolfgang freigegebene Betrag, noch offen; Beispiel 3.000 € |
| Geld | Sicher / Langfrist / Kurzfrist | Anteile am freigegebenen Betrag, Vorschlag von Dachs und Maus; Beispiel 1.000 / 1.000 / 1.000 € |
| Geld | Gebührenpuffer | 30 € |
| Geld | Kaufpuffer auf das Limit | 1 % |
| Instrumente | Produktarten | Aktie, ETF physisch UCITS |
| Instrumente | Universum Sicher | zum Start nur Bargeld; später höchstens ein von Wolfgang namentlich freigegebener physischer UCITS-ETF auf Euro-Staatsanleihen mit Restlaufzeit bis 1 Jahr |
| Instrumente | Schwankungsgrenze Sicher | Melde- und Zukaufsregel: 1 % unter Einstand: keine Zukäufe, Nachricht an Wolfgang; keine Verlustgrenze |
| Instrumente | Universum Langfrist | DAX 40 und eine feste ETF-Liste (Welt, Europa) |
| Instrumente | Universum Kurzfrist | DAX 40, MDAX und große Euro-ETFs an Xetra |
| Aufträge | Auftragsart | nur Limit; Kurzfrist mit angehängtem Stopp |
| Aufträge | Limit-Abstand zum letzten Kurs | höchstens 1 % |
| Aufträge | unplausibler Kurssprung | über 15 % zum Vortag |
| Aufträge | Gebührenquote | höchstens 1 % des Ordervolumens |
| Langfrist | Positionen | höchstens 4 |
| Langfrist | Anteil je Wert | Einzelaktie höchstens 25 %, ETF höchstens 60 % |
| Langfrist | Mindestorder | 200 € |
| Langfrist | Umschichtungsschwelle | 5 Prozentpunkte und mindestens Mindestorder |
| Langfrist | Orders | höchstens 6 im Jahr, Erstkauf ausgenommen |
| Kurzfrist | Positionen | höchstens 2, je höchstens 50 % des Bereichs |
| Kurzfrist | Mindestorder | 250 € |
| Kurzfrist | Stopp-Abstand | Standard 4 %, Spielraum der KI 2 bis 6 % |
| Kurzfrist | Kostenregel | erwarteter Gewinn mindestens 3 × (Höchstgebühren für Kauf und Verkauf + gemessener Spread) |
| Kurzfrist | Haltedauer | höchstens 20 Handelstage |
| Kurzfrist | Trades | höchstens 2 Hin-und-zurück (4 Orders) im Monat; beim Kostendeckel von 6 € praktisch einer |
| Kurzfrist | Bereichs-Stopps | Auslöser: 10 % Verlust im Monat: Pause bis Monatsende; 20 % unter Startwert: Ende |
| Gesamt | Tagesverlust der KI-Bereiche | 5 %: Halt |
| Gesamt | Notbremse | 20 % unter dem Höchststand der KI-Bereiche: Halt sofort; offene Kauforders storniert, Stopps und Verkaufsorders bleiben |
| Gesamt | Notbremsen-Modus | Bestätigung (Vorschlag) oder automatisch, mit dem Paket freigegeben |
| Sammelstelle | Abstand Kurse und Konto | 1 Minute in der Handelszeit |
| Sammelstelle | Abstand Schlagzeilen | 15 Minuten |
| Sammelstelle | Punkte | Gewinnwarnung oder anderes Stichwort zu einem Depotwert 3; Position ±2 % seit letzter KI-Sicht 2; DAX ±1,5 % am Tag 2; Volumen dreifach 1; Schlagzeile ohne Stichwort 1 |
| Sammelstelle | dringende Ereignisse | Kurs weniger als 1 % über dem Stopp; Stopp oder Order ausgeführt |
| Sammelstelle | Schwelle | 4 Punkte |
| Sammelstelle | Mindestabstand zwischen Anfragen | 30 Minuten je Bereich |
| Sammelstelle | Anfragen je Tag | Kurzfrist höchstens 4, Langfrist höchstens 1 |
| Sammelstelle | Pflichtblick Langfrist | wöchentlich; gründlich monatlich |
| Sammelstelle | Terminhorizont | Kurzfrist die nächsten 2 Handelstage, Langfrist die nächsten 2 Wochen |
| Sammelstelle | Punkte für Termine | Termin der Stufe „bestätigt“ oder „angekündigt“ mit Bezug zu einem Depotwert oder dem Bereich tritt in den Horizont: 3; Stufe „vermutet“: 1 |
| Kalender | Quellenliste | feste Liste öffentlicher Kalender, Änderung nur per Festschreibung |
| Aufträge | Höchstanteil vor Terminen | Vorschlag im Paket, unter dem normalen Höchstanteil je Wert; Kauf- und Bestandsgrenze mit der Toleranz der Bestandsgrenze, Abbau vor Beginn des Termins |
| Beobachtung | Beobachtungsfenster | 30 Minuten fortlaufender Handel nach Veröffentlichung des Ergebnisses; für Langfrist der folgende Handelstag |
| Beobachtung | Mindestfallzahl für Zusammenhänge | 10 gleichartige Ereignisse |
| Beobachtung | Wirkungskarte | feste, versionierte Liste von Größen (Rohstoffe, Zinsen, Währungen, Branchen) mit zugeordneten Werten im Universum und Richtung; Vorschlag kommt mit Stufe 4 |
| Nachrichten | Lagebild | höchstens 30 Schlagzeilen, je höchstens 300 Zeichen |
| Nachrichten | Anbindung | aus, bis Wolfgang eine Quellenliste freigibt |
| Nachrichten | KI-Websuche | aus |
| KI | je Aufruf Dachs | Ereignis höchstens 16.000 Token Eingabe und 4.000 Ausgabe (0,15 €); gründliche Prüfung höchstens 40.000 und 16.000 (0,48 €); Reservierungen auf ganze Cent aufgerundet |
| KI | je Aufruf Maus | höchstens 16.000 Token Eingabe, 3.000 Ausgabe (0,31 €) |
| KI | Wiederholung je Anfrage | höchstens 1 |
| KI | Monat | Kurzfrist 3,00 € (Dachs 1,80 €, Maus 1,20 €), Langfrist 1,50 € (Dachs 0,90 €, Maus 0,60 €); Dachs und Maus zusammen 4,50 € |
| KI | Jahr | 54 € für Dachs und Maus zusammen (1,8 % des Startkapitals), einschließlich Wiederholungen und Fehlversuchen |
| KI | Warnung | bei 80 % des Monatsbudgets |
| KI | Guthaben beim Anbieter | höchstens 25 € je Anbieter, ohne automatisches Aufladen |
| Stufen | Papierbetrieb | Langfrist mindestens 3 Monate, Kurzfrist mindestens 6 und höchstens 18 Monate |
| Stufen | Aufstockung | frühestens nach 12 Monaten Echtbetrieb |

**Startwerte der Vertrauensgrenzen:**

| Bereich | Wert | Startwert |
| --- | --- | --- |
| Anteil | Toleranz der Bestandsgrenze | 2 Prozentpunkte über dem Höchstanteil: Zukaufsperre und Abbaupflicht |
| Prüfbasis | Geld- und Mengengrenzen | am Limit mit höchstmöglicher Gebühr |
| Kodex | harte Lints | forbid |
| Kodex | Abhängigkeiten | Name, Version, Quelle, Features, Hash von Cargo.lock |
| Kodex | Essenz-Register | feste Liste der Regelnummern |
| Export | Umfang | nur von Git verfolgte Dateien erlaubter Pfadrollen, keine Links |
| Kassenbuch | Anker | bestätigter Stand (Zeilenzahl und Hash) im Journal und im Abendbericht aufs Handy; höchstens ein unbestätigter Eintrag beim Start |
| Aufträge | Schutzfrist Kurzfrist | 60 Sekunden bis zum bestätigten Stopp; sonst bei erreichbarem Broker Rest stornieren, ungeschützte Menge zwingend verkaufen, Halt; ohne Verbindung Zustand „Schutz ungewiss“: Halt, Nachricht, Neuversuch alle 30 Sekunden |
| Aufträge | zwingender Ausstieg | Limit um den Limit-Abstand unter dem letzten Kurs, höchstens 3 Nachzieh-Schritte zu je diesem Abstand |
| Kurse | Höchstalter | 20 Minuten für Entscheidungen, freiwillige Aufträge, Tagesverlust und Notbremse; älter: Halt |
| Kurse | Spreadgrenze | höchstens 0,5 % der Mitte zwischen Bid und Ask, Bid und Ask nicht älter als das Höchstalter; ohne Bid oder Ask kein freiwilliger Auftrag |
| Freigaben | Handelszustimmung von Maus | gültig 15 Minuten und nur bis zum nächsten Lagebild |
| Rundung | Reservierungen und Kosten | auf ganze Cent aufgerundet; verfügbares Geld abgerundet |

**Startwerte für Freigaben, Kosten und Maßstäbe** (stehen vor Beginn des Papierbetriebs fest und ändern sich danach nicht mehr):

| Bereich | Wert | Startwert |
| --- | --- | --- |
| Freigabe | Echtgeld Kurzfrist | drei getrennte Bedingungen: technische Beobachtung mindestens 6 Monate; Evidenz aus mindestens 12 abgeschlossenen Trades; wirtschaftliches Ergebnis nach den Zeilen unten. Kommen bis 18 Monate keine 12 Trades zustande, lautet das Ergebnis „nicht nachgewiesen“, und Kurzfrist bleibt Bargeld. Wolfgang kann den Bereich dann beenden oder eine neue Papierphase mit vorher festgelegten Kriterien beginnen, die nicht lockerer sind (R16, R32) |
| Freigabe | Vergleich mit Bargeld | Ergebnis nach allen Kosten (Gebühren, Spread, echte KI-Kosten des Bereichs) und nach der Steuer auf Gewinne besser als derselbe Betrag zum €STR im selben Zeitraum, täglich verzinst und gleich versteuert; der Welt-ETF wird daneben berichtet |
| Freigabe | Rückgang Kurzfrist | höchster Rückgang vom Höchststand unter 15 %, also strenger als Notbremse (20 %) und Bereichsende (20 %) |
| Freigabe | Disziplin Kurzfrist | höchstens ein ausgelöster Monats-Stopp; keine Regelverletzung; gesonderte Freigabe durch Wolfgang |
| Freigabe | Echtgeld Langfrist | mindestens 3 Monate auf Papier; keine Regelverletzung; Kosten höchstens 20 % über der Rechnung; jeder Abgleich stimmig |
| Freigabe | Kurzfrist vor Freigabe | Geld bleibt uninvestiert als Bargeld |
| Kosten | Handelsbudget Kurzfrist | Gebühren höchstens 0,6 % des Bereichswerts im Monat (6 € bei 1.000 €); ein Einstieg belastet Kauf- und Verkaufsgebühr (4 €); Höchstgebühr je Order 2,00 € bis 2.500 € Volumen, vor dem Start an der Gebührentabelle des tatsächlichen Kontos bestätigen |
| Kosten | Handelsbudget Langfrist | Gebühren höchstens 1,2 % des Bereichswerts im Jahr (12 € bei 1.000 €), Erstkauf ausgenommen |
| Kosten | Handelsbudget Sicher | Gebühren höchstens 0,5 % des Bereichswerts im Jahr |
| Kosten | Budget für KI und Gebühren | 5 % des Startkapitals im Jahr (im Beispiel 150 €); Spread und Fondskosten kommen hinzu, werden gemessen und berichtet, aber nicht vorab gedeckelt |
| Nachrichten | Quellenliste | feste Liste von RSS-Adressen, Änderung nur per Festschreibung |
| Maßstab | Sicher | Euro-Geldmarktzins (€STR) |
| Maßstab | Langfrist | breiter Welt-ETF (z. B. MSCI World oder FTSE All-World) |
| Maßstab | Kurzfrist | Bargeld und breiter Welt-ETF |

## 19 Stufen bis zum Echtgeld

Jede Stufe endet mit grünem Kodex und einem Assistenten-Export (R18).

1. **Grundlagen** (fertig): Kern, Regelwerk, Budgetwächter mit Kassenbuch, Kodex mit Essenz, `run.cmd` mit Assistenten-Export.
2. **Vertrauensgrenzen schließen, Persistenzbasis legen, Festschreibung umstellen:** Der vorhandene Code wird entlang R19 bis R38 neu geschnitten, in einzeln prüfbaren Abschlüssen. Zuerst wird jedes Gegenbeispiel aus den Prüfungen durch Maus ein Negativtest (`docs/BEFUNDE.md`). Dann folgt der Kodex (als eigene Änderung), danach Kern und Regelwerk, dann die Persistenzbasis: Journal als Hash-Kette mit zentraler Vergabe der Auftragskennungen und das Kassenbuch mit Commitprotokoll und Wiederherstellung (R38), zuletzt der Export. Dachs und Maus arbeiten Aufteilung und Startwerte für den freigegebenen Betrag aus, rechnen die Tragfähigkeit nach allen Kosten und legen alles als ein begründetes Paket vor. Es wird als `FestschreibungAenderung` mit Wolfgangs Freigabe wirksam. Die Planung setzt keinen festen Betrag voraus; der endgültige Betrag muss vor dem ersten Echtgeld feststehen. Die Essenz wird um die neuen Regeln ergänzt. Die Architektur liegt als `docs/ARCHITEKTUR.md` im Repository und wird dort gepflegt. Bis dahin verspricht die README nur, was der Code tatsächlich durchsetzt.
3. **Kontowächter, Auftragszustände, Wiederanlauf:** Reservierung über Bereiche und Gesamtkonto, Zustandsmaschine der Aufträge (R37), Wiederanlauf aus dem Journal, geprüft mit erfundenen Lagen samt Teilausführung und Neustart.
4. **Sammelstelle, Ereigniskalender und Lagebild** ohne KI, mit aufgezeichneten und erfundenen Ereignissen, darunter verschobene Termine und Termine unterschiedlicher Verlässlichkeit.
5. **Berater Dachs und Maus** mit allen Kostensperren.
6. **Broker-Adapter IBKR** am Papierkonto, Berichte, Nachrichten aufs Handy. Auftragsverfahren, Schutz bei Teilausführung, Stopp-Gruppen und unbekannter Sendestatus werden am Papierkonto nachgewiesen, nicht angenommen (R34, R35).
7. **Technische Abnahme, dann Papierbetrieb:** Vor dem Papierbetrieb mit echten Schnittstellen steht die technische Abnahme: Jeder Befund in `docs/BEFUNDE.md` steht auf „technisch geschlossen“ und ist von einer unabhängigen Prüfung erneut bestätigt, der Kodex ist grün, und die Abnahme liegt als Datei vor, gebunden an Commit und Festschreibung (R30). Fachliche Entscheidungen ersetzen diese Abnahme nicht. Danach läuft der Papierbetrieb für Langfrist mindestens 3 und für Kurzfrist mindestens 6 Monate, mit echten KI-Kosten. Die Prüfkriterien stehen vorher fest (Kapitel 18).
8. **Echtgeld für Sicher und Langfrist**, wenn die Prüfkriterien für Langfrist aus Kapitel 18 erfüllt sind und Wolfgang freigibt. Drei Monate beweisen nichts über die Rendite, nur dass die Technik stimmt (R16).
9. **Echtgeld für Kurzfrist** erst, wenn alle Prüfkriterien für Kurzfrist aus Kapitel 18 erfüllt sind und Wolfgang gesondert freigibt. Bis dahin bleibt sein Geld uninvestiert als Bargeld.
10. **Später, optional:** Backtest mit der Simulation, lokale Statusseite, KI-Websuche mit Domain-Liste, Echtzeit-Kurse.

## 20 Broker-Wahl

Gewählt ist IBKR: offizielle Order-Schnittstelle, kostenloses Papierkonto mit derselben Schnittstelle, niedrige Gebühren und Xetra-Handel. Geprüft wurden auch deutsche Banken mit automatischer Abgeltungsteuer. Nur comdirect (kostenlose REST-Schnittstelle, aber deutlich höhere Gebühren) und Smartbroker+ (Schnittstelle 29,90 € im Monat unter 45 Orders im Quartal) bieten eine offizielle Order-Schnittstelle, und keiner hat ein Testkonto. Trade Republic, Scalable, flatex, Consorsbank und ING haben keine offizielle Schnittstelle. Bei kleinen Beträgen und häufigeren Orders wiegen die Gebühren schwerer als der Steueraufwand, der mit Kapitel 15 beherrschbar ist. Wolfgang hat sich am 01.11.2026 für IBKR entschieden.

## 21 Offene Entscheidungen

**Entschieden am 01.11.2026:** Ziel ist ein Plus nach allen Kosten (Kapitel 1). Der Rahmen richtet sich nach dem freigegebenen Betrag, ohne feste Aufteilung und ohne weitere Mittel. Dachs und Maus arbeiten die Einzelwerte aus, Wolfgang gibt sie als Paket frei. Innerhalb der Grenzen handelt der Rahmen automatisch, aber erst nach der technischen Abnahme. Halt braucht nie eine Bestätigung, Echtgeld und Wiederanlauf bleiben bei Wolfgang. Ein Auftrag entsteht nur nach ausdrücklicher Zustimmung von Maus und Genehmigung durch das Regelwerk. Nachrichten kommen nur aus einer festen RSS-Quellenliste, ohne KI-Websuche, und bleiben bis zur Freigabe dieser Liste abgeschaltet. Die Gebühr je Order beträgt höchstens 1 %, und jeder Bereich hat seinen eigenen Maßstab. Kurzfrist läuft zuerst auf Papier.

**Noch offen bei Wolfgang:** der Betrag (genannt wurden 2.000, 2.500 oder 3.000 €; er muss vor dem ersten Echtgeld feststehen) und die Freigabe des Pakets.

**Inhalt des Pakets** (Vorschläge von Dachs und Maus, gemeinsam freigegeben, nicht einzeln):

1. Aufteilung auf Sicher, Langfrist und Kurzfrist als Anteile am Betrag; im Beispiel je 1.000 €. Sicher bleibt zum Start Bargeld, Kurzfrist bis zur Freigabe uninvestiert.
2. Prüfkriterien für Echtgeld Kurzfrist wie in Kapitel 18: mindestens 6 Monate Beobachtung, Evidenz aus 12 Trades, höchstens 18 Monate, sonst „nicht nachgewiesen“; nach allen Kosten und Steuern besser als Bargeld zum €STR; Rückgang unter 15 %.
3. KI-Obergrenze 54 € im Jahr für Dachs und Maus zusammen (im Beispiel 1,8 %), 4,50 € im Monat.
4. **Notbremsen-Modus, eine bewusste Risikowahl.** „Bestätigung“: Nichts wird ohne dich verkauft, aber der Verlust ist nicht automatisch begrenzt; Langfrist-Positionen laufen bis zu deiner Entscheidung ungeschützt weiter. „Automatisch“: Das Risiko weiterer Verluste sinkt, eine Verlusthöhe ist aber nicht garantiert (R33); es wird womöglich am Tiefpunkt verkauft, Verluste werden realisiert, und im hektischen Markt kann die Ausführung schlecht sein. Vorschlag: Bestätigung, weil Kurzfrist durch Stopps geschützt ist und Langfrist auf Jahre angelegt ist.
5. **Kostenrahmen, eine bewusste Kostenentscheidung.** Vorab gedeckelt werden KI und Gebühren: Kurzfrist 0,6 % im Monat, Langfrist 1,2 % im Jahr, zusammen mit der KI höchstens 5 % des Betrags im Jahr (im Beispiel 150 €). Der Spread wird nicht ins Budget gerechnet, weil er sich vorab nicht verlässlich bestimmen lässt. Stattdessen begrenzt ihn je freiwilligem Auftrag die Spreadgrenze (Vorschlag 0,5 %), und er zählt voll in Kostenregel und Bewertung. Die tatsächliche wirtschaftliche Belastung liegt damit über diesem Budget, und das Ziel nach allen Kosten bleibt der Maßstab. Die Alternative wäre, einen geschätzten Spread mit ins Budget zu nehmen; dann passt im Kurzfrist-Bereich womöglich kein Trade mehr in den Monat.
6. RSS- und Kalender-Quellenliste: Der Vorschlag kommt mit Stufe 4 zur Bestätigung.
7. **Art des Schutzstopps, ebenfalls eine Risikowahl.** Stopp mit Marktorder: wird ausgeführt, aber womöglich deutlich unter dem Stopp. Stopp-Limit: hält den Preis, bleibt aber womöglich unausgeführt, und die Position ist dann ungeschützt. Vorschlag: Stopp mit Marktorder.

## 22 Was Wolfgang selbst erledigt

- [ ] IBKR-Konto eröffnen: Cash-Konto ohne Margin, Basiswährung Euro, Handelsrechte nur für Aktien und ETFs, Kostenbasis FIFO, Papierkonto freischalten.
- [ ] API-Schlüssel für Anthropic und OpenAI anlegen, je höchstens 25 € Prepaid-Guthaben, automatisches Aufladen aus.
- [ ] Den Betrag nennen, spätestens vor dem ersten Echtgeld.
- [ ] Das Paket aus Kapitel 21 freigeben, wenn Dachs und Maus es begründet vorlegen.
