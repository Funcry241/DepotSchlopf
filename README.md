# DepotSchlopf

Rust-Rahmen, in dem zwei KIs (Dachs = Claude, Maus = GPT) je einen Topf eines echten Depots verwalten.

**KI schlägt vor. Regelwerk entscheidet. Auftragsstelle schreibt. Journal belegt. Abgleich prüft.**

## Stand

| Crate | Inhalt |
|---|---|
| `depot_kern` | Geld in Cent (geprüfte Rechnung), ISIN mit Prüfziffer, Töpfe, Kurse, Vorschläge, Grenzen mit Fingerabdruck, Positivliste |
| `depot_regelwerk` | Reine Funktion ohne Netz, Dateien oder Uhr: prüft Vorschläge und erzeugt `GenehmigterAuftrag` (nur hier erzeugbar). Dazu Kursprüfung, Wächter (Tagesverlust, Notbremse) |

## Harte Grenzen (Standard)

- höchstens 25 % je Wert, höchstens 8 Werte, höchstens 4 Orders je Topf und Zyklus
- Mindestorder 500 € (Komplettverkauf ausgenommen), Umschichtung erst ab 5 Prozentpunkten
- Umschlag höchstens 50 % des Topfes je Zyklus
- nur Limit-Orders, Limit höchstens 1 % vom Schlusskurs
- Kurssprung über 15 % = unplausibel, Zyklus fällt aus
- Tagesverlust 5 % = Halt, 20 % unter Höchststand = Notbremse (alles verkaufen)
- Verkäufe zuerst; Käufe nur aus gesichertem Bargeld; keine Hebel, keine Derivate, keine Leerverkäufe

## Prüfen

```
cargo fmt --check
cargo clippy --all-targets -- -D warnings
cargo test
```
