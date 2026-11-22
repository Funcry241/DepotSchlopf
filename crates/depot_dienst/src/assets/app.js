"use strict";
(function () {
  // Änderungen bleiben im Formular; insbesondere kein Schlüssel in Browser-Speichern.
  var gehtWeg = false;
  var andererStart = false;
  var bearbeitet = new WeakSet();
  var normalerSpeichertext = new WeakMap();
  var felder = "input:not([type=hidden]),select";
  function formulare() { return Array.from(document.querySelectorAll("form[data-aenderungen]")); }
  function auswahlGeaendert(select) {
    var optionen = Array.from(select.options);
    var standard = optionen.find(function (o) { return o.defaultSelected; }) || optionen[0];
    return !!standard && select.value !== standard.value;
  }
  function istGeaendert(f) {
    return f.dataset.entwurf === "ja" || Array.from(f.querySelectorAll(felder)).some(function (i) {
      if (!bearbeitet.has(i) || (i.type === "password" && i.matches(":autofill"))) { return false; }
      if (i.type === "select-one") { return auswahlGeaendert(i); }
      return i.type === "checkbox" ? i.checked !== i.defaultChecked : i.value !== i.defaultValue;
    });
  }
  function offen(ausser) { return formulare().some(function (f) { return f !== ausser && istGeaendert(f); }); }
  function textSetzen(element, text) {
    if (element.textContent !== text) { element.textContent = text; }
  }
  function kostenvorschau(f, neu) {
    var status = f.querySelector("[data-kostenstand]");
    textSetzen(status, neu ? "Kostenvorschau für ungespeicherte Änderungen." : "Kostenschätzung für gespeicherte Einstellungen.");
    var dachsNeu = auswahlGeaendert(f.elements["dachs-modell"]);
    var mausNeu = auswahlGeaendert(f.elements["maus-modell"]);
    f.querySelectorAll("[data-kostenlimit]").forEach(function (zeile) {
      var feld = zeile.dataset.kostenlimit;
      var input = f.elements[feld];
      var modus = feld === "neuro-aufrufe" ? f.elements["neuro-empfaenger"].value : "standard";
      var anderesProfil = (dachsNeu && (feld === "dachs-aufrufe" || modus === "dachs" || modus === "beide")) ||
        (mausNeu && (feld === "maus-aufrufe" || modus === "maus" || modus === "beide"));
      var basis = zeile.querySelector("[data-kostenbasis]");
      if (basis) { basis.hidden = anderesProfil; }
      var anzahl = Number(input.value);
      var werte = /^[0-9]+$/.test(input.value) && input.validity.valid && (!anderesProfil || anzahl === 0)
        ? JSON.parse(zeile.getAttribute("data-kosten-" + modus))[anzahl] : null;
      zeile.querySelectorAll("[data-kosten-zeit]").forEach(function (zelle, i) {
        zelle.hidden = !werte;
        textSetzen(zelle, werte ? werte[i] : "");
      });
      zeile.querySelector("[data-kosten-unbekannt]").hidden = !!werte;
    });
  }
  function zeige(f) {
    var neu = istGeaendert(f);
    f.classList.toggle("ungespeichert", neu);
    var status = f.querySelector("[data-speicherstand]");
    if (status) { textSetzen(status, neu ? "Noch nicht gespeichert. Bitte unten speichern, damit deine Änderungen gelten." : "Keine ungespeicherten Änderungen in diesem Formular."); }
    var knopf = f.querySelector("[data-speichern-hervorheben]");
    if (knopf) {
      if (!normalerSpeichertext.has(knopf)) { normalerSpeichertext.set(knopf, knopf.textContent); }
      knopf.classList.toggle("primaer", neu);
      textSetzen(knopf, neu ? "Änderungen speichern" : normalerSpeichertext.get(knopf));
    }
    if (f.dataset.kiEinstellungen !== undefined) { kostenvorschau(f, neu); }
  }
  ["input", "change"].forEach(function (ereignis) {
    document.addEventListener(ereignis, function (e) {
      var f = e.target.closest("form[data-aenderungen]");
      if (f) { bearbeitet.add(e.target); zeige(f); }
    });
  });
  document.addEventListener("invalid", function (e) {
    var f = e.target.closest("form[data-aenderungen]");
    var status = f && f.querySelector("[data-speicherstand]");
    if (status) { status.textContent = "Noch nicht gespeichert. Bitte prüfe das markierte Feld. Deine übrigen Eingaben bleiben stehen."; }
  }, true);
  window.addEventListener("beforeunload", function (e) {
    if (!gehtWeg && offen()) { e.preventDefault(); e.returnValue = ""; }
  });
  formulare().forEach(zeige);
  // Delegiert, da Live auch laufende Protokolle regelmäßig ersetzt.
  document.addEventListener("click", async function (e) {
    var b = e.target.closest("[data-protokoll-kopieren]");
    if (!b) { return; }
    var block = b.closest("details");
    var log = block.querySelector("pre");
    var status = block.querySelector("[role=status]");
    try {
      await navigator.clipboard.writeText(log.textContent);
      status.textContent = "Kopiert.";
    } catch (_) {
      var auswahl = window.getSelection();
      if (auswahl && log.isConnected) {
        var bereich = document.createRange();
        bereich.selectNodeContents(log);
        auswahl.removeAllRanges();
        auswahl.addRange(bereich);
        status.textContent = "Text markiert. Drücke Strg+C zum Kopieren.";
      } else {
        status.textContent = "Bitte markiere und kopiere den Text von Hand.";
      }
    }
  });
  async function estrImport(form, knopf) {
    var datei = form.querySelector('input[type="file"]').files[0];
    var status = form.querySelector('[role="status"]');
    if (!datei || knopf.disabled) { return; }
    if (datei.size > Number(form.dataset.estrImport)) {
      status.className = "offen";
      status.textContent = "Die CSV ist größer als 2 MiB.";
      return;
    }
    knopf.disabled = true;
    status.textContent = "CSV wird geprüft …";
    try {
      var r = await fetch(form.action, {
        method: "POST",
        headers: { "Content-Type": "text/csv", "X-DepotSchlopf-Token": form.elements.t.value },
        body: datei
      });
      status.className = r.ok ? "gut" : "offen";
      status.textContent = (r.headers.get("content-type") || "").startsWith("text/plain")
        ? await r.text() : "Import abgelehnt. Öffne die Einrichtung erneut.";
      if (r.ok) { form.reset(); }
    } catch (_) {
      status.className = "offen";
      status.textContent = "Der Dienst ist nicht erreichbar. Versuche den Import erneut.";
    } finally {
      knopf.disabled = false;
    }
  }
  async function kiSpeichern(form, knopf) {
    if (knopf.disabled) { return; }
    var status = form.querySelector("[data-speicherstand]");
    var daten = new URLSearchParams(new FormData(form));
    var eingaben = Array.from(form.querySelectorAll(felder)).filter(function (i) { return !i.disabled; });
    knopf.disabled = true;
    eingaben.forEach(function (i) { i.disabled = true; });
    textSetzen(status, "KI-Einstellungen werden gespeichert …");
    var fehler = "KI-Einstellungen konnten nicht gespeichert werden. Deine Eingaben bleiben stehen.";
    try {
      var antwort = await fetch(form.action, { method: "POST", body: daten });
      if (antwort.ok && antwort.redirected) {
        var ziel = new URL(antwort.url);
        ziel.hash = "ki-einstellungen";
        gehtWeg = true;
        window.location.replace(ziel.href);
        return;
      }
      if (antwort.status === 409) { fehler = "Das Update läuft. Speichere deine Änderungen danach erneut."; }
    } catch (_) {
      gehtWeg = false;
      fehler = "Der Dienst antwortet nicht. Deine Eingaben bleiben stehen.";
    }
    eingaben.forEach(function (i) { i.disabled = false; });
    knopf.disabled = false;
    textSetzen(status, fehler);
  }
  // Doppelklicks: Ein abgesendetes Formular sperrt seinen Knopf. Der Dienst lehnt doppelte
  // Aufträge ohnehin ab; so sieht man sofort, dass der Klick angekommen ist.
  document.addEventListener("submit", function (e) {
    if (andererStart) { e.preventDefault(); return; }
    if (e.target.dataset.estrImport !== undefined) {
      e.preventDefault();
      void estrImport(e.target, e.submitter || e.target.querySelector("button[type=submit]"));
      return;
    }
    if (offen(e.target) && !window.confirm("In einem anderen Formular stehen noch ungespeicherte Änderungen. Sie werden bei dieser Aktion verworfen. Trotzdem fortfahren?")) {
      e.preventDefault(); return;
    }
    // Ein Download lässt die Seite offen; der Exportknopf bleibt benutzbar.
    if (e.target.dataset.download !== undefined) { return; }
    var b = e.submitter || e.target.querySelector("button[type=submit]");
    if (e.target.dataset.kiEinstellungen !== undefined) {
      e.preventDefault();
      void kiSpeichern(e.target, b);
      return;
    }
    if (b) {
      if (b.disabled) { e.preventDefault(); return; }
      if (b.dataset.gesendet) { e.preventDefault(); return; }
      gehtWeg = true;
      b.dataset.gesendet = b.textContent;
      setTimeout(function () { b.disabled = true; b.textContent = "wird gesendet …"; }, 0);
    }
  }, true);
  // Eine Verbindungsprüfung gilt für jede Seite, auch Bestätigung und Protokoll.
  var banner = document.getElementById("getrennt");
  var herz = document.getElementById("herz");
  var ampel = "offen";
  var herzVerbindung = null;
  function zeigeHerz() {
    if (!herz) { return; }
    var zustand = herzVerbindung || ampel;
    var text = zustand === "gut" ? "Alles läuft" : zustand === "halt" ? "Eingriff nötig" : "Wartet";
    herz.setAttribute("data-zustand", zustand);
    herz.setAttribute("aria-label", text);
    herz.setAttribute("title", text);
  }
  function kopfaussagen() {
    var ziel = document.getElementById("kopffiguren");
    var daten = document.querySelector('#betriebsleiste [data-figuren]');
    if (!ziel || !daten) { return; }
    var blind = document.querySelector("[data-blindentscheidung]");
    var denkt = daten.getAttribute("data-denkt");
    var pose = blind ? "dran" : daten.getAttribute("data-otter");
    if (ziel.getAttribute("data-denkt") !== denkt) { ziel.setAttribute("data-denkt", denkt); }
    if (ziel.getAttribute("data-otter") !== pose) { ziel.setAttribute("data-otter", pose); }
    ["dachs", "maus", "otter"].forEach(function (wer) {
      var blase = document.getElementById("kopf-" + wer);
      if (!blase) { return; }
      var satz = daten.getAttribute("data-" + (wer === "otter" ? "otter-satz" : wer)) || "";
      if (wer === "otter" && blind) {
        satz = "Eine Blindbewertung wartet auf dein Urteil." +
          (daten.getAttribute("data-otter") === "dran" ? " " + satz : "");
      }
      if (blase.textContent !== satz) { blase.textContent = satz; }
      var titel = daten.getAttribute("data-" + wer + "-titel");
      if (titel) { blase.setAttribute("title", titel); }
    });
  }
  function merkeAmpel() {
    var marke = document.querySelector('#betriebsleiste [data-ampel]');
    var zustand = marke && marke.getAttribute("data-ampel");
    ampel = zustand === "gut" || zustand === "halt" ? zustand : "offen";
    zeigeHerz();
    kopfaussagen();
  }
  merkeAmpel();
  var marke = document.querySelector("[data-start]");
  var start = marke ? marke.getAttribute("data-start") : null;
  var neuladen = false;
  var updateErwartetSeit = new URL(window.location.href).searchParams.get("m") === "update" ? Date.now() : null;
  var updateWartetSeit = null;
  var updateFrist = 180000;
  var pausiert = false;
  var generation = 0;
  var geldVerbindung = 0;
  var timer = new Set();
  var anfragen = new Set();
  function planeAbruf(f, ms) {
    if (pausiert || neuladen) { return; }
    var id = setTimeout(function () { timer.delete(id); f(); }, ms);
    timer.add(id);
  }
  async function lade(url) {
    var steuerung = new AbortController();
    anfragen.add(steuerung);
    var frist = setTimeout(function () { steuerung.abort(); }, 5000);
    try {
      var r = await fetch(url, { cache: "no-store", credentials: "same-origin", signal: steuerung.signal });
      if (!r.ok) { throw new Error(String(r.status)); }
      return await r.text();
    } finally {
      clearTimeout(frist);
      anfragen.delete(steuerung);
    }
  }
  function kennung(t) {
    var m = /data-start="([^"]*)"/.exec(t);
    return m ? m[1] : null;
  }
  function wiederverbindungsziel() {
    // Die Position und fachliche Parameter bleiben erhalten, die alte Rückmeldung entfällt.
    var ziel = new URL(window.location.href);
    ziel.searchParams.delete("m");
    return ziel.pathname + ziel.search + ziel.hash;
  }
  function zeigeUpdateWartezeit() {
    var jetzt = Date.now();
    var wartet = jetzt - updateWartetSeit < updateFrist;
    herzVerbindung = wartet ? "offen" : "halt";
    zeigeHerz();
    if (wartet) {
      banner.setAttribute("data-update-wartet", "");
      banner.textContent = "Die neue Version startet gerade. Du musst nichts tun. Die Seite lädt sich von selbst neu.";
    } else {
      banner.removeAttribute("data-update-wartet");
      banner.textContent = "Die neue Version wurde nicht innerhalb von drei Minuten bestätigt. Prüfe das DepotSchlopf-Fenster auf eine Fehlermeldung. Wenn es geschlossen ist, starte DepotSchlopf.cmd erneut.";
    }
    banner.hidden = false;
  }
  async function verbindung() {
    var g = generation;
    var fehler = false;
    try {
      var t = await lade("/teil/status");
      if (g !== generation || pausiert) { return; }
      var neu = kennung(t);
      if (!neu) { throw new Error("Startkennung fehlt"); }
      // Ältere Rückfallstände kennen nur data-start; sie bleiben erreichbar.
      if (t.indexOf('data-update-laeuft="ja"') !== -1 && updateErwartetSeit === null) {
        updateErwartetSeit = Date.now();
      }
      var bereit = t.indexOf('data-bereit="nein"') === -1;
      if (!bereit) {
        if (updateWartetSeit === null) { updateWartetSeit = Date.now(); }
        zeigeUpdateWartezeit();
        return;
      }
      if (t.indexOf('data-update-laeuft="nein"') !== -1) {
        updateErwartetSeit = null;
      }
      if (herzVerbindung) { geldVerbindung += 1; }
      herzVerbindung = null;
      zeigeHerz();
      var updateBestaetigt = updateWartetSeit !== null;
      updateWartetSeit = null;
      banner.removeAttribute("data-update-wartet");
      if (andererStart || updateBestaetigt || (start && neu !== start)) {
        andererStart = true;
        var ziel = wiederverbindungsziel();
        if (offen()) {
          banner.textContent = "DepotSchlopf wurde neu gestartet. Deine ungespeicherten Eingaben stehen noch hier. Prüfe sie vor dem Neuladen. Zum Speichern braucht diese Seite die neue Verbindung.";
          banner.setAttribute("data-update-wartet", "");
          banner.hidden = false;
          document.getElementById("neuverbinden-link").href = ziel;
          document.getElementById("neuverbinden").hidden = false;
        } else if (!neuladen) {
          banner.hidden = true;
          // Derselbe Anker kann bei replace() eine Navigation im selben Dokument sein.
          if (ziel === window.location.pathname + window.location.search + window.location.hash) {
            window.location.reload();
          } else {
            window.location.replace(ziel);
          }
          neuladen = true;
        }
      } else {
        start = neu;
        banner.hidden = true;
      }
    } catch (_) {
      if (g !== generation || pausiert) { return; }
      fehler = true;
      if (updateWartetSeit === null && updateErwartetSeit !== null) {
        updateWartetSeit = updateErwartetSeit;
      }
      if (updateWartetSeit !== null) { zeigeUpdateWartezeit(); return; }
      banner.removeAttribute("data-update-wartet");
      herzVerbindung = "halt";
      zeigeHerz();
      banner.textContent = "Verbindung zu DepotSchlopf unterbrochen. Die Seite versucht es automatisch erneut. Lass das DepotSchlopf-Fenster offen.";
      banner.hidden = false;
    } finally {
      if (g === generation) { planeAbruf(verbindung, fehler ? 5000 : 2000); }
    }
  }
  function konsolePosition(d, vorher) {
    var pre = d.querySelector("pre.konsole");
    if (!pre || !d.open) { return; }
    pre.scrollTop = vorher == null || vorher.unten ? pre.scrollHeight : vorher.scrollTop;
  }
  document.querySelectorAll("details[open]").forEach(function (d) { konsolePosition(d, null); });
  document.addEventListener("toggle", function (e) {
    var d = e.target;
    if (!(d instanceof HTMLDetailsElement) || !d.open || d.dataset.konsoleGeoeffnet) { return; }
    d.dataset.konsoleGeoeffnet = "ja";
    konsolePosition(d, null);
  }, true);
  function geldwerte(ziel) {
    return ziel.querySelectorAll("[data-wert][data-schluessel]");
  }
  function geldwechsel(element, von, bis, bewegung) {
    var endtext = element.textContent;
    element.classList.add(bis > von ? "steigt" : "faellt");
    if (bewegung.matches) { return; }
    var format = new Intl.NumberFormat("de-DE", {style:"currency", currency:"EUR"});
    var beginn = performance.now();
    element.textContent = format.format(von / 100);
    function schritt(jetzt) {
      if (!element.isConnected) { return; }
      var anteil = (jetzt - beginn) / 600;
      if (anteil >= 1 || bewegung.matches) { element.textContent = endtext; return; }
      element.textContent = format.format(Math.round(von + (bis - von) * anteil) / 100);
      requestAnimationFrame(schritt);
    }
    requestAnimationFrame(schritt);
  }
  var teile = Array.from(document.querySelectorAll("[data-teil]")).map(function (ziel) {
    var url = ziel.getAttribute("data-teil");
    var depot = url.startsWith("/teil/depot?");
    var zuletzt = null;
    var geldstand = null;
    return async function hole() {
      var g = generation;
      var fehler = false;
      try {
        var abfrage = url;
        if (depot) {
          var stand = ziel.querySelector("[data-depot-stand]");
          if (stand) { abfrage += "&stand=" + encodeURIComponent(stand.dataset.depotStand); }
        }
        var t = await lade(abfrage);
        if (g !== generation || pausiert || neuladen) { return; }
        var neu = kennung(t);
        // Der zentrale Verbindungstest wartet auf die Bestätigung und lädt genau einmal neu.
        if (start && neu && neu !== start) { return; }
        var geldKennung = generation + "/" + geldVerbindung;
        var geldAnimation = geldstand === geldKennung && !herzVerbindung;
        geldstand = herzVerbindung ? null : geldKennung;
        if ((!depot || t) && t !== zuletzt) {
          var werte = new Map();
          if (geldAnimation) {
            geldwerte(ziel).forEach(function (e) {
              werte.set(e.dataset.schluessel, Number(e.dataset.wert));
            });
          }
          var protokolle = new Map();
          ziel.querySelectorAll("details[id]").forEach(function (d) {
            var pre = d.querySelector("pre.konsole");
            protokolle.set(d.id, {
              offen: d.open,
              geoeffnet: d.dataset.konsoleGeoeffnet,
              position: pre && d.open ? {
                unten: pre.scrollHeight - pre.scrollTop - pre.clientHeight < 20,
                scrollTop: pre.scrollTop
              } : null
            });
          });
          ziel.innerHTML = t;
          if (werte.size) {
            var bewegung = window.matchMedia("(prefers-reduced-motion: reduce)");
            geldwerte(ziel).forEach(function (e) {
              var von = werte.get(e.dataset.schluessel);
              var bis = Number(e.dataset.wert);
              if (Number.isSafeInteger(von) && Number.isSafeInteger(bis) && von !== bis) {
                geldwechsel(e, von, bis, bewegung);
              }
            });
          }
          if (url === "/teil/kopf") { merkeAmpel(); }
          zuletzt = t;
          ziel.querySelectorAll("details[id]").forEach(function (d) {
            var vorher = protokolle.get(d.id);
            if (vorher) { d.open = vorher.offen; }
            if (d.open || (vorher && vorher.geoeffnet)) { d.dataset.konsoleGeoeffnet = "ja"; }
            konsolePosition(d, vorher && vorher.position);
          });
        }
      } catch (_) { fehler = true; geldstand = null; }
      finally { if (g === generation) { planeAbruf(hole, fehler ? 5000 : 2000); } }
    };
  });
  function starte() {
    planeAbruf(verbindung, 0);
    teile.forEach(function (f) { planeAbruf(f, 2000); });
  }
  window.addEventListener("pagehide", function () {
    pausiert = true;
    generation += 1;
    timer.forEach(function (id) { clearTimeout(id); });
    timer.clear();
    anfragen.forEach(function (a) { a.abort(); });
  });
  window.addEventListener("pageshow", function (e) {
    gehtWeg = false;
    document.querySelectorAll("button[data-gesendet]").forEach(function (b) {
      b.textContent = b.dataset.gesendet;
      delete b.dataset.gesendet;
      b.disabled = false;
    });
    formulare().forEach(zeige);
    if (e.persisted) { pausiert = false; generation += 1; starte(); }
  });
  starte();
})();

