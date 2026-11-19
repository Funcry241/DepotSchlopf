//! Lesender Einzelabruf beim lokalen IB Gateway für das ausdrücklich angegebene Papierkonto.

pub(super) mod abgleich;
mod diagnose;

use std::collections::{BTreeMap, BTreeSet};
use std::path::Path;
use std::process::ExitCode;
use std::time::{Duration, Instant};

use depot_kern::Cent;
use depot_kern::ibkr_papier::{KONTO_DATEI, kontonummer_gueltig};
use ibapi::Client;
use ibapi::accounts::types::AccountGroup;
use ibapi::accounts::{AccountSummary, AccountSummaryResult, AccountSummaryTags, PositionUpdate};
use ibapi::orders::{Action, Orders};
use ibapi::subscriptions::SubscriptionItem;

const ADRESSE: &str = "127.0.0.1:4002";
const CLIENT_ID: i32 = 356;
const FRIST: Duration = Duration::from_secs(15);
const GATEWAY_NICHT_BEREIT: &str =
    "Lokales IB Gateway auf Port 4002 nicht erreichbar oder nicht bereit.";
const KONTO_NICHT_PASSEND: &str =
    "Im Gateway muss genau das konfigurierte IBKR-Papierkonto angemeldet sein.";
const KONTOLISTE_FEHLT: &str = "Verwaltete Konten konnten beim Gateway nicht abgerufen werden.";
const KONTOLISTE_TIMEOUT: &str = "Gateway antwortet nicht auf die Anfrage nach verwalteten Konten.";
const KONTOLISTE_VERBINDUNG: &str = "Verbindungsfehler beim Abruf der verwalteten Konten.";
const KONTOLISTE_FORMAT: &str = "Antwortformat der verwalteten Konten ist nicht lesbar.";
const EUR_FEHLT: &str = "Der einzelne EUR-Saldo im Währungsledger fehlt.";
const KONTO_DATEI_FEHLT: &str = "Lokale Datei daten/ibkr-papierkonto.txt fehlt oder ist unlesbar.";
const KONTO_FORMAT: &str = "In daten/ibkr-papierkonto.txt wird eine Papierkontonummer mit DU oder DUR und danach nur Ziffern erwartet.";

#[derive(Debug, PartialEq, Eq)]
struct Salden {
    gesamt: Cent,
    abgerechnet: Option<Cent>,
    verfuegbar: Option<Cent>,
}

fn saldo_text(wert: Option<Cent>) -> String {
    wert.map_or_else(
        || "nicht geliefert".into(),
        |cent| format!("{} Cent", cent.0),
    )
}

#[derive(Debug, PartialEq, Eq, PartialOrd, Ord)]
struct OffeneOrder {
    dauerhafte_id: i64,
    referenz: String,
    kontrakt: i32,
    richtung: &'static str,
    stueck: u64,
}

#[derive(Debug, PartialEq, Eq)]
struct Auszug {
    salden: Salden,
    positionen: BTreeMap<i32, u64>,
    orders: Vec<OffeneOrder>,
}

fn eurocent(text: &str) -> Option<Cent> {
    let (negativ, text) = text.strip_prefix('-').map_or((false, text), |t| (true, t));
    let (euro, nachkomma) = text.split_once('.').unwrap_or((text, ""));
    if euro.is_empty()
        || !euro
            .bytes()
            .chain(nachkomma.bytes())
            .all(|b| b.is_ascii_digit())
    {
        return None;
    }
    let (cent, rest) = nachkomma.split_at(nachkomma.len().min(2));
    if rest.bytes().any(|b| b != b'0') {
        return None;
    }
    let cent = match cent.len() {
        0 => 0,
        1 => cent.parse::<i64>().ok()?.checked_mul(10)?,
        _ => cent.parse().ok()?,
    };
    let gesamt = euro
        .parse::<i64>()
        .ok()?
        .checked_mul(100)?
        .checked_add(cent)?;
    if negativ {
        gesamt.checked_neg().map(Cent)
    } else {
        Some(Cent(gesamt))
    }
}

