# Kodex-Schritt – Negativtests und Umbau

Stufe 2, Kodex-Schritt. Er besteht aus zwei Änderungen, weil der Kodex eine Kodex-Änderung nie zusammen mit Produktivcode zulässt:

1. **Teil 1 (`RefactorOnly`, PR #6):** Die harten Lints stehen im Kopf von `depot_kern`, `depot_regelwerk`, `depot_ki` und `depot_lauf` auf `forbid`.
2. **Teil 2 (`KodexAenderung`, dieser Schritt):** Erster Commit nur die Negativtests (rot), danach der Umbau des Kodex, bis sie grün sind.

Ein grüner Test schließt keinen Befund. „Technisch geschlossen“ setzt erst die unabhängige Prüfung durch Maus.

## So prüfst du es nach

```text
cargo test --locked -p depot_kodex --test befunde
```

Am ersten Commit von Teil 2 (Tests mit `#[ignore]`) zeigt `-- --ignored` alle zehn Tests rot. Nach dem Umbau laufen sie ohne `ignore` im Gate. Die vier Gegenbeispiele aus der Prüfung von PR #7 (K7-01 bis K7-03) kamen als eigener roter Commit dazu (`1eed183`) und laufen jetzt ebenfalls im Gate: zusammen 14 Tests.

Jeder Test baut eine Kopie des Arbeitsbaums unter `target/kodex-befunde/` mit eigener Git-Basis. Vor jedem Verstoß besteht dieselbe Kopie mit einer harmlosen, sauber deklarierten Änderung das Gate (Kontrolle). Ein Verstoß gilt nur als abgelehnt, wenn die Ablehnung das erwartete Stichwort nennt. Mehrere Fälle eines Befunds laufen einzeln; jeder muss für sich abgelehnt werden.

## Ergebnis

| Befund | Test (`depot_kodex`, `tests/befunde.rs`) | am alten Kodex | nach dem Umbau |
| --- | --- | --- | --- |
| 1·G01 | `befund_1_g01_ausnahme_hinter_cfg_attr_wird_abgelehnt`: `cfg_attr(not(test), allow)`, `cfg_attr(all(), expect)`, verschachteltes `cfg_attr`, Gruppen `restriction` und `pedantic`, `unsafe_code` hinter `cfg_attr` | rot: alle sechs Fälle angenommen | grün |
| 1·G01 | `befund_1_g01_kopf_ohne_forbid_wird_abgelehnt`: `forbid` entfernt, hinter `cfg_attr`, ohne `clippy::panic` | rot: alle drei angenommen | grün |
| 1·G01, 2·A11 | `befund_1_g01_compiler_lehnt_ausnahme_in_jedem_target_ab`: bewusster Verstoß (`allow` und `expect` hinter `cfg_attr`) in jedem Produktiv-Target, `cargo clippy -D warnings`; nur `E0453` zählt. Die Targets kommen aus `cargo metadata` (keine feste Liste) und müssen genau den Wurzeln entsprechen, die der Kodex prüft | rot: `depot_kodex` Bibliothek und Programm übersetzen (die vier übrigen Targets sind seit Teil 1 geschützt) | grün |
| 1·G02, 2·A11 | `befund_1_g02_reinheit_haelt_gegen_gruppierte_importe`: gruppiert, umbenannt, `self as`, `extern crate std as`, Glob aus `std`, `use std as s` | rot: fünf von sechs angenommen | grün |
| 1·G03 | `befund_1_g03_vorgetaeuschte_lint_vererbung_wird_abgelehnt`: Vererbung nur in einem mehrzeiligen Text; Vererbung abgeschaltet | rot: der Text-Fall angenommen | grün |
| 1·G04 | `befund_1_g04_abhaengigkeit_mit_anderer_identitaet_wird_abgelehnt`: andere Features, anderes Paket unter gleichem Namen, andere Version, veränderte `Cargo.lock` | rot: alle vier angenommen | grün |
| 1·G05 | `befund_1_g05_geloeschte_oder_umbenannte_regel_wird_abgelehnt`: D17 gelöscht, alles ab D01 gelöscht, D05 umbenannt | rot: alle drei angenommen | grün |
| 1·G06 | `befund_1_g06_unicode_pfade_werden_richtig_geprueft`: deklarierte Datei mit Umlaut und Leerzeichen; undeklarierte wird mit echtem Namen gemeldet | rot: Pfad quotiert, deklarierte Datei abgelehnt | grün (Kodex-Teil) |
| 1·R03 | `befund_1_r03_lesefehler_beim_scan_macht_den_kodex_rot`: `tests` ist kein Ordner; Link ins Leere im Quellbaum; Ordner ohne Leserecht | rot: beide ersten Fälle übersprungen | grün |
| 1·R08 | `befund_1_r08_freigabe_ohne_passenden_hash_wird_abgelehnt`: ohne Freigabe, Freigabe für eine andere Festschreibung, nach der Freigabe weiter geändert; Gegenstück: passende Freigabe besteht | rot: alle drei angenommen | grün |
| K7-01 (1·G02) | `befund_k7_01_alias_in_anderer_funktion_verdeckt_std_nicht`: `use std as basis` in einer Funktion, `use core as basis` in einer anderen (`scope_alias`, unverändert aus der Prüfung) | rot: angenommen | grün |
| K7-02 (1·G02) | `befund_k7_02_interner_reexport_von_std_wird_abgelehnt`: `pub use std as platform` im Modul, Zugriff über `crate::geld::audit_bridge::platform::fs` (`local_reexport`) | rot: angenommen | grün |
| K7-03 (2·A11) | `befund_k7_03_target_aus_md_wird_geprueft`: `[[bin]]` mit `path = "src/audit_target.md"` und `std::fs` (`target_md`) | rot: angenommen | grün |
| K7-03 (2·A11) | `befund_k7_03_modul_aus_md_wird_geprueft`: `#[path = "audit_payload.md"] pub mod audit_payload;` mit `std::fs` (`module_md`) | rot: angenommen | grün |

Der Fall „Ordner ohne Leserecht“ läuft nur, wenn das Lesen wirklich scheitert. Unter Administratorrechten meldet der Test „nicht ausgeführt“; die beiden anderen Fälle laufen immer.

## Gegenproben am neuen Kodex

Jede Zeile schwächt den neuen Kodex an genau einer Stelle und zeigt, dass der zuständige Test dann rot wird.

| Gegenprobe | Ergebnis |
| --- | --- |
| Kopf von `depot_kodex` mit `deny` statt `forbid` | Compilertest rot |
| `cfg_attr` wird nicht ausgewertet | G01-Attributtest rot |
| Gruppen `restriction` und `pedantic` gelten nicht als hart | G01-Attributtest rot |
| ein `forbid` hinter `cfg_attr` zählt als Kopf | G01-Kopftest rot |
| Import-Aliasse werden nicht aufgelöst | G02 rot |
| `[lints]` wird nicht semantisch geprüft | G03 rot |
| Abhängigkeiten nur nach Alias festgeschrieben (Festschreibung neu berechnet) | G04 rot: Features, Paket und Version angenommen |
| Essenz ohne Register | G05 rot |
| Pfadliste ohne `-z` | G06 rot |
| Links im Quellbaum erlaubt | R03 rot |
| Lesefehler eines Ordners übersprungen, Nicht-Ordner still ignoriert | R03 rot |
| Freigabe nicht an den Diff gebunden | R08 rot |
| Alias behält nur eine Bedeutung **und** mehrdeutige Aliasse erlaubt | K7-01 rot |
| Einbindung von `std` als Ganzes in reinen Crates erlaubt | K7-02 rot |
| Targetpfade ohne `.rs`-Pflicht, Nicht-`.rs`-Dateien in `src/` erlaubt | K7-03 Target rot |
| `#[path]` erlaubt und Nicht-`.rs`-Dateien in `src/` erlaubt | K7-03 Modul rot |
| Kodex kennt `src/main.rs` nicht als Wurzel | Compilertest rot (Abgleich mit `cargo metadata`) |

Bei K7-01 sind zwei Schichten absichtlich doppelt: Jede Bedeutung eines Alias wird geprüft, und ein Alias mit mehreren Bedeutungen im reinen Crate wird abgelehnt. Schaltet man nur eine Schicht ab, bleibt der Test grün; erst beide zusammen machen ihn rot. Die oben genannten zwölf früheren Gegenproben wurden nach dem K7-Umbau wiederholt und sind wieder rot.

## Was der Umbau tut

- **Lexer und Syntax (`lexer.rs`, `rust.rs`):** Rust-Code wird in Token zerlegt; Kommentare und Literale sind kein Code. Lint-Attribute werden samt `cfg_attr` (auch verschachtelt) ausgewertet, ohne die Bedingung als falsch anzunehmen. Importe werden aufgelöst: Gruppen, `self`, `as`, Glob, `extern crate … as …`. Ein Alias behält **alle** Bedeutungen, die er irgendwo in der Datei hat; jede wird geprüft. Auch `#[path]`, die versteckt in `cfg_attr` steht, wird erkannt.
- **Quellscan (`quellscan.rs`):** Jedes Produktiv-Target hat am Dateianfang ein unbedingtes `#![forbid(..)]` mit allen neun harten Lints, auch der Kodex selbst. Keine Datei setzt einen harten Lint oder die Gruppen `clippy::restriction`, `clippy::pedantic`, `warnings` herab. Reine Crates verwenden kein `std::fs`, `std::net`, `std::env`, `std::time`, `std::process`, `std::thread` und hängen nur von reinen Crates und `sha2` ab. In reinen Crates ist ein Alias mit mehreren Bedeutungen verboten (konservativ, K7-01), ebenso jede Einbindung von `std` als Ganzes (`use std as …`, `pub use std as …`, `extern crate std as …`), weil sie `std` weiterreichen kann (K7-02). `include!` und `#[path]` sind überall verboten (K7-03). Ein Crate-Ordner enthält nur `Cargo.toml`, `src/` und `tests/`; darin liegen nur `.rs`-Dateien, und jede wird vollständig geprüft. So bestimmt nicht die Endung, was geprüft wird, sondern alles, was der Compiler lesen kann, ist `.rs` und wird gescannt. Lesefehler, Links und Sonderdateien sind Fehler. Der Kodex prüft jetzt auch sich selbst.
- **Manifeste (`toml.rs`, `manifest.rs`):** Ein strenger TOML-Leser. `[lints]` jedes Crates muss genau `workspace = true` sein; der Workspace setzt jeden harten Lint auf `deny` oder `forbid`. Produktiv-Targets liegen unter `src/`, Test-, Bench- und Beispiel-Targets unter `tests/`, jeder Targetpfad endet auf `.rs`; Build-Skripte sind verboten.
- **Festschreibung:** Abhängigkeiten mit voller Identität (Abschnitt, Alias und kanonischer Eintrag mit Version, Paket, Quelle, Features); neuer Schlüssel `cargo_lock` (Hash von `Cargo.lock`); Lints semantisch. Windows-Zeilenenden ändern nichts.
- **Grenzfreigabe (`freigabe.rs`, R30):** Jede Änderung von `kodex/festschreibung.txt` braucht in derselben Änderung eine neue `kodex/freigabe.txt`, gebunden an den Diff und an den Hash der neuen Festschreibung. Die Klassen `FestschreibungAenderung` und `KodexAenderung` dürfen die Festschreibung ändern; eine `KodexAenderung` kann dabei nur die Berechnung ändern, nicht Grenzen, Budgets oder Produktarten, denn die liegen im Produktivcode. Die Vorlage erzeugt `cargo run -p depot_kodex -- freigabe --basis <basis>`.
- **Essenz-Register:** `kodex/essenz_register.txt` nennt D00 bis D17 mit Titel. Fehlt eine Regel, ist sie umbenannt oder kommt eine ohne Registereintrag dazu, ist der Kodex rot. Der Kaltstart muss den Bereich aus dem Register nennen.
- **Git (`git.rs`):** Pfadlisten werden NUL-getrennt und ohne Umbenennungserkennung gelesen (eine Umbenennung zeigt beide Pfade).

## Grenzen, die ausdrücklich bleiben

- **Makros und Reexporte:** Der Kodex expandiert keine Makros und verfolgt keine Reexporte fremder Crates. Dafür stehen die Crate-Grenzen der reinen Crates (nur reine Crates und `sha2`), das Verbot von `include!` und `#[path]` und das Verbot, `std` im reinen Crate als Ganzes einzubinden. Interne Weiterreichungen einzelner unbedenklicher `std`-Module (etwa `std::fmt`) bleiben erlaubt; verbotene Module werden an jeder Stelle des aufgelösten Pfads erkannt.
- **Urheberschaft der Freigabe (1·R08):** Der Kodex prüft die Bindung an Diff und Festschreibung, nicht, wer die Datei geschrieben hat. Dass Wolfgang sie erteilt, sichern die Regeln auf GitHub (nur Wolfgang führt zusammen). Eine maschinenprüfbare Unterschrift wäre ein eigener späterer Schritt.
- **Export (1·G06, 1·R03):** Der Export in `depot_lauf` liest Pfade noch zeilenweise und überspringt Lesefehler. Das ist Produktivcode und gehört zum Schnitt „Export“.

## Netzzugang für den Sammler (Kap. 6)

Wolfgang hat am 04.11.2026 die lesende Datenversorgung freigegeben. Den Abruf übernimmt das Crate `depot_sammler`; es steht damit als einziges auf der Liste `NETZ_ERLAUBT`. Damit das Netz nicht über eine Abhängigkeit in andere Crates wandert, gilt jetzt eine allgemeine Regel: Kein Crate außerhalb der Liste hängt von einem Netz-Crate (`reqwest`, `ureq`, `hyper`, `tokio`, `socket2`) oder von einem Crate der Liste ab. Das gilt für jeden Abhängigkeitsabschnitt, auch für zielsystemabhängige, und auch unter anderem Namen (`package = …`). Der Leser der Abhängigkeitsnamen nennt dafür jetzt auch in zielsystemabhängigen Abschnitten das umbenannte Paket.

| Test | Datei | zeigt |
| --- | --- | --- |
| `netz_nur_im_sammler` | `crates/depot_kodex/src/quellscan.rs` | Netz im Sammler erlaubt, in Journal und Lauf abgelehnt |
| `netz_bleibt_im_blatt` | `crates/depot_kodex/tests/befunde.rs` | Netz-Crate direkt, umbenannt nur für Windows, und Abhängigkeit vom Sammler: jeweils abgelehnt |

Gegenproben, jeweils neu übersetzt: ohne die neue Regel scheitern alle drei Fälle von `netz_bleibt_im_blatt`; ohne das umbenannte Paket in zielsystemabhängigen Abschnitten scheitert der zweite Fall.

Grenze: Ein Crate, das ein anderes Programm startet (`std::process`), könnte darüber ins Netz. Außerhalb der reinen Crates ist `std::process` erlaubt, weil `depot_lauf` Git und Cargo startet. Diese Grenze sichert weiter die Durchsicht jeder Änderung.

## Netzzugang für die Berater (Papierhandel)

Wolfgang hat am 05.11.2026 freigegeben, dass Dachs (Claude) und Maus (GPT) über das Netz gefragt werden, mit Websuche („Ja, mit Websuche“). Das übernimmt das neue Crate `depot_berater`; es steht als zweites auf der Liste `NETZ_ERLAUBT`. Die allgemeine Regel bleibt: Kein Crate außerhalb der Liste hängt von einem Netz-Crate oder einem Crate der Liste ab. `depot_berater` darf also von `depot_sim` abhängen, nie umgekehrt; Journal, Regelwerk und Simulation bleiben ohne Netz. Das Crate selbst folgt mit einer eigenen Änderung (Festschreibung, Grenzfreigabe).

| Test | Datei | zeigt |
| --- | --- | --- |
| `netz_nur_im_sammler_und_berater` | `crates/depot_kodex/src/quellscan.rs` | Netz in Sammler und Berater erlaubt, in Simulation, Journal und Lauf abgelehnt |
| `netz_bleibt_im_blatt`, Fall „ueber den Berater“ | `crates/depot_kodex/tests/befunde.rs` | Abhängigkeit vom Berater: abgelehnt |

## Nur lokal lauschen: der Dienst (Oberfläche)

Wolfgang hat am 05.11.2026 freigegeben, dass DepotSchlopf als Dienst mit einer lokalen Weboberfläche läuft, erreichbar nur vom eigenen Rechner („Ja, nur 127.0.0.1“). Das übernimmt das kommende Crate `depot_dienst`. Es steht nicht auf `NETZ_ERLAUBT`, sondern auf einer eigenen, engeren Liste `NETZ_LOKAL`:

- `TcpListener::bind` nur wörtlich auf `Ipv4Addr::LOCALHOST`, als Tupel `(Ipv4Addr::LOCALHOST, port)` oder in `SocketAddrV4::new(Ipv4Addr::LOCALHOST, port)`, jeweils auch mit `std::net::` davor. Direkt danach folgt `,` oder `)`: Die Adresse steht im Quelltext, nie in einer Variable, einer Zeichenkette oder einer Einstellung, und kein Methodenaufruf verändert sie.
- Angenommene Verbindungen (`TcpStream` aus `accept`) sind erlaubt. Verboten sind `connect`, `connect_timeout`, `UdpSocket`, Unix-Sockets, `ToSocketAddrs`/`to_socket_addrs`, `lookup_host` und jedes Netz-Crate.
- Kein anderes Crate hängt von `depot_dienst` ab. Jedes andere Crate darf auch lokal nicht lauschen.

Den Schutz der Seite gegen Aufrufe fremder Webseiten (Host- und Origin-Prüfung, Token) bringt `depot_dienst` selbst mit; der Kodex sichert nur, dass nichts von außen den Dienst erreicht und der Dienst nicht hinaus kann.

| Test | Datei | zeigt |
| --- | --- | --- |
| `dienst_lauscht_nur_lokal` | `crates/depot_kodex/src/quellscan.rs` | erlaubt: Tupel, `SocketAddrV4::new` mit `std::net::`, angenommene Verbindung; abgelehnt: `UNSPECIFIED`, Zeichenkette, Variable, `LOCALHOST.to_ipv6_mapped()`, `connect`, `UdpSocket`, `to_socket_addrs`, Netz-Crate; lokales Lauschen in `depot_lage` abgelehnt |
| `netz_bleibt_im_blatt`, Fall „ueber den Dienst“ | `crates/depot_kodex/tests/befunde.rs` | Abhängigkeit vom Dienst: abgelehnt |

Gegenproben, jeweils neu übersetzt: Ohne den Eintrag in `NETZ_LOKAL` scheitern die erlaubten Fälle; ohne die Prüfung von `bind` gehen `UNSPECIFIED`, Zeichenkette und Variable durch; ohne `NETZ_LOKAL` in der Abhängigkeitsprüfung geht „ueber den Dienst“ durch.

Grenze wie beim Sammler: Ein Programmstart über `std::process` bleibt außerhalb der reinen Crates erlaubt und wird in der Durchsicht geprüft.
