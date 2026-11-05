//! Das Anbieterprotokoll `daten/anbieter.txt`: je Aufruf einer Quelle oder KI eine Zeile, was
//! dabei herauskam. Es ist kein Teil der Hash-Kette und entscheidet nichts: Der Kostenschutz
//! rechnet weiter nur mit den Reservierungen im Kassenbuch. Das Protokoll dient der Anzeige
//! (Schluesselstatus, Anbieterlimit, tatsaechliche Kosten).
//!
//! Zeile: `anbieter-v1;<sekunden>;<wer>;<ergebnis>;<http>;<ist>;<reserviert>;<warten_bis>`.
//! Felder ohne Wert stehen als `-`, unbekannte Kosten als `?`. Kein Feld enthaelt Text von
//! aussen; ein Schluessel kann darin nicht stehen.

use std::fs::OpenOptions;
use std::io::Write as _;
use std::path::Path;

use depot_kern::{Cent, Zeitpunkt};

/// Dateiname im Ordner `daten/`.
pub const DATEI: &str = "anbieter.txt";
const KOPF: &str = "anbieter-v1";

/// Wessen Aufruf.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum Wer {
    Kursquelle,
    Dachs,
    Maus,
}

impl Wer {
    pub fn kurz(self) -> &'static str {
        match self {
            Wer::Kursquelle => "kursquelle",
            Wer::Dachs => "dachs",
            Wer::Maus => "maus",
        }
    }

    pub fn aus_kurz(k: &str) -> Option<Wer> {
        [Wer::Kursquelle, Wer::Dachs, Wer::Maus]
            .into_iter()
            .find(|w| w.kurz() == k)
    }
}

/// Was ein Aufruf ergab.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Ergebnis {
    /// Beantwortet: Der Schluessel gilt.
    Ok,
    /// Der Anbieter begrenzt gerade die Aufrufe (etwa HTTP 429 oder das Tageskontingent).
    Limit,
    /// Beim Anbieter fehlt Guthaben oder ein Kontingent.
    Guthaben,
    /// Der Anbieter lehnt den Schluessel ab (HTTP 401 oder 403, ungueltiger Schluessel).
    Abgelehnt,
    /// Der Anbieter ist ueberlastet oder gestoert (HTTP 5xx, 529).
    Ueberlastet,
    /// Eine andere Ablehnung.
    Fehler,
    /// Nicht gesendet (keine Verbindung); kostet nichts.
    Netz,
    /// Antwort fehlt oder ist unlesbar; Kosten unbekannt, die KIs sind gesperrt.
    Unklar,
    /// Kosten oder Verbrauch ueber dem Erlaubten; die KIs sind gesperrt.
    Kosten,
    /// Ein neuer Schluessel wurde hinterlegt; noch nicht benutzt.
    Neu,
}

const ERGEBNISSE: [(Ergebnis, &str); 10] = [
    (Ergebnis::Ok, "ok"),
    (Ergebnis::Limit, "limit"),
    (Ergebnis::Guthaben, "guthaben"),
    (Ergebnis::Abgelehnt, "abgelehnt"),
    (Ergebnis::Ueberlastet, "ueberlastet"),
    (Ergebnis::Fehler, "fehler"),
    (Ergebnis::Netz, "netz"),
    (Ergebnis::Unklar, "unklar"),
    (Ergebnis::Kosten, "kosten"),
    (Ergebnis::Neu, "neu"),
];

impl Ergebnis {
    pub fn kurz(self) -> &'static str {
        ERGEBNISSE
            .iter()
            .find(|(e, _)| *e == self)
            .map_or("fehler", |(_, k)| k)
    }

    pub fn aus_kurz(k: &str) -> Option<Ergebnis> {
        ERGEBNISSE.iter().find(|(_, t)| *t == k).map(|(e, _)| *e)
    }

    /// Hat der Anbieter den Schluessel angenommen? Auch ein Limit oder fehlendes Guthaben
    /// meldet er erst nach der Anmeldung.
    pub fn schluessel_angenommen(self) -> bool {
        matches!(
            self,
            Ergebnis::Ok | Ergebnis::Limit | Ergebnis::Guthaben | Ergebnis::Kosten
        )
    }
}

