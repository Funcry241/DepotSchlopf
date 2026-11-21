//! Lesende Ansicht des abgeschlossenen Neuro2026-Referenzlaufs.
use crate::http::antwort;
use crate::werkbank::Dienst;
use depot_kern::Datum;

// Der dokumentierte Lauf enthält ausschließlich synthetische Multiplikationsbeispiele.
const MESSUNG: &str = include_str!("../../../docs/neuroschlopf/lernlauf-v0.1.16.json");

pub(super) fn daten() -> Vec<u8> {
    antwort(
        200,
        "application/json; charset=utf-8",
        MESSUNG.replace("\r\n", "\n").as_bytes(),
    )
}

pub(super) fn seite(d: &Dienst, tag: Option<Datum>) -> Vec<u8> {
    let (schalter, einrichtung) = match crate::neuroschlopf::einstellung(&d.wurzel.join("daten")) {
        Ok(true) => ("Der lokale Belegvergleich ist eingeschaltet.", ""),
        Ok(false) => (
            "Der lokale Belegvergleich ist ausgeschaltet.",
            r#" <a href="/einrichtung#neuroschlopf">NeuroSchlopf einrichten</a>"#,
        ),
        Err(_) => (
            "Die Einstellung für den lokalen Belegvergleich ist unlesbar.",
            "",
        ),
    };
    let inhalt = format!(
        r#"<p><a href="/neuroschlopf">Zurück zu NeuroSchlopf</a></p>
<section><h2>Neuro2026 · Referenzlauf Multiplikation</h2>
<p>Das Netz hat aus Zahlenpaaren gelernt, ihr Produkt zu schätzen. Hier siehst du den abgeschlossenen Lauf vom 20.11.2026.</p>
<p>{schalter} Die Auswahl von Nachrichten und Terminen in DepotSchlopf verwendet bisher den Belegvergleich. Für dessen Bewertung durch Neuro2026 steht der Modellbetrieb noch aus.{einrichtung}</p></section>
<p><a href="/neuroschlopf/lernlauf.json" download="neuro2026-lernlauf.json">Originalmesswerte herunterladen (JSON)</a></p>
<p id="lernstand" role="status">Für die grafische Ansicht benötigt dein Browser JavaScript.</p>
<div id="lernansicht" data-messung="/neuroschlopf/lernlauf.json" hidden>
<section><h2>Netz und Training</h2><p id="lernnetz"></p>
<svg id="lernaufbau" viewBox="0 0 480 215" role="img" aria-labelledby="netz-titel" style="width:100%;max-width:32rem">
<title id="netz-titel">Vollständig verbundene Schichten des Referenzmodells</title></svg><p id="lernplan"></p></section>
<section><h2>Lernkurve</h2><p id="lernfehler"></p>
<figure><svg id="lernkurve" viewBox="0 0 600 210" role="img" aria-labelledby="kurve-titel kurve-text" style="width:100%;max-width:44rem">
<title id="kurve-titel">Trainingsfehler über die Epochen</title>
<desc id="kurve-text">Die Linie zeigt den mittleren quadratischen Batchfehler während jeder Epoche auf logarithmischer Skala. Alle Epochenwerte stehen in den Originalmesswerten.</desc></svg>
<figcaption>Eine Epoche umfasst alle Trainingspaare. Die Linie zeigt den Mittelwert der Batchfehler während der Gewichtsaktualisierung.</figcaption></figure></section>
<section><h2>Vorhersagen auf neuen Zahlenpaaren</h2><p id="lernvergleich"></p>
<label for="lernpunkt">Testpaar auswählen</label><select id="lernpunkt"></select><p id="lernantwort" aria-live="polite"></p>
<p id="lernladen" class="leise"></p></section></div>"#
    );
    let html = crate::seiten::rahmen(
        "/neuroschlopf/lernlauf",
        None,
        None,
        &inhalt,
        d.version.as_deref(),
        (&crate::betriebsleiste(d, tag), &crate::update_leiste(d)),
        ("", ""),
    );
    antwort(200, crate::HTML, html.as_bytes())
}