fn lese_bis<T>(
    mut naechster: impl FnMut(Duration) -> Option<Result<SubscriptionItem<T>, ibapi::Error>>,
    mut belege: impl FnMut(T) -> Result<bool, &'static str>,
    diagnose_aktiv: bool,
) -> Result<(), &'static str> {
    let beginn = Instant::now();
    loop {
        let rest = FRIST.saturating_sub(beginn.elapsed());
        match naechster(rest) {
            Some(Ok(SubscriptionItem::Data(datensatz))) => {
                if belege(datensatz)? {
                    return Ok(());
                }
            }
            Some(Ok(SubscriptionItem::Notice(meldung))) => {
                if diagnose_aktiv {
                    diagnose::zeile(&format!("Gateway-Code {}", meldung.code));
                }
            }
            Some(Err(e)) => {
                diagnose::fehler(diagnose_aktiv, &e);
                return Err("Gateway meldet einen Fehler beim Lesen.");
            }
            None => return Err("Gateway-Auszug unvollständig oder Frist abgelaufen."),
        }
    }
}

fn salden(zeilen: &[AccountSummary], konto: &str) -> Result<Salden, &'static str> {
    let mut summen = BTreeMap::new();
    let mut waehrungen = BTreeSet::new();
    for a in zeilen {
        let kontosumme = matches!(
            a.tag.as_str(),
            AccountSummaryTags::TOTAL_CASH_VALUE
                | AccountSummaryTags::SETTLED_CASH
                | AccountSummaryTags::AVAILABLE_FUNDS
        );
        // $LEDGER:ALL kann Währungszeilen für "All" liefern. Der Aufrufer hat
        // bereits geprüft, dass die Sitzung genau das konfigurierte Konto verwaltet.
        if a.account != konto && (a.account != "All" || kontosumme) {
            return Err("Gateway meldet Daten eines anderen Kontos.");
        }
        if matches!(a.tag.as_str(), "CashBalance" | "$LEDGER-CashBalance") {
            if a.currency == "BASE" {
                continue;
            }
            let cent =
                eurocent(&a.value).ok_or("Ungültiger oder nicht centgenauer Währungssaldo.")?;
            if !waehrungen.insert(a.currency.as_str()) || a.currency.is_empty() {
                return Err("Währungssaldo fehlt oder kommt mehrfach vor.");
            }
            if a.currency != "EUR" && cent != Cent::NULL {
                return Err("Gateway meldet einen Fremdwährungssaldo.");
            }
        } else if kontosumme {
            if a.currency != "EUR" {
                return Err("Kontosummen sind nicht in Euro gemeldet.");
            }
            let cent = eurocent(&a.value).ok_or("Kontosumme nicht centgenau lesbar.")?;
            if summen.insert(a.tag.as_str(), cent).is_some() {
                return Err("Gateway meldet eine Kontosumme mehrfach.");
            }
        }
    }
    if !waehrungen.contains("EUR") {
        return Err(EUR_FEHLT);
    }
    Ok(Salden {
        gesamt: summen
            .get(AccountSummaryTags::TOTAL_CASH_VALUE)
            .copied()
            .ok_or("Gesamtbargeld (TotalCashValue) fehlt in der Gateway-Antwort.")?,
        abgerechnet: summen.get(AccountSummaryTags::SETTLED_CASH).copied(),
        verfuegbar: summen.get(AccountSummaryTags::AVAILABLE_FUNDS).copied(),
    })
}

fn stueck(menge: f64) -> Result<u64, &'static str> {
    menge
        .to_string()
        .parse()
        .map_err(|_| "Gateway meldet keine nichtnegative ganze Stückzahl.")
}

