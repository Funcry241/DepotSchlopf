@echo off
setlocal EnableExtensions
cd /d "%~dp0"
rem DepotSchlopf – Dienst mit Oberflaeche, Start per Doppelklick.
rem   DepotSchlopf.cmd                  Dienst starten und http://127.0.0.1:47410/ oeffnen.
rem                                     Laeuft er schon, oeffnet sich nur die Seite.
rem   DepotSchlopf.cmd --ohne-browser   ohne Seite (so startet der Autostart)
rem Der Dienst lauscht nur auf 127.0.0.1. Fenster schliessen beendet ihn.
title DepotSchlopf
where cargo >nul 2>&1
if errorlevel 1 (
    echo [DepotSchlopf] FEHLER: Rust fehlt. Bitte einmal von https://rustup.rs installieren. 1>&2
    pause
    exit /b 1
)
cargo run --quiet -p depot_dienst -- %*
if errorlevel 1 pause
exit /b %errorlevel%