// Die Referenzansicht zeigt den gemessenen Lauf des separaten Lernkerns.
(async function neuroLernlauf() {
  const bereich = document.querySelector("[data-messung]");
  if (!bereich?.dataset?.messung) { return; }
  const feld = (id) => document.getElementById(id);
  const zahl = (wert) => wert.toLocaleString("de-DE", { maximumSignificantDigits: 5 });
  const stand = feld("lernstand");
  stand.textContent = "Messwerte werden geladen …";
  try {
    const antwort = await fetch(bereich.dataset.messung);
    if (!antwort.ok) { throw new Error("Messwerte nicht lesbar"); }
    const lauf = await antwort.json();
    const netz = lauf.model;
    const training = lauf.train_report;
    const schichten = [netz.input_size, netz.hidden_width, netz.hidden_width, netz.output_size];
    const gewichte = schichten.slice(1).reduce((summe, anzahl, i) => summe + anzahl * schichten[i], 0);
    const bias = schichten.slice(1).reduce((summe, anzahl) => summe + anzahl, 0);
    feld("lernnetz").textContent = "Neuro2026 v" + lauf.version + " · " + schichten.join(" → ") +
      " Neuronen. Die beiden inneren Schichten verwenden SiLU, der Ausgang ist linear. Beim Training verändert das Netz " +
      (gewichte + bias) + " Parameter (" + gewichte + " Gewichte und " + bias + " Biaswerte).";
    zeichneLernnetz(feld("lernaufbau"), schichten);
    feld("lernplan").textContent = training.samples + " Trainingspaare · " + training.epochs +
      " Epochen · AdamW auf der CPU · Startwert " + lauf.seed + ".";
    feld("lernfehler").textContent = "Auf dem vollständigen Trainingssatz fiel der mittlere quadratische Fehler von " +
      zahl(training.initial_loss) + " auf " + zahl(training.final_loss) + ".";
    feld("lernvergleich").textContent = "Mittlere absolute Abweichung auf " + lauf.test_rows.length +
      " untrainierten Zwischenwerten: " + zahl(lauf.scores.mae) + ". Die einfache Regel „immer den Trainingsmittelwert " +
      zahl(lauf.baseline.value) + " vorhersagen“ erreichte " + zahl(lauf.baseline.scores.mae) + ".";
    feld("lernladen").textContent = "Nach Speichern und Laden änderte sich eine Vorhersage um höchstens " +
      zahl(lauf.reload.max_abs_difference) + " (Toleranz " + zahl(lauf.reload.tolerance) + ").";
    const auswahl = feld("lernpunkt");
    for (const fall of lauf.test_rows) {
      auswahl.add(new Option(fall.features.map(zahl).join(" × ")));
    }
    function zeigeVorhersage() {
      const fall = lauf.test_rows[auswahl.selectedIndex];
      feld("lernantwort").textContent = "Sollwert " + zahl(fall.target) + " · Vorhersage " + zahl(fall.prediction) +
        " · absoluter Fehler " + zahl(Math.abs(fall.target - fall.prediction)) + ".";
    }
    auswahl.addEventListener("change", zeigeVorhersage);
    zeigeVorhersage();
    zeichneLernkurve(feld("lernkurve"), training.metrics);
    bereich.hidden = false;
    stand.hidden = true;
  } catch (_) {
    stand.textContent = "Die Laufansicht konnte die Messwerte nicht anzeigen. Du kannst die Originalmesswerte oben öffnen.";
  }
})();

