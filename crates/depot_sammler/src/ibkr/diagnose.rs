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
            | AccountSummaryTags::BUYING_POWER
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
        && a.account == konto
        && (1..=20).contains(&a.value.len())
        && a.value.bytes().all(|b| b.is_ascii_uppercase())
    {
        a.value.clone()
    } else {
        wertform(&a.value)
    };
    format!("{name} | {waehrung} | {klasse} | {wert}")
}

fn eur_wert<'a>(
    zeilen: &'a [AccountSummary],
    konto: &str,
    tags: &[&str],
    waehrungszeile: bool,
) -> Option<&'a str> {
    let mut treffend = zeilen.iter().filter(|a| {
        a.currency == "EUR"
            && (a.account == konto || (waehrungszeile && a.account == "All"))
            && tags.contains(&a.tag.as_str())
    });
    let wert = treffend.next()?;
    treffend.next().is_none().then_some(wert.value.as_str())
}

fn dezimal_normalisiert(wert: &str) -> Option<(bool, &str, &str)> {
    let negativ = wert.starts_with('-');
    let zahl = wert.strip_prefix(['-', '+']).unwrap_or(wert);
    let (ganz, dezimal) = zahl.split_once('.').unwrap_or((zahl, ""));
    if ganz.is_empty()
        || !ganz
            .bytes()
            .chain(dezimal.bytes())
            .all(|b| b.is_ascii_digit())
    {
        return None;
    }
    let ganz = ganz.trim_start_matches('0');
    let dezimal = dezimal.trim_end_matches('0');
    Some((
        negativ && !(ganz.is_empty() && dezimal.is_empty()),
        ganz,
        dezimal,
    ))
}

fn werte_gleich(links: Option<&str>, rechts: Option<&str>) -> &'static str {
    match (
        links.and_then(dezimal_normalisiert),
        rechts.and_then(dezimal_normalisiert),
    ) {
        (Some(a), Some(b)) => passt_zum_start(a == b),
        _ => "unbekannt",
    }
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
        AccountSummaryTags::BUYING_POWER,
    ] {
        if zeilen.iter().all(|a| a.tag != tag) {
            zeile(&format!("Zeilen für {tag}: 0"));
        }
    }
    let kontowert = |tag| eur_wert(zeilen, konto, &[tag], false);
    let gesamt = kontowert(AccountSummaryTags::TOTAL_CASH_VALUE);
    let verfuegbar = kontowert(AccountSummaryTags::AVAILABLE_FUNDS);
    let ledger = eur_wert(zeilen, konto, &["CashBalance", "$LEDGER-CashBalance"], true);
    for (name, links, rechts) in [
        (
            "BuyingPower gleich AvailableFunds",
            kontowert(AccountSummaryTags::BUYING_POWER),
            verfuegbar,
        ),
        (
            "SettledCash gleich TotalCashValue",
            kontowert(AccountSummaryTags::SETTLED_CASH),
            gesamt,
        ),
        ("AvailableFunds gleich TotalCashValue", verfuegbar, gesamt),
        ("EUR-CashBalance gleich TotalCashValue", ledger, gesamt),
    ] {
        zeile(&format!("{name}: {}", werte_gleich(links, rechts)));
    }
    let passt = gesamt
        .and_then(super::eurocent)
        .map_or("unbekannt", |betrag| {
            passt_zum_start(betrag == depot_kern::messlatte::ibkr::LANGFRIST.startkapital)
        });
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
        zeile.account = "DUR1234".into();
        zeile.tag = AccountSummaryTags::ACCOUNT_TYPE.into();
        zeile.currency.clear();
        zeile.value = "PARTNERSHIP".into();
        assert_eq!(
            form(&zeile, "DUR1234"),
            "AccountType | leer | eigen | PARTNERSHIP"
        );
        zeile.value = "DUR8675309".into();
        assert_eq!(
            form(&zeile, "DUR1234"),
            "AccountType | leer | eigen | Text oder anderes Format"
        );
    }

    #[test]
    fn vergleicht_dezimalwerte_exakt_und_doppelte_werden_nicht_gewaehlt() {
        assert_eq!(werte_gleich(Some("+001.23400"), Some("1.234")), "ja");
        assert_eq!(werte_gleich(Some("-0.00"), Some("0")), "ja");
        assert_eq!(werte_gleich(Some("1.234"), Some("1.235")), "nein");
        assert_eq!(werte_gleich(Some("1E3"), Some("1000")), "unbekannt");
        assert_eq!(werte_gleich(None, Some("1000")), "unbekannt");
        let a = AccountSummary {
            account: "All".into(),
            currency: "EUR".into(),
            tag: "$LEDGER-CashBalance".into(),
            value: "1.234".into(),
        };
        let tags = ["CashBalance", "$LEDGER-CashBalance"];
        assert_eq!(
            eur_wert(std::slice::from_ref(&a), "DUR1234", &tags, true),
            Some("1.234")
        );
        assert_eq!(eur_wert(&[a.clone(), a], "DUR1234", &tags, true), None);
    }
}
