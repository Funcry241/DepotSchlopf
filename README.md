# DepotSchlopf

Rust-Rahmen, in dem zwei KIs (Dachs = Claude, Maus = GPT) je einen Topf eines echten Depots verwalten.

**Essenz lenkt. KI schlägt vor. Regelwerk entscheidet. Auftragsstelle schreibt. Journal belegt. Abgleich prüft. Kodex wacht.**

Neu hier? Zuerst [docs/kaltstart.md](docs/kaltstart.md) lesen.

## Stand

| Crate | Inhalt |
|---|---|
| `depot_kern` | Geld in Cent (geprüfte Rechnung), ISIN mit Prüfziffer, Töpfe, Kurse, Vorschläge, Grenzen mit Fingerabdruck, Positivliste |
| `depot_ki` | Harter KI-Budgetwächter: Kassenbuch auf der Platte, Erlaubnis nur nach Buchung der Höchstkosten, Grenzen je Zyklus, Tag, Monat, Jahr |
| `depot_kodex` | Das Gate (`run.cmd kodex`): Deklaration gegen realen Diff, Pfadrollen, Festschreibung, Quellscan, Essenz, Format, Clippy, Tests |
| `depot_regelwerk` | Reine Funktion ohne Netz, Dateien oder Uhr: prüft Vorschläge und erzeugt `GenehmigterAuftrag` (nur hier erzeugbar). Dazu Kursprüfung, Wächter (Tagesverlust, Notbremse) |

## Harte Grenzen (Standard)

- höchstens 25 % je Wert, höchstens 8 Werte, höchstens 4 Orders je Topf und Zyklus
- Mindestorder 500 € (Komplettverkauf ausgenommen), Umschichtung erst ab 5 Prozentpunkten
- Umschlag höchstens 50 % des Topfes je Zyklus
- nur Limit-Orders, Limit höchstens 1 % vom Schlusskurs
- Kurssprung über 15 % = unplausibel, Zyklus fällt aus
- Tagesverlust 5 % = Halt, 20 % unter Höchststand = Notbremse (alles verkaufen)
- Verkäufe zuerst; Käufe nur aus gesichertem Bargeld; keine Hebel, keine Derivate, keine Leerverkäufe

## Essenz und Kodex

- [docs/essenz.md](docs/essenz.md): Erfahrungsregeln D00–D17. Beratend, vor jeder Änderung lesen.
- Kodex: entscheidet nach jeder Änderung. Jede Änderung deklariert sich in `kodex/aenderung.txt`. Harte Grenzen, KI-Budget, Produktarten, Abhängigkeiten und Lints stehen in `kodex/festschreibung.txt` und ändern sich nur mit Freigabe des Eigentümers.

## Prüfen

```
run.cmd kodex
```

Dasselbe läuft auf jedem Pull Request in GitHub Actions.
