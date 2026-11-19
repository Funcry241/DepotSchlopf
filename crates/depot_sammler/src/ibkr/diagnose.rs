//! Teilt die Struktur der Broker-Antwort, ohne Beträge, Kontonummern oder Broker-Freitext auszugeben.

use std::collections::BTreeMap;

use ibapi::accounts::{AccountSummary, AccountSummaryTags};

pub(super) fn zeile(text: &str) {
    println!("[IBKR-Diagnose] {text}");
}

pub(super) fn fehler(aktiv: bool, fehler: &ibapi::Error) {
    if aktiv && let ibapi::Error::Notice(meldung) = fehler {
        zeile(&format!("Gateway-Code {}", meldung.code));
    }
}

fn tag(tag: &str) -> &str {
    if matches!(
        tag.strip_prefix("$LEDGER-").unwrap_or(tag),
        AccountSummaryTags::ACCOUNT_TYPE
            | AccountSummaryTags::TOTAL_CASH_VALUE
            | AccountSummaryTags::SETTLED_CASH
            | AccountSummaryTags::AVAILABLE_FUNDS
            | "CashBalance"
            | "TotalCashBalance"
            | "SettledCashByDate"
            | "Currency"
            | "RealCurrency"
    ) {
        tag
    } else {
        "sonstiger Tag"
    }
}

fn wertform(wert: &str) -> String {
    let (zeichen, zahl) = if let Some(zahl) = wert.strip_prefix('-') {
        ("minus", zahl)
    } else if let Some(zahl) = wert.strip_prefix('+') {
        ("plus", zahl)
    } else {
        ("ohne Vorzeichen", wert)
    };
    let (ganz, dezimal) = zahl.split_once('.').unwrap_or((zahl, ""));
    if wert.is_empty() {
        "leer".into()
    } else if !ganz.is_empty()
        && ganz
            .bytes()
            .chain(dezimal.bytes())
            .all(|b| b.is_ascii_digit())
    {
        format!("{zeichen}, {} Nachkommastellen", dezimal.len())
    } else {
        "Text oder anderes Format".into()
    }
}

fn form(a: &AccountSummary, konto: &str) -> String {
    let klasse = match a.account.as_str() {
        n if n == konto => "eigen",
        "All" => "All",
        "" => "leer",
        _ => "fremd",
    };
    let waehrung = match a.currency.as_str() {
        "" => "leer",
        "BASE" => "BASE",
        w if w.len() == 3 && w.bytes().all(|b| b.is_ascii_uppercase()) => w,
        _ => "unbekannt",
    };
    let name = tag(&a.tag);
    let wert = if name == AccountSummaryTags::ACCOUNT_TYPE
        && matches!(
            a.value.as_str(),
            "CASH" | "MARGIN" | "INDIVIDUAL" | "LLC" | "Cash" | "Margin"
        ) {
        a.value.clone()
    } else {
        wertform(&a.value)
    };
    format!("{name} | {waehrung} | {klasse} | {wert}")
}

pub(super) fn summen(zeilen: &[AccountSummary], konto: &str) {
    let mut formen = BTreeMap::new();
    let mut anzahl = BTreeMap::new();
    for a in zeilen {
        let n = formen.entry(form(a, konto)).or_insert(0_usize);
        *n = n.saturating_add(1);
        let n = anzahl.entry(tag(&a.tag)).or_insert(0_usize);
        *n = n.saturating_add(1);
    }
    for (form, n) in formen {
        zeile(&format!("{form} | {n}×"));
    }
    for (tag, n) in anzahl {
        zeile(&format!("Zeilen für {tag}: {n}"));
    }
    for tag in [
        AccountSummaryTags::TOTAL_CASH_VALUE,
        AccountSummaryTags::SETTLED_CASH,
        AccountSummaryTags::AVAILABLE_FUNDS,
        AccountSummaryTags::ACCOUNT_TYPE,
    ] {
        if zeilen.iter().all(|a| a.tag != tag) {
            zeile(&format!("Zeilen für {tag}: 0"));
        }
    }
    let mut gesamt = zeilen.iter().filter(|a| {
        a.account == konto && a.tag == AccountSummaryTags::TOTAL_CASH_VALUE && a.currency == "EUR"
    });
    let passt = match (
        gesamt.next().and_then(|a| super::eurocent(&a.value)),
        gesamt.next(),
    ) {
        (Some(gesamt), None) => {
            passt_zum_start(gesamt == depot_kern::messlatte::ibkr::LANGFRIST.startkapital)
        }
        _ => "unbekannt",
    };
    zeile(&format!("Gesamtbargeld passt zum Journalstart: {passt}"));
}

pub(super) fn passt_zum_start(passt: bool) -> &'static str {
    if passt { "ja" } else { "nein" }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn zeigt_nur_die_form_einer_broker_antwort() {
        let mut zeile = AccountSummary {
            account: "DUR8675309".into(),
            currency: "EUR".into(),
            tag: AccountSummaryTags::SETTLED_CASH.into(),
            value: "-191785.1234".into(),
        };
        assert_eq!(
            form(&zeile, "DUR1234"),
            "SettledCash | EUR | fremd | minus, 4 Nachkommastellen"
        );
        zeile.tag = "DUR8675309".into();
        zeile.currency = "191785".into();
        zeile.value = "DUR8675309 191785".into();
        assert_eq!(
            form(&zeile, "DUR1234"),
            "sonstiger Tag | unbekannt | fremd | Text oder anderes Format"
        );
    }
}