function svgElement(svg, name, merkmale, text) {
  const e = document.createElementNS("http://www.w3.org/2000/svg", name);
  for (const [schluessel, wert] of Object.entries(merkmale)) { e.setAttribute(schluessel, wert); }
  if (text !== undefined) { e.textContent = text; }
  svg.append(e);
}

function zeichneLernnetz(svg, groessen) {
  const gruppen = groessen.map((anzahl, schicht) => Array.from({length:anzahl}, (_, i) => ({
    x:45 + 130 * schicht, y:94 + (i - (anzahl - 1) / 2) * (schicht === 0 ? Math.min(55, 165 / Math.max(anzahl - 1, 1)) : Math.min(11, 165 / Math.max(anzahl - 1, 1)))
  })));
  const linien = [];
  for (let schicht = 1; schicht < gruppen.length; schicht++) {
    for (const von of gruppen[schicht - 1]) {
      for (const bis of gruppen[schicht]) { linien.push("M" + von.x + " " + von.y + "L" + bis.x + " " + bis.y); }
    }
  }
  svgElement(svg, "path", {d:linien.join(" "), fill:"none", stroke:"currentColor", opacity:0.13});
  const namen = ["Eingaben", "SiLU", "SiLU", "Ausgang"];
  gruppen.forEach((punkte, i) => {
    for (const punkt of punkte) {
      svgElement(svg, "circle", {cx:punkt.x, cy:punkt.y, r:4, fill:"var(--nav-akzent)"});
    }
    svgElement(svg, "text", {x:punkte[0].x, y:203, fill:"currentColor", "text-anchor":"middle", "font-size":13},
      namen[i] + " · " + punkte.length);
  });
}