fn konten_pruefen(client: &Client, konto: &str, diagnose_aktiv: bool) -> Result<(), &'static str> {
    let konten = client.managed_accounts().map_err(|e| {
        diagnose::fehler(diagnose_aktiv, &e);
        match e {
            ibapi::Error::Timeout => KONTOLISTE_TIMEOUT,
            ibapi::Error::ConnectionReset
            | ibapi::Error::ConnectionFailed
            | ibapi::Error::Io(_) => KONTOLISTE_VERBINDUNG,
            ibapi::Error::UnexpectedWireFormat(_) | ibapi::Error::ProtobufDecode(_) => {
                KONTOLISTE_FORMAT
            }
            _ => KONTOLISTE_FEHLT,
        }
    })?;
    let passend = matches!(konten.as_slice(), [gemeldet] if gemeldet == konto);
    if diagnose_aktiv {
        diagnose::zeile(&format!(
            "Verwaltete Konten: {} | genau das konfigurierte Konto: {}",
            konten.len(),
            diagnose::passt_zum_start(passend)
        ));
    }
    if !passend {
        return Err(KONTO_NICHT_PASSEND);
    }
    Ok(())
}

fn summen_lesen(
    client: &Client,
    konto: &str,
    diagnose_aktiv: bool,
) -> Result<Salden, &'static str> {
    let mut tags = vec![
        AccountSummaryTags::TOTAL_CASH_VALUE,
        AccountSummaryTags::SETTLED_CASH,
        AccountSummaryTags::AVAILABLE_FUNDS,
        AccountSummaryTags::LEDGER_ALL,
    ];
    if diagnose_aktiv {
        tags.extend([
            AccountSummaryTags::ACCOUNT_TYPE,
            AccountSummaryTags::BUYING_POWER,
        ]);
    }
    let zeilen = client
        .account_summary(&AccountGroup("All".into()), &tags)
        .map_err(|e| {
            diagnose::fehler(diagnose_aktiv, &e);
            "Kontosummen konnten nicht angefragt werden."
        })?;
    let mut summen = Vec::new();
    let gelesen = lese_bis(
        |rest| zeilen.next_timeout(rest),
        |zeile| match zeile {
            AccountSummaryResult::End => Ok(true),
            AccountSummaryResult::Summary(a) => {
                summen.push(a);
                Ok(false)
            }
        },
        diagnose_aktiv,
    );
    zeilen.cancel();
    if diagnose_aktiv {
        diagnose::summen(&summen, konto);
    }
    gelesen?;
    salden(&summen, konto)
}

fn positionen_lesen(
    client: &Client,
    konto: &str,
    diagnose_aktiv: bool,
) -> Result<BTreeMap<i32, u64>, &'static str> {
    let meldungen = client.positions().map_err(|e| {
        diagnose::fehler(diagnose_aktiv, &e);
        "Positionen konnten nicht angefragt werden."
    })?;
    let mut positionen = BTreeMap::new();
    lese_bis(
        |rest| meldungen.next_timeout(rest),
        |meldung| match meldung {
            PositionUpdate::PositionEnd => Ok(true),
            PositionUpdate::Position(p) => {
                if p.account != konto
                    || p.contract.currency.0 != "EUR"
                    || p.contract.contract_id <= 0
                {
                    return Err("Position ohne eindeutige Zuordnung zum EUR-Papierkonto.");
                }
                let anzahl = stueck(p.position)?;
                if anzahl != 0 && positionen.insert(p.contract.contract_id, anzahl).is_some() {
                    return Err("Position für einen Kontrakt mehrfach gemeldet.");
                }
                Ok(false)
            }
        },
        diagnose_aktiv,
    )?;
    meldungen.cancel();
    Ok(positionen)
}

fn orders_lesen(
    client: &Client,
    konto: &str,
    diagnose_aktiv: bool,
) -> Result<Vec<OffeneOrder>, &'static str> {
    let mut orders = Vec::new();
    let mut ids = BTreeSet::new();
    for meldung in client
        .all_open_orders()
        .and_then(|sub| sub.collect_to_end_within(FRIST))
        .map_err(|e| {
            diagnose::fehler(diagnose_aktiv, &e);
            "Offene Orders wurden nicht vollständig gelesen."
        })?
    {
        let Orders::OrderData(o) = meldung else {
            continue;
        };
        if o.order.account != konto
            || o.contract.currency.0 != "EUR"
            || o.contract.contract_id <= 0
            || o.order.perm_id <= 0
            || !ids.insert(o.order.perm_id)
        {
            return Err("Offene Order ohne eindeutige Zuordnung zum EUR-Papierkonto.");
        }
        let richtung = match o.order.action {
            Action::Buy => "Kauf",
            Action::Sell => "Verkauf",
            _ => return Err("Gateway meldet eine unzulässige Orderrichtung."),
        };
        orders.push(OffeneOrder {
            dauerhafte_id: o.order.perm_id,
            referenz: o.order.order_ref,
            kontrakt: o.contract.contract_id,
            richtung,
            stueck: stueck(o.order.total_quantity)?,
        });
    }
    orders.sort_unstable();
    Ok(orders)
}

