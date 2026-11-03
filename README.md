# DepotSchlopf

[![Kodex](https://github.com/Funcry241/DepotSchlopf/actions/workflows/kodex.yml/badge.svg)](https://github.com/Funcry241/DepotSchlopf/actions/workflows/kodex.yml)
[![Lizenz: MIT](https://img.shields.io/badge/Lizenz-MIT-blue.svg)](LICENSE)
![Rust 1.98](https://img.shields.io/badge/Rust-1.98-orange.svg)
![Status: experimentell](https://img.shields.io/badge/Status-experimentell-red.svg)

Rust-Rahmen, in dem zwei KIs je einen Topf eines echten Aktiendepots beraten: **Dachs** (Claude) und **Maus** (GPT). Die KIs schlagen nur Zielgewichte vor. Ein festes, getestetes Regelwerk entscheidet, was wirklich gehandelt wird. Harte Grenzen gelten für Risiko, Handelskosten und KI-Kosten.

> **Essenz lenkt. KI schlägt vor. Regelwerk entscheidet. Auftragsstelle schreibt. Journal belegt. Abgleich prüft. Kodex wacht.**

> [!WARNING]
> Experimentelles Privatprojekt. **Keine Anlageberatung**, keine Steuerberatung. Totalverlust ist möglich. Siehe [Risiko- und Haftungshinweis](docs/RISIKO.md).

Neu hier? Zuerst [docs/kaltstart.md](docs/kaltstart.md) lesen.

## Was DepotSchlopf garantiert (Standardwerte)

| Gefahr | Was im Code dagegen steht |
|---|---|
| KI erfindet Werte oder verlangt Unmögliches | Vorschlag wird ganz verworfen; nur Werte der Positivliste |
| Zu viel in einem Wert | höchstens 25 % je Wert, höchstens 8 Werte |
| Produkte mit Verlust über dem Einsatz | nur Aktien (ETFs vorbereitet, gesperrt); keine Derivate, kein Hebel, kein Leerverkauf, nur Bargeld |
| Gebühren fressen den Ertrag | Mindestorder 500 €, Umschichtung erst ab 5 Prozentpunkten, höchstens 4 Orders je Topf und Zyklus, Umschlag höchstens 50 % |
| Absturz des Marktes | Tagesverlust 5 % = Halt; 20 % unter Höchststand = Notbremse, alles verkaufen |
| KI-Kosten laufen aus dem Ruder | Erlaubnis erst nach dauerhafter Buchung der Höchstkosten; je KI 2 Aufrufe je Zyklus, 1 Zyklus je Tag, 4 Aufrufe und 4 € je Monat, 50 € je Jahr |
| Grenzen werden heimlich gelockert | Kodex vergleicht jede Änderung mit `kodex/festschreibung.txt` und lehnt ab |
| Neue KI-Sitzung kennt die Vorgeschichte nicht | jeder Lauf schreibt `assistant_export_latest.zip` mit Übergabe und Quellstand |
| Unklare Lage | fail-closed: Halt statt Raten |

## Aufbau

| Crate | Inhalt |
|---|---|
| `depot_kern` | Geld in Cent (geprüfte Rechnung), ISIN mit Prüfziffer, Töpfe, Kurse, Vorschläge, Grenzen mit Fingerabdruck, Positivliste |
| `depot_regelwerk` | Reine Funktion ohne Netz, Dateien oder Uhr: prüft Vorschläge und erzeugt `GenehmigterAuftrag` (nur hier erzeugbar). Dazu Kursprüfung und Wächter |
| `depot_journal` | Das Journal als einzige dauerhafte Datei: Hash-Kette mit Anhangsvermerk, geprüfte Wiederherstellung; Papierdepot, Aufträge, Beobachtungen und KI-Buchungen |
| `depot_ki` | Regeln des KI-Budgets ohne Datei; das Kassenbuch steht im Journal, eine Erlaubnis gibt es nur nach Buchung der Höchstkosten dort |
| `depot_lauf` | Der Ein-Befehl-Einstieg `run.cmd` mit Assistenten-Export nach jedem Lauf |
| `depot_kodex` | Das Gate: Deklaration gegen realen Diff, Pfadrollen, Festschreibung, Quellscan, Essenz, Format, Clippy, Tests |

Geplant: Backtest, Berater-Anbindung (Claude, GPT), Steuer-Export aus dem Journal, Broker-Adapter (Interactive Brokers, Cash-Konto), Papierhandel.

## Essenz und Kodex

- [Essenz](docs/essenz.md): Erfahrungsregeln D00–D17. Beratend, vor jeder Änderung lesen.
- Kodex: entscheidet nach jeder Änderung. Jede Änderung deklariert sich in `kodex/aenderung.txt`. Harte Grenzen, KI-Budget, Produktarten, Abhängigkeiten und Lints stehen in `kodex/festschreibung.txt` und ändern sich nur mit Freigabe des Eigentümers.

## Schnellstart

Voraussetzung: [Rust](https://rustup.rs/) (die Version wird über `rust-toolchain.toml` automatisch gewählt) und Git.

```
git clone https://github.com/Funcry241/DepotSchlopf.git
cd DepotSchlopf
run.cmd
```

Ein Befehl macht alles: Gate prüfen (Struktur, Festschreibung, Quellscan, Essenz, Format, Clippy, Tests) und danach `assistant_export_latest.zip` schreiben. Diese Datei kann man einer neuen KI-Sitzung geben. Unter Linux oder macOS: `cargo run -p depot_lauf`.

## Mitmachen

[Beitragsregeln](docs/CONTRIBUTING.md) · [Sicherheit](docs/SECURITY.md) · [Umgang miteinander](docs/CODE_OF_CONDUCT.md) · [Änderungen](CHANGELOG.md)

## Lizenz

[MIT](LICENSE) © 2026 Funcry241