(function depotLernlauf() {
  for (const netz of document.querySelectorAll("[data-netzschichten]")) {
    zeichneLernnetz(netz, JSON.parse(netz.dataset.netzschichten));
  }
  const bereich = document.querySelector("[data-depotmodell]");
  if (!bereich?.dataset?.depotmodell) { return; }
  const lauf = JSON.parse(bereich.dataset.depotmodell);
  const format = (n) => n.toLocaleString("de-DE", {maximumSignificantDigits: 4});
  bereich.querySelector("[data-trainingsfehler]").textContent = lauf.paare + " Trainingspaare · " +
    lauf.fehler.length + " Epochen · Fehler vor dem Training " + format(lauf.anfang) +
    " · danach " + format(lauf.ende);
  const kurve = bereich.querySelector("svg");
  const hoehe = Math.max(...lauf.fehler) || 1;
  const x = (i) => 68 + i / Math.max(lauf.fehler.length - 1, 1) * 500;
  const y = (fehler) => 175 - fehler / hoehe * 150;
  for (const wert of [0, hoehe / 2, hoehe]) {
    svgElement(kurve, "line", {x1:68, y1:y(wert), x2:568, y2:y(wert), stroke:"currentColor", opacity:0.25});
    svgElement(kurve, "text", {x:58, y:y(wert) + 4, "text-anchor":"end", fill:"currentColor", "font-size":13}, format(wert));
  }
  svgElement(kurve, "polyline", {points:lauf.fehler.map((wert, i) => x(i) + "," + y(wert)).join(" "),
    fill:"none", stroke:"var(--nav-akzent)", "stroke-width":2.5});
  svgElement(kurve, "circle", {cx:x(0), cy:y(lauf.fehler[0]), r:3, fill:"var(--nav-akzent)"});
  svgElement(kurve, "text", {x:68, y:196, "text-anchor":"middle", fill:"currentColor", "font-size":13}, "1");
  if (lauf.fehler.length > 1) {
    svgElement(kurve, "text", {x:568, y:196, "text-anchor":"middle", fill:"currentColor", "font-size":13}, lauf.fehler.length);
  }
  svgElement(kurve, "text", {x:318, y:196, "text-anchor":"middle", fill:"currentColor", "font-size":13}, "Epoche");
})();