fn auszug(client: &Client, konto: &str, diagnose_aktiv: bool) -> Result<Auszug, &'static str> {
    konten_pruefen(client, konto, diagnose_aktiv)?;
    let salden = summen_lesen(client, konto, diagnose_aktiv);
    if !diagnose_aktiv && let Err(e) = salden {
        return Err(e);
    }
    let positionen = positionen_lesen(client, konto, diagnose_aktiv);
    if !diagnose_aktiv && let Err(e) = &positionen {
        return Err(e);
    }
    let orders = orders_lesen(client, konto, diagnose_aktiv);
    if diagnose_aktiv {
        diagnose::zeile(&format!(
            "Positionen: {} | Offene Orders: {}",
            positionen
                .as_ref()
                .map_or_else(|_| "unbekannt".into(), |p| p.len().to_string()),
            orders
                .as_ref()
                .map_or_else(|_| "unbekannt".into(), |o| o.len().to_string()),
        ));
        diagnose::zeile(&format!(
            "Positionen passen zum Journalstart: {} | Offene Orders passen zum Journalstart: {}",
            positionen
                .as_ref()
                .map_or("unbekannt", |p| diagnose::passt_zum_start(p.is_empty())),
            orders
                .as_ref()
                .map_or("unbekannt", |o| diagnose::passt_zum_start(o.is_empty())),
        ));
    }
    Ok(Auszug {
        salden: salden?,
        positionen: positionen?,
        orders: orders?,
    })
}

fn kontonummer(wurzel: &Path) -> Result<String, &'static str> {
    let konto = std::fs::read_to_string(wurzel.join("daten").join(KONTO_DATEI))
        .map_err(|_| KONTO_DATEI_FEHLT)?;
    let konto = konto.trim();
    if !kontonummer_gueltig(konto) {
        return Err(KONTO_FORMAT);
    }
    Ok(konto.to_owned())
}

fn hole(konto: &str) -> Result<Auszug, &'static str> {
    hole_mit_diagnose(konto, false)
}

fn hole_mit_diagnose(konto: &str, diagnose_aktiv: bool) -> Result<Auszug, &'static str> {
    let client = Client::connect(ADRESSE, CLIENT_ID).map_err(|e| {
        diagnose::fehler(diagnose_aktiv, &e);
        GATEWAY_NICHT_BEREIT
    })?;
    auszug(&client, konto, diagnose_aktiv)
}