/// Ordnet eine Ablehnung des Anbieters ein: Status und der Anfang der Antwort.
pub fn einordnen(status: u16, inhalt: &str) -> Ergebnis {
    let klein = inhalt.to_ascii_lowercase();
    if klein.contains("insufficient_quota")
        || klein.contains("credit balance")
        || klein.contains("billing")
        || status == 402
    {
        return Ergebnis::Guthaben;
    }
    match status {
        200 => Ergebnis::Ok,
        401 | 403 => Ergebnis::Abgelehnt,
        429 => Ergebnis::Limit,
        500..=599 => Ergebnis::Ueberlastet,
        _ => Ergebnis::Fehler,
    }
}

/// Was ein Aufruf tatsaechlich gekostet hat.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Kosten {
    /// Keine Abrechnung (Ablehnung, nicht gesendet, Kursquelle).
    Keine,
    /// Unbekannt: Antwort fehlt oder ist unlesbar.
    Unbekannt,
    Betrag(Cent),
}

/// Eine Zeile des Protokolls.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Eintrag {
    pub zeit: Zeitpunkt,
    pub wer: Wer,
    pub ergebnis: Ergebnis,
    pub http: Option<u16>,
    /// Tatsaechliche Kosten nach dem Verbrauch, den der Anbieter meldet.
    pub ist: Kosten,
    pub reserviert: Option<Cent>,
    /// Frueheste Zeit fuer den naechsten Versuch, wenn der Anbieter sie nennt.
    pub warten_bis: Option<Zeitpunkt>,
}

impl Eintrag {
    /// Ein Eintrag ohne Kosten und ohne Wartezeit.
    pub fn neu(zeit: Zeitpunkt, wer: Wer, ergebnis: Ergebnis, http: Option<u16>) -> Eintrag {
        Eintrag {
            zeit,
            wer,
            ergebnis,
            http,
            ist: Kosten::Keine,
            reserviert: None,
            warten_bis: None,
        }
    }

    pub fn als_zeile(&self) -> String {
        let feld = |o: Option<String>| o.unwrap_or_else(|| "-".into());
        let ist = match self.ist {
            Kosten::Keine => "-".into(),
            Kosten::Unbekannt => "?".into(),
            Kosten::Betrag(c) => c.0.to_string(),
        };
        format!(
            "{KOPF};{};{};{};{};{ist};{};{}",
            self.zeit.sekunden(),
            self.wer.kurz(),
            self.ergebnis.kurz(),
            feld(self.http.map(|h| h.to_string())),
            feld(self.reserviert.map(|c| c.0.to_string())),
            feld(self.warten_bis.map(|z| z.sekunden().to_string())),
        )
    }

    /// Liest genau die eigene Form.
    pub fn lies(zeile: &str) -> Option<Eintrag> {
        let f: Vec<&str> = zeile.split(';').collect();
        let [kopf, zeit, wer, ergebnis, http, ist, reserviert, warten] = f.as_slice() else {
            return None;
        };
        if *kopf != KOPF {
            return None;
        }
        let zahl = |t: &str| -> Option<Option<i64>> {
            if t == "-" {
                Some(None)
            } else if t.bytes().all(|b| b.is_ascii_digit()) && !t.is_empty() && t.len() <= 18 {
                t.parse().ok().map(Some)
            } else {
                None
            }
        };
        let zeitpunkt = |t: &str| -> Option<Option<Zeitpunkt>> {
            match zahl(t)? {
                None => Some(None),
                Some(s) => Zeitpunkt::aus_sekunden(s).map(Some),
            }
        };
        let e = Eintrag {
            zeit: zeitpunkt(zeit)??,
            wer: Wer::aus_kurz(wer)?,
            ergebnis: Ergebnis::aus_kurz(ergebnis)?,
            http: match zahl(http)? {
                None => None,
                Some(h) => Some(u16::try_from(h).ok()?),
            },
            ist: if *ist == "?" {
                Kosten::Unbekannt
            } else {
                zahl(ist)?.map_or(Kosten::Keine, |c| Kosten::Betrag(Cent(c)))
            },
            reserviert: zahl(reserviert)?.map(Cent),
            warten_bis: zeitpunkt(warten)?,
        };
        (e.als_zeile() == zeile).then_some(e)
    }
}

