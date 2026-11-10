# Mathematik in DepotSchlopf

**Stand: 10. November 2026, Quellstand 3300645.** Diese Seite beschreibt, wo DepotSchlopf Mathematik einsetzt, welche Rolle sie dort hat und wo ihre Grenzen liegen. Sie ändert keine Formel, Grenze oder Festschreibung. Maßgeblich bleiben Code, Tests und `kodex/festschreibung.txt`.

DepotSchlopf benutzt Mathematik nicht, um Kurse vorherzusagen. Sie sichert, dass Geld, Budget, Aufträge und Journal unter allen Abläufen das tun, was die Regeln sagen, und sie misst ehrlich, ob die Beobachtungen der Berater etwas taugen.

## Wie die Aussagen zu lesen sind

| Rolle | Bedeutung |
|---|---|
| Entscheidet | Das Ergebnis wirkt direkt auf Geld, Aufträge oder Budget. |
| Schützt | Eine Grenze oder Invariante, die nie verletzt werden darf (fail-closed). |
| Misst | Liefert eine Beobachtung oder Kennzahl, erhält dadurch keine Entscheidungsbefugnis. |
| Prüft | Ein Test oder Beweis über den Code. Er gilt nur im geprüften Modell. |
| Geplant | Vereinbart, aber noch nicht im Code. |

Fehlende Information wird nie zu null oder zu einer Erlaubnis. „Kein Kurs“, „kein Budget“ und „keine Zustimmung“ bedeuten jeweils: nichts geschieht.

## Bestand im Quellstand