/// Meldet das Ergebnis des lesenden Tests an die Oberfläche, ohne Kontostand oder Kontonummer auszugeben.
pub fn pruefe_papier(wurzel: &Path) -> ExitCode {
    let ergebnis = kontonummer(wurzel).and_then(|konto| hole_mit_diagnose(&konto, true));
    let text = match &ergebnis {
        Ok(a) => {
            let start = depot_kern::messlatte::ibkr::LANGFRIST.startkapital;
            let vorhanden = |wert: Option<Cent>| {
                if wert.is_some() {
                    "geliefert"
                } else {
                    "nicht geliefert"
                }
            };
            diagnose::zeile(&format!(
                "Abgerechnetes Bargeld: {} | Verfügbare Mittel: {}",
                vorhanden(a.salden.abgerechnet),
                vorhanden(a.salden.verfuegbar),
            ));
            if a.salden.gesamt == start && a.positionen.is_empty() && a.orders.is_empty() {
                format!(
                    "Papierkonto lesbar. Gesamtbargeld, Positionen und offene Orders passen zum vorgesehenen Journalstart mit {start}."
                )
            } else {
                format!(
                    "Papierkonto lesbar. Der Bestand weicht vom vorgesehenen Journalstart mit {start} ab. Prüfe unten die Gateway-Diagnose und im Paper Client Portal den verfügbaren Reset-Betrag. Teste erneut, sobald IBKR den passenden Bestand zeigt."
                )
            }
        }
        Err(grund) => {
            let weiter = match *grund {
                KONTO_DATEI_FEHLT | KONTO_FORMAT | KONTO_NICHT_PASSEND => {
                    "Vergleiche die Nummer unter „Paper Trading Account“ im Client Portal und speichere sie hier erneut."
                }
                GATEWAY_NICHT_BEREIT => {
                    "Öffne das lokale IB Gateway mit „IB API“ und „Paper-Trading“. Prüfe Port 4002 und warte auf „verbunden“."
                }
                KONTOLISTE_FEHLT
                | KONTOLISTE_TIMEOUT
                | KONTOLISTE_VERBINDUNG
                | KONTOLISTE_FORMAT => {
                    "Beende das Gateway vollständig, melde dich erneut im Paper-Trading-Modus an und starte den Test, sobald der API-Server „verbunden“ meldet."
                }
                EUR_FEHLT => {
                    "Prüfe im Client Portal im Paper-Modus, ob das Papierkonto Bargeld in EUR führt."
                }
                _ => {
                    "Öffne die Gateway-Diagnose unter dem Ergebnis. Die Zeilen zeigen, welche Angaben IBKR geliefert hat."
                }
            };
            format!("{grund} {weiter}")
        }
    };
    println!("{}", depot_kern::hinweis::zeile(&text));
    if ergebnis.is_ok() {
        ExitCode::SUCCESS
    } else {
        ExitCode::FAILURE
    }
}