function zeichneLernkurve(svg, epochen) {
  const werte = epochen.map((m) => Math.log10(m.mean_loss));
  const unten = Math.floor(Math.min(...werte));
  const oben = Math.ceil(Math.max(...werte));
  const y = (wert) => 175 - (wert - unten) / (oben - unten) * 155;
  const x = (i) => 68 + i / (epochen.length - 1) * 500;
  for (let stufe = unten; stufe <= oben; stufe++) {
    svgElement(svg, "line", {x1:68, y1:y(stufe), x2:568, y2:y(stufe), stroke:"currentColor", opacity:"0.25"});
    svgElement(svg, "text", {x:58, y:y(stufe) + 4, "text-anchor":"end", fill:"currentColor", "font-size":13},
      (10 ** stufe).toLocaleString("de-DE", { maximumSignificantDigits: 2 }));
  }
  svgElement(svg, "polyline", {points:werte.map((wert, i) => x(i) + "," + y(wert)).join(" "),
    fill:"none", stroke:"var(--nav-akzent)", "stroke-width":2.5});
  for (const i of [0, Math.floor(epochen.length / 2) - 1, epochen.length - 1]) {
    svgElement(svg, "text", {x:x(i), y:196, "text-anchor":"middle", fill:"currentColor", "font-size":13}, epochen[i].epoch);
  }
  svgElement(svg, "text", {x:425, y:196, fill:"currentColor", "font-size":13}, "Epoche");
}
