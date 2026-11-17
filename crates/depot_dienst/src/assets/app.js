"use strict";
(function () {
  // Änderungen bleiben im Formular; insbesondere kein Schlüssel in Browser-Speichern.
  var gehtWeg = false;
  var andererStart = false;
  var bearbeitet = new WeakSet();
  function formulare() { return Array.from(document.querySelectorAll("form[data-aenderungen]")); }
  function istGeaendert(f) {
    return f.dataset.entwurf === "ja" || Array.from(f.querySelectorAll("input:not([type=hidden])")).some(function (i) {
      if (!bearbeitet.has(i) || (i.type === "password" && i.matches(":autofill"))) { return false; }
      return i.type === "checkbox" ? i.checked !== i.defaultChecked : i.value !== i.defaultValue;
    });
  }
  function offen(ausser) { return formulare().some(function (f) { return f !== ausser && istGeaendert(f); }); }
  function zeige(f) {
    var neu = istGeaendert(f);
    f.classList.toggle("ungespeichert", neu);
    var status = f.querySelector("[data-speicherstand]");
    if (status) { status.textContent = neu ? "Noch nicht gespeichert. Bitte unten speichern, damit deine Änderungen gelten." : "Keine ungespeicherten Änderungen in diesem Formular."; }
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
  // Doppelklicks: Ein abgesendetes Formular sperrt seinen Knopf. Der Dienst lehnt doppelte
  // Aufträge ohnehin ab; so sieht man sofort, dass der Klick angekommen ist.
  document.addEventListener("submit", function (e) {
    if (andererStart) { e.preventDefault(); return; }
    if (offen(e.target) && !window.confirm("In einem anderen Formular stehen noch ungespeicherte Änderungen. Sie werden bei dieser Aktion verworfen. Trotzdem fortfahren?")) {
      e.preventDefault(); return;
    }
    // Ein Download lässt die Seite offen; der Exportknopf bleibt benutzbar.
    if (e.target.dataset.download !== undefined) { return; }
    var b = e.submitter || e.target.querySelector("button[type=submit]");
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
    ziel.setAttribute("data-denkt", daten.getAttribute("data-denkt"));
    ziel.setAttribute("data-otter", daten.getAttribute("data-otter"));
    ["dachs", "maus", "otter"].forEach(function (wer) {
      var blase = document.getElementById("kopf-" + wer);
      if (!blase) { return; }
      var satz = daten.getAttribute("data-" + (wer === "otter" ? "otter-satz" : wer)) || "";
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
    formulare().forEach(zeige);
    document.querySelectorAll("button[data-gesendet]").forEach(function (b) {
      b.textContent = b.dataset.gesendet;
      delete b.dataset.gesendet;
      b.disabled = false;
    });
    if (e.persisted) { pausiert = false; generation += 1; starte(); }
  });
  starte();
})();