pub fn zeige_papier(wurzel: &Path) -> ExitCode {
    let a = match kontonummer(wurzel).and_then(|konto| hole(&konto)) {
        Ok(a) => a,
        Err(e) => {
            eprintln!("[IBKR-Papier] {e}");
            return ExitCode::FAILURE;
        }
    };
    println!(
        "[IBKR-Papier] Bargeld {} Cent, abgerechnet {}, verfügbar {}.",
        a.salden.gesamt.0,
        saldo_text(a.salden.abgerechnet),
        saldo_text(a.salden.verfuegbar)
    );
    for (kontrakt, menge) in a.positionen {
        println!("Position {kontrakt}: {menge} Stück");
    }
    for o in a.orders {
        println!(
            "Offene Order {}: {}, ursprüngliche Menge {} Stück, Kontrakt {}",
            o.dauerhafte_id, o.richtung, o.stueck, o.kontrakt
        );
    }
    ExitCode::SUCCESS
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn geld_und_stueck_sind_exakt() {
        assert_eq!(eurocent("123.4500"), Some(Cent(12_345)));
        assert_eq!(eurocent("-1.2"), Some(Cent(-120)));
        assert_eq!(eurocent("0.001"), None);
        assert_eq!(eurocent("1e3"), None);
        assert_eq!(stueck(12.0), Ok(12));
        assert!(stueck(1.25).is_err());
    }

    #[test]
    fn unvollstaendiger_auszug_wird_verworfen() {
        let mut daten = [Ok(SubscriptionItem::Data(17_u8))].into_iter();
        assert!(lese_bis(|_| daten.next(), |_| Ok(false), false).is_err());
    }

    #[test]
    fn fremde_waehrungen_und_falsches_konto_werden_abgelehnt() {
        let mut zeilen = vec![AccountSummary {
            account: "DU1234".into(),
            currency: "EUR".into(),
            tag: "CashBalance".into(),
            value: "1000".into(),
        }];
        zeilen.extend(
            [
                AccountSummaryTags::TOTAL_CASH_VALUE,
                AccountSummaryTags::SETTLED_CASH,
                AccountSummaryTags::AVAILABLE_FUNDS,
            ]
            .map(|tag| AccountSummary {
                account: "DU1234".into(),
                currency: "EUR".into(),
                tag: tag.into(),
                value: "1000".into(),
            }),
        );
        assert!(salden(&zeilen, "DU1234").is_ok());
        assert!(salden(&zeilen, "DU1235").is_err());
        let ohne_abgerechnetes_bargeld = zeilen
            .iter()
            .filter(|zeile| zeile.tag != AccountSummaryTags::SETTLED_CASH)
            .cloned()
            .collect::<Vec<_>>();
        assert_eq!(
            salden(&ohne_abgerechnetes_bargeld, "DU1234"),
            Ok(Salden {
                gesamt: Cent(100_000),
                abgerechnet: None,
                verfuegbar: Some(Cent(100_000)),
            }),
        );
        let nur_gesamt = zeilen
            .iter()
            .filter(|zeile| {
                matches!(
                    zeile.tag.as_str(),
                    AccountSummaryTags::TOTAL_CASH_VALUE | "CashBalance"
                )
            })
            .cloned()
            .collect::<Vec<_>>();
        assert_eq!(
            salden(&nur_gesamt, "DU1234"),
            Ok(Salden {
                gesamt: Cent(100_000),
                abgerechnet: None,
                verfuegbar: None,
            })
        );
        let ohne_gesamt = zeilen
            .iter()
            .filter(|zeile| zeile.tag != AccountSummaryTags::TOTAL_CASH_VALUE)
            .cloned()
            .collect::<Vec<_>>();
        assert!(salden(&ohne_gesamt, "DU1234").is_err());
        zeilen.push(AccountSummary {
            account: "DU1234".into(),
            currency: "USD".into(),
            tag: "CashBalance".into(),
            value: "0.01".into(),
        });
        assert!(salden(&zeilen, "DU1234").is_err());
    }

    #[test]
    fn all_ist_nur_fuer_waehrungszeilen_zulaessig() {
        let mut zeilen = [
            AccountSummaryTags::TOTAL_CASH_VALUE,
            AccountSummaryTags::SETTLED_CASH,
            AccountSummaryTags::AVAILABLE_FUNDS,
        ]
        .map(|tag| AccountSummary {
            account: "DUR1234".into(),
            currency: "EUR".into(),
            tag: tag.into(),
            value: "1000".into(),
        })
        .to_vec();
        zeilen.push(AccountSummary {
            account: "All".into(),
            currency: "EUR".into(),
            tag: "$LEDGER-CashBalance".into(),
            value: "1000".into(),
        });
        assert_eq!(
            salden(&zeilen, "DUR1234"),
            Ok(Salden {
                gesamt: Cent(100_000),
                abgerechnet: Some(Cent(100_000)),
                verfuegbar: Some(Cent(100_000)),
            }),
        );

        zeilen.push(AccountSummary {
            account: "All".into(),
            currency: "USD".into(),
            tag: "$LEDGER-CashBalance".into(),
            value: "0.01".into(),
        });
        assert_eq!(
            salden(&zeilen, "DUR1234"),
            Err("Gateway meldet einen Fremdwährungssaldo."),
        );
        let _ = zeilen.pop();

        for (account, tag) in [
            ("All", AccountSummaryTags::SETTLED_CASH),
            ("DUR1235", "CashBalance"),
        ] {
            zeilen.push(AccountSummary {
                account: account.into(),
                currency: "EUR".into(),
                tag: tag.into(),
                value: "1000".into(),
            });
            assert_eq!(
                salden(&zeilen, "DUR1234"),
                Err("Gateway meldet Daten eines anderen Kontos."),
            );
            let _ = zeilen.pop();
        }
    }

    #[test]
    fn nur_exakt_konfigurierte_papierkontonummern() {
        assert!(kontonummer_gueltig("DU1234"));
        assert!(kontonummer_gueltig("DUR1234"));
        assert!(!kontonummer_gueltig("U1234"));
        assert!(!kontonummer_gueltig("DU"));
        assert!(!kontonummer_gueltig("DUR"));
        assert!(!kontonummer_gueltig("DURR1234"));
        assert!(!kontonummer_gueltig("DUR123456789012345678901"));
        assert!(!kontonummer_gueltig("DU123\nU456"));
    }
}
