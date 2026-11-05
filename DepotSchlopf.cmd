@echo off
setlocal EnableExtensions
cd /d "%~dp0"
rem DepotSchlopf – Dienst mit Oberflaeche, Start per Doppelklick.
rem   DepotSchlopf.cmd                  Dienst starten und http://127.0.0.1:47410/ oeffnen.
rem                                     Laeuft er schon, oeffnet sich nur die Seite.
rem   DepotSchlopf.cmd --ohne-browser   ohne Seite (so startet der Autostart)
rem Der Dienst lauscht nur auf 127.0.0.1. Fenster schliessen beendet ihn.
rem Gibt es einen gesicherten, vom Gate geprueften Programmstand (daten\programm\starte.cmd),
rem startet dessen Waechter ohne Cargo; er prueft selbst, ob der Stand noch passt, und baut
rem sonst mit Cargo. Ohne gesicherten Stand baut und startet der Waechter (depot_waechter)
rem den Dienst mit Cargo. Nach einem Update startet er ihn neu; kommt der neue Stand nicht
rem hoch, setzt er den alten zurueck.
rem Alles steht in EINEM Klammerblock: cmd liest ihn einmal ganz. Aendert ein Update diese
rem Datei, waehrend sie laeuft, liest cmd nichts Neues nach.
title DepotSchlopf
(
    if exist "daten\programm\starte.cmd" (
        call "daten\programm\starte.cmd" %*
        if errorlevel 1 (
            pause
            exit /b 1
        )
        exit /b 0
    )
    where cargo >nul 2>&1
    if errorlevel 1 (
        echo [DepotSchlopf] FEHLER: Rust fehlt. Bitte einmal von https://rustup.rs installieren. 1>&2
        pause
        exit /b 1
    )
    cargo run --quiet -p depot_dienst --bin depot_waechter -- %*
    if errorlevel 1 (
        pause
        exit /b 1
    )
    exit /b 0
)
