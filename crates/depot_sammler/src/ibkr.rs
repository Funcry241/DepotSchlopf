//! Lesender Einzelabruf beim lokalen IB Gateway für das ausdrücklich angegebene Papierkonto.

pub(super) mod abgleich;

use std::collections::{BTreeMap, BTreeSet};
use std::path::Path;
use std::process::ExitCode;
use std::time::{Duration, Instant};

use depot_kern::Cent;
use ibapi::Client;
use ibapi::accounts::types::AccountGroup;
use ibapi::accounts::{AccountSummary, AccountSummaryResult, AccountSummaryTags, PositionUpdate};
use ibapi::orders::{Action, Orders};
use ibapi::subscriptions::SubscriptionItem;

const ADRESSE: &str = "127.0.0.1:4002";
const CLIENT_ID: i32 = 356;
const FRIST: Duration = Duration::from_secs(15);

#[derive(Debug, PartialEq, Eq)]
struct Salden {
    gesamt: Cent,
    abgerechnet: Cent,
    verfuegbar: Cent,
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

fn papierkonto(konto: &str) -> bool {
    konto
        .strip_prefix("DUR")
        .or_else(|| konto.strip_prefix("DU"))
        .is_some_and(|nr| {
            !nr.is_empty() && nr.len() <= 20 && nr.bytes().all(|b| b.is_ascii_digit())
        })
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
            Some(Ok(SubscriptionItem::Notice(_))) => (),
            Some(Err(_)) => return Err("Gateway meldet einen Fehler beim Lesen."),
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
        if a.tag == "CashBalance" {
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
        return Err("Der einzelne EUR-Saldo im Währungsledger fehlt.");
    }
    let wert = |tag| {
        summen
            .get(tag)
            .copied()
            .ok_or("Erforderliche Kontosumme fehlt.")
    };
    Ok(Salden {
        gesamt: wert(AccountSummaryTags::TOTAL_CASH_VALUE)?,
        abgerechnet: wert(AccountSummaryTags::SETTLED_CASH)?,
        verfuegbar: wert(AccountSummaryTags::AVAILABLE_FUNDS)?,
    })
}

fn stueck(menge: f64) -> Result<u64, &'static str> {
    menge
        .to_string()
        .parse()
        .map_err(|_| "Gateway meldet keine nichtnegative ganze Stückzahl.")
}

fn auszug(client: &Client, konto: &str) -> Result<Auszug, &'static str> {
    let konten = client
        .managed_accounts()
        .map_err(|_| "Konten nicht lesbar.")?;
    if !matches!(konten.as_slice(), [gemeldet] if gemeldet == konto) {
        return Err("Im Gateway muss genau das konfigurierte IBKR-Papierkonto angemeldet sein.");
    }
    let zeilen = client
        .account_summary(
            &AccountGroup("All".into()),
            &[
                AccountSummaryTags::TOTAL_CASH_VALUE,
                AccountSummaryTags::SETTLED_CASH,
                AccountSummaryTags::AVAILABLE_FUNDS,
                AccountSummaryTags::LEDGER_ALL,
            ],
        )
        .map_err(|_| "Kontosummen konnten nicht angefragt werden.")?;
    let mut summen = Vec::new();
    lese_bis(
        |rest| zeilen.next_timeout(rest),
        |zeile| match zeile {
            AccountSummaryResult::End => Ok(true),
            AccountSummaryResult::Summary(a) => {
                summen.push(a);
                Ok(false)
            }
        },
    )?;
    zeilen.cancel();
    let salden = salden(&summen, konto)?;

    let meldungen = client
        .positions()
        .map_err(|_| "Positionen konnten nicht angefragt werden.")?;
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
    )?;
    meldungen.cancel();

    let mut orders = Vec::new();
    let mut ids = BTreeSet::new();
    for meldung in client
        .all_open_orders()
        .and_then(|sub| sub.collect_to_end_within(FRIST))
        .map_err(|_| "Offene Orders wurden nicht vollständig gelesen.")?
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
    Ok(Auszug {
        salden,
        positionen,
        orders,
    })
}

fn kontonummer(wurzel: &Path) -> Result<String, &'static str> {
    let konto = std::fs::read_to_string(wurzel.join("daten").join("ibkr-papierkonto.txt"))
        .map_err(|_| "Lokale Datei daten/ibkr-papierkonto.txt fehlt oder ist unlesbar.")?;
    let konto = konto.trim();
    if !papierkonto(konto) {
        return Err(
            "In daten/ibkr-papierkonto.txt wird eine Papierkontonummer mit DU oder DUR und danach nur Ziffern erwartet.",
        );
    }
    Ok(konto.to_owned())
}

fn hole(konto: &str) -> Result<Auszug, &'static str> {
    let client = Client::connect(ADRESSE, CLIENT_ID)
        .map_err(|_| "Lokales IB Gateway auf Port 4002 nicht erreichbar oder nicht bereit.")?;
    auszug(&client, konto)
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
        "[IBKR-Papier] Bargeld {} Cent, abgerechnet {} Cent, verfügbar {} Cent.",
        a.salden.gesamt.0, a.salden.abgerechnet.0, a.salden.verfuegbar.0
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
        assert!(lese_bis(|_| daten.next(), |_| Ok(false)).is_err());
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
            tag: "CashBalance".into(),
            value: "1000".into(),
        });
        assert_eq!(
            salden(&zeilen, "DUR1234"),
            Ok(Salden {
                gesamt: Cent(100_000),
                abgerechnet: Cent(100_000),
                verfuegbar: Cent(100_000),
            }),
        );

        zeilen.push(AccountSummary {
            account: "All".into(),
            currency: "USD".into(),
            tag: "CashBalance".into(),
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
        assert!(papierkonto("DU1234"));
        assert!(papierkonto("DUR1234"));
        assert!(!papierkonto("U1234"));
        assert!(!papierkonto("DU"));
        assert!(!papierkonto("DUR"));
        assert!(!papierkonto("DURR1234"));
        assert!(!papierkonto("DUR123456789012345678901"));
        assert!(!papierkonto("DU123\nU456"));
    }
}
