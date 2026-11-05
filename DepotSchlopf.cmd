@echo off
setlocal EnableExtensions
cd /d "%~dp0"
rem DepotSchlopf – Dienst mit Oberflaeche, Start per Doppelklick.
rem   DepotSchlopf.cmd                  Dienst starten und http://127.0.0.1:47410/ oeffnen.
rem                                     Laeuft er schon, oeffnet sich nur die Seite.
rem   DepotSchlopf.cmd --ohne-browser   ohne Seite (so startet der Autostart)
rem Der Dienst lauscht nur auf 127.0.0.1. Fenster schliessen beendet ihn.
rem Der Waechter (depot_waechter) baut und startet den Dienst. Nach einem Update startet er ihn
rem neu; kommt der neue Stand nicht hoch, setzt er den alten zurueck.
rem Alles nach dem Pruefen von Rust steht in EINEM Klammerblock: cmd liest ihn einmal ganz.
rem Aendert ein Update diese Datei, waehrend sie laeuft, liest cmd nichts Neues nach.
title DepotSchlopf
where cargo >nul 2>&1
if errorlevel 1 (
    echo [DepotSchlopf] FEHLER: Rust fehlt. Bitte einmal von https://rustup.rs installieren. 1>&2
    pause
    exit /b 1
)
(
    cargo run --quiet -p depot_dienst --bin depot_waechter -- %*
    if errorlevel 1 (
        pause
        exit /b 1
    )
    exit /b 0
)
