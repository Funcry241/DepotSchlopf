//! Der Abdruck der Vorgeschichte (K15-01).
//!
//! Jeder Tagesabschluss traegt den Abdruck des Szenarios bis zu diesem Tag: Positivliste,
//! Startstand, Vorlauf und jeder Tag mit Datum, allen Kursen und allen Ereignissen. Der Abdruck
//! ist eine Kette: Der Abdruck eines Tages haengt am Abdruck des Vortags. Damit legt er auch die
//! Zahl der Tage fest und damit die Zyklusnummern und die Vergleichskurse.
//!
//! Ein Wiederanlauf setzt nur fort, wenn das uebergebene Szenario bis zum letzten Abschluss
//! denselben Abdruck ergibt. Nur die Tage danach duerfen anders sein.
//!
//! Jedes Feld geht mit seiner Laenge ein; kein Text kann so eine Feldgrenze vortaeuschen.

use std::fmt::Write as _;

use depot_kern::{Datum, Kurse, Topf};
use sha2::{Digest, Sha256};

use crate::szenario::{Szenario, Tag};

const FASSUNG: &str = "depot_sim-abdruck-v1";

fn feld(h: &mut Sha256, text: &str) {
    h.update(text.len().to_string().as_bytes());
    h.update(b":");
    h.update(text.as_bytes());
    h.update(b";");
}

fn datum(d: Datum) -> String {
    format!("{:04}-{:02}-{:02}", d.jahr(), d.monat(), d.tag())
}

fn kurse(h: &mut Sha256, k: &Kurse) {
    feld(h, &k.stand().sekunden().to_string());
    for (isin, kurs) in k.alle() {
        feld(h, isin.als_str());
        feld(h, &kurs.preis().0.to_string());
        feld(h, &format!("{:?}", kurs.waehrung()));
        feld(h, &datum(kurs.handelstag()));
        feld(h, &kurs.zeitpunkt().sekunden().to_string());
        feld(h, &format!("{:?}", kurs.herkunft()));
    }
    feld(h, "ende-kurse");
}

fn topf(h: &mut Sha256, t: &Topf) {
    feld(h, &format!("{:?}", t.id()));
    feld(h, &t.bargeld().0.to_string());
    for (isin, stueck) in t.positionen() {
        feld(h, isin.als_str());
        feld(h, &stueck.to_string());
    }
    feld(h, "ende-topf");
}

fn hex(bytes: &[u8]) -> String {
    bytes.iter().fold(String::new(), |mut s, b| {
        let _ = write!(s, "{b:02x}");
        s
    })
}

/// Abdruck vor dem ersten Tag: Positivliste, Startstand und Vorlauf.
fn anfang(s: &Szenario) -> String {
    let mut h = Sha256::new();
    feld(&mut h, FASSUNG);
    for i in s.universum.instrumente() {
        feld(&mut h, i.isin.als_str());
        feld(&mut h, &i.name);
        feld(&mut h, &format!("{:?}", i.art));
    }
    feld(&mut h, "ende-universum");
    for t in &s.start {
        topf(&mut h, t);
    }
    feld(&mut h, "ende-start");
    kurse(&mut h, &s.vorlauf);
    hex(&h.finalize())
}

/// Abdruck nach einem Tag, an den Abdruck davor gekettet.
fn weiter(vorher: &str, tag: &Tag) -> String {
    let mut h = Sha256::new();
    feld(&mut h, vorher);
    feld(&mut h, &datum(tag.datum));
    kurse(&mut h, &tag.kurse);
    for e in &tag.ereignisse {
        feld(&mut h, e);
    }
    feld(&mut h, "ende-tag");
    hex(&h.finalize())
}

/// Die Abdruecke nach jedem Tag des Szenarios, in Reihenfolge.
pub(crate) fn je_tag(s: &Szenario) -> Vec<String> {
    let mut vorher = anfang(s);
    s.tage
        .iter()
        .map(|t| {
            vorher = weiter(&vorher, t);
            vorher.clone()
        })
        .collect()
}