| # | Aufgabe und Mathematik | Rolle und Grenze | Quellstelle |
|---|---|---|---|
| 1 | Geld als Festkomma: ganze Cent in `i64`, jede Rechnung mit Überlaufprüfung, nie `f64` | Schützt. Ein Überlauf ist ein Fehler, kein Wert. | [geld.rs](../crates/depot_kern/src/geld.rs) |
| 2 | Kurse als Festkomma in Zehntausendstel Cent | Entscheidet. Kurse bleiben exakt, gerundet wird erst am Geld. | [preis.rs](../crates/depot_kern/src/preis.rs) |
| 3 | Gerichtete Rundung: Bewertung ab, Deckung auf, Kauflimit ab, Verkaufslimit auf | Schützt. Jede Rundung fällt zu Lasten des Depots aus, nie zu seinen Gunsten. | [preis.rs](../crates/depot_kern/src/preis.rs), [depot.rs](../crates/depot_kern/src/depot.rs) |
| 4 | Anteile in Basispunkten (1/10.000) mit Ab- und Aufrundung | Entscheidet bei Zielgewichten, Gebühren und Reserven. | [geld.rs](../crates/depot_kern/src/geld.rs) |
| 5 | Devisen: Dollar durch EZB-Referenzkurs desselben Tages, auf sechs Stellen abgeschnitten | Entscheidet. Fehler unter 0,000001 € je Stück; ein anderer Tag ersetzt den Kurs nie. | [devisen.rs](../crates/depot_kern/src/devisen.rs) |
| 6 | Börsenkalender: Ostersonntag nach Meeus/Jones/Butcher, feste und bewegliche Feiertage | Entscheidet, welcher Tag ein Handelstag ist. | [zeit.rs](../crates/depot_kern/src/zeit.rs), [boerse.rs](../crates/depot_kern/src/boerse.rs) |
| 7 | Journal als Hashkette: $h_i = \mathrm{SHA256}(h_{i-1} \,\Vert\, ; \,\Vert\, e_i)$, $h_0 = 0^{64}$ | Schützt. Jede nachträgliche Änderung bricht die Kette. Die Kette beweist Unverändertheit, nicht Richtigkeit. | [datei.rs](../crates/depot_journal/src/datei.rs) |
| 8 | Zustand als Faltung über das Journal: $Z = \mathrm{fold}(\text{anwenden}, Z_0, e_1 \dots e_n)$ | Entscheidet. Nach einem Neustart gilt nur, was aus den Einträgen folgt. | [stand.rs](../crates/depot_journal/src/stand.rs) |
| 9 | Auftrag als endlicher Zustandsautomat (reserviert → gesendet → … → Endzustand) | Schützt. Aus einem Endzustand führt kein Weg heraus; gesendet wird höchstens einmal. | [auftrag.rs](../crates/depot_journal/src/auftrag.rs) |
| 10 | KI-Budget: Vorab-Reservierung zum Höchstbetrag, Abrechnung nach echten Kosten | Schützt. Ungeklärte Reservierungen bleiben voll gebucht. | [depot_ki](../crates/depot_ki/src/lib.rs), [kasse.rs](../crates/depot_journal/src/kasse.rs) |
| 11 | Tagesrahmen: $r(d) = \max\bigl(0,\ \min(\lfloor \max(0, M - V_d) / n_d \rfloor,\ D) - H_d\bigr)$ mit Monatsbudget $M$, Kosten vor dem Tag $V_d$, Resthandelstagen $n_d$, Deckel $D$, heutigen Kosten $H_d$ | Schützt. Die Monatsgrenze hält; die Gleichverteilung an börsenfreien Tagen ist offen (A3 in #100). | [tagesrahmen.rs](../crates/depot_ki/src/tagesrahmen.rs) |
| 12 | Abstimmung Dachs/Maus: ausgeführt wird nur ein Vorschlag $v$ mit Autor hält $v$ in der letzten Runde $\land$ Partner stimmt genau $v$ zu | Schützt (Vier-Augen-Prinzip, R31). | [abstimmung.rs](../crates/depot_sim/src/abstimmung.rs) |
| 13 | Weltprognose: Richtung, Horizont 1–20 Handelstage, Ausgangskurs fixiert vor dem Ergebnis | Misst. | [weltprognose.rs](../crates/depot_journal/src/weltprognose.rs) |
| 14 | Auflösung der Prognosen: Zieltag als $h$-ter Handelstag im Kalender der Börse des Werts, Schlusskurs genau dieses Tages, Gleichstand = verfehlt | Misst. Die Auflösung folgt allein aus dem Stand; jede andere ist Halt. | [aufloesung.rs](../crates/depot_journal/src/aufloesung.rs) |
| 15 | Wilson-Intervall 95 % ganzzahlig: $\frac{k + z^2/2 \pm z\sqrt{k(n-k)/n + z^2/4}}{n + z^2}$ mit $z^2 = 3{,}8416$, Wurzel aufgerundet, Untergrenze ab-, Obergrenze aufgerundet; Vergleich mit „steigt immer“ auf denselben Fällen | Misst. „Besser als naiv“ erst, wenn die Untergrenze über der naiven Quote liegt; daraus folgt die Mindestfallzahl. Hinweis, nie Regel. | [aufloesung.rs](../crates/depot_journal/src/aufloesung.rs) |
| 16 | Eigenschaftstests: vollständige Aufzählung kleiner Modelle und Zufallsfolgen mit festem Startwert (xorshift64) | Prüft die Invarianten unten an allen Folgen statt an Einzelfällen. Ohne Abhängigkeit. | [journal](../crates/depot_journal/tests/invarianten.rs), [Abstimmung und Wiederanlauf](../crates/depot_sim/tests/invarianten.rs) |

## Zentrale Invarianten

Diese Sätze müssen für **jeden** Ablauf gelten, nicht nur für die ausgedachten Testfälle. Sie werden mit Verfahren 16 geprüft (#111, #112); jede Prüfung ist mit einer Gegenprobe belegt, die bei verletztem Satz rot wird.

1. **Geld:** Bargeld und Bestände sind nie negativ. Bewertung ≤ exakter Wert ≤ Deckung. Kauflimit ≤ exakter Kurs ≤ Verkaufslimit.
2. **Budget:** Für jede Folge von Reservierungen und Abrechnungen gilt je KI: Gebuchtes + Reserviertes ≤ Grenze für Tag, Monat und Jahr. Die Summe der Tagesrahmen eines Monats ist ≤ $M$.
3. **Abstimmung:** Ein Vorschlag, den sein Autor geändert oder zurückgezogen hat, wird nie ausgeführt. Ein Abbruch, eine Budgetsperre oder ein Fehler kann keine neue Zustimmung begründen. Ein bereits erklärter Widerruf bleibt wirksam.
4. **Auftrag:** Kein Übergang aus einem Endzustand. Höchstens ein Versand je Auftrag. Summe der Ausführungen ≤ beauftragte Menge. Jede Ausführung hält das Limit.
5. **Journal:** Laden nach Schreiben ergibt denselben Zustand. Ein Abbruch nach einem beliebigen Eintrag führt nach dem Neustart zu einem gültigen Zustand. Ein doppelt eingespielter Beleg ändert nichts. Jeder Beleg lässt sich verlustfrei lesen und wieder schreiben ($\text{lies}(\text{text}(b)) = b$).

## Geplant

| Vorhaben | Mathematik | Rolle |
|---|---|---|
| Rundung als Intervall | Jeder Betrag als $[u, o]$; Grenzen prüfen immer die ungünstige Seite | Prüft |
| Ausfall der Zusatz-KIs | Sicherungsschalter (zu / offen / halboffen) mit verdoppelter Wartezeit; Satz: Ergebnis mit ausgefallener Zusatz-KI = Ergebnis ohne sie | Schützt |
| Wirkungskarte (R41) | Gerichteter Graph Größe → Wert mit Vorzeichen; Relevanz = Erreichbarkeit, Ordnung = Pfadlänge; Kreisfreiheit geprüft | Entscheidet über Relevanz |
| Brier-Wert der Prognosen | Sobald eine Prognose eine Zuversicht nennt | Misst |
| Reaktion auf Ereignisse (R40) | Rendite des Werts minus Rendite des Vergleichsindex im Beobachtungsfenster | Misst |
| Entdoppeln von Schlagzeilen | Wort-Schindeln und Jaccard-Ähnlichkeit, deterministisch | Misst |

## Bewusst nicht

- **Keine Kursprognose als Formel.** Dachs und Maus entscheiden aus der Beobachtung, mit Begründung im Journal. Die Mathematik misst nur, wie gut das war.
- **Kein TLA+.** Die Abstimmung läuft mit zwei Teilnehmern nacheinander im selben Prozess; Eigenschaftstests reichen.
- **Kani erst später** und nur für kleine reine Funktionen (Cent-Rechnung, Auftragsübergang). Kani läuft nicht unter Windows, nur in der Linux-CI.

## Offene Grenzen

- `Cent::anteil_bp_ab` rundet Richtung null. Für negative Beträge (Differenzen) ist das eine Aufrundung. Klären, ob das gewollt ist, oder die Benennung anpassen.
- Der Tagesrahmen verteilt an börsenfreien Tagen und nach dem letzten Handelstag ungleichmäßig (A3 in #100).
- Die Hashkette beweist Unverändertheit, nicht Vollständigkeit gegenüber der Außenwelt. Dafür bleibt der Abgleich mit dem Broker.

## Quellen

- J. Meeus: *Astronomical Algorithms*, Kap. 8 (Osterdatum).
- E. B. Wilson (1927): Probable inference, the law of succession, and statistical inference. *JASA* 22.
- G. W. Brier (1950): Verification of forecasts expressed in terms of probability. *Monthly Weather Review* 78.
- K. Claessen, J. Hughes (2000): QuickCheck. *ICFP* (Grundlage von `proptest`).
- M. Nygard: *Release It!* (Sicherungsschalter).