/// Liest das ganze Protokoll: die lesbaren Eintraege und die Zahl unlesbarer Zeilen.
pub fn lies_alle(text: &str) -> (Vec<Eintrag>, usize) {
    let mut gut = Vec::new();
    let mut schlecht = 0_usize;
    for z in text.lines().filter(|z| !z.is_empty()) {
        match Eintrag::lies(z) {
            Some(e) => gut.push(e),
            None => schlecht = schlecht.saturating_add(1),
        }
    }
    (gut, schlecht)
}

/// Haengt einen Eintrag an `pfad` an und schreibt ihn auf den Datentraeger.
///
/// # Errors
/// Die Datei laesst sich nicht oeffnen oder schreiben.
pub fn anhaengen(pfad: &Path, e: &Eintrag) -> Result<(), String> {
    if let Some(o) = pfad.parent() {
        std::fs::create_dir_all(o).map_err(|err| format!("{}: {err}", o.display()))?;
    }
    OpenOptions::new()
        .create(true)
        .append(true)
        .open(pfad)
        .and_then(|mut f| {
            writeln!(f, "{}", e.als_zeile())?;
            f.sync_all()
        })
        .map_err(|err| format!("{}: {err}", pfad.display()))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn z(s: i64) -> Option<Zeitpunkt> {
        Zeitpunkt::aus_sekunden(s)
    }

    #[test]
    fn zeile_hin_und_zurueck() {
        let Some(t) = z(1_793_900_000) else {
            return;
        };
        let e = Eintrag {
            zeit: t,
            wer: Wer::Dachs,
            ergebnis: Ergebnis::Ok,
            http: Some(200),
            ist: Kosten::Betrag(Cent(7)),
            reserviert: Some(Cent(43)),
            warten_bis: None,
        };
        assert_eq!(e.als_zeile(), "anbieter-v1;1793900000;dachs;ok;200;7;43;-");
        assert_eq!(Eintrag::lies(&e.als_zeile()), Some(e));
        let u = Eintrag {
            ist: Kosten::Unbekannt,
            warten_bis: z(1_793_900_060),
            ergebnis: Ergebnis::Limit,
            ..e
        };
        assert_eq!(Eintrag::lies(&u.als_zeile()), Some(u));
        for falsch in [
            "",
            "anbieter-v2;1;dachs;ok;-;-;-;-",
            "anbieter-v1;1;fuchs;ok;-;-;-;-",
            "anbieter-v1;1;dachs;gut;-;-;-;-",
            "anbieter-v1;1;dachs;ok;-;-;-",
            "anbieter-v1;-1;dachs;ok;-;-;-;-",
            "anbieter-v1;01;dachs;ok;-;-;-;-",
            "anbieter-v1;1;dachs;ok;70000;-;-;-",
            "anbieter-v1;1;dachs;ok;-;-;-;-;x",
        ] {
            assert_eq!(Eintrag::lies(falsch), None, "{falsch}");
        }
        let (gut, schlecht) =
            lies_alle(&format!("{}\nkaputt\n\n{}\n", e.als_zeile(), u.als_zeile()));
        assert_eq!((gut.len(), schlecht), (2, 1));
    }

    #[test]
    fn ablehnungen_werden_eingeordnet() {
        assert_eq!(einordnen(429, "rate_limit_error"), Ergebnis::Limit);
        assert_eq!(
            einordnen(429, "{\"error\":{\"code\":\"insufficient_quota\"}}"),
            Ergebnis::Guthaben
        );
        assert_eq!(
            einordnen(400, "Your credit balance is too low"),
            Ergebnis::Guthaben
        );
        assert_eq!(einordnen(401, "invalid x-api-key"), Ergebnis::Abgelehnt);
        assert_eq!(einordnen(403, ""), Ergebnis::Abgelehnt);
        assert_eq!(einordnen(529, "overloaded_error"), Ergebnis::Ueberlastet);
        assert_eq!(einordnen(500, ""), Ergebnis::Ueberlastet);
        assert_eq!(einordnen(400, "bad request"), Ergebnis::Fehler);
    }
}
