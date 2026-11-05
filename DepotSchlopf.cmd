@echo off
setlocal EnableExtensions
cd /d "%~dp0"
rem DepotSchlopf – Dienst mit Oberflaeche, Start per Doppelklick.
rem   DepotSchlopf.cmd                  Dienst starten und http://127.0.0.1:47410/ oeffnen.
rem                                     Laeuft er schon, oeffnet sich nur die Seite.
rem   DepotSchlopf.cmd --ohne-browser   ohne Seite (so startet der Autostart)
rem Der Dienst lauscht nur auf 127.0.0.1. Fenster schliessen beendet ihn.
rem Der mitgelieferte Starter prueft den gesicherten Stand, bevor ein Programm daraus laeuft.
rem Ist er nicht startklar, baut der Waechter mit Cargo.
rem Alles steht in EINEM Klammerblock: cmd liest ihn einmal ganz. Aendert ein Update diese
rem Datei, waehrend sie laeuft, liest cmd nichts Neues nach.
title DepotSchlopf
(
    if exist "daten\programm\aktuell.txt" (
        powershell.exe -NoProfile -NonInteractive -ExecutionPolicy Bypass -File "crates\depot_dienst\src\start-gesichert.ps1" %*
        if not errorlevel 2 (
            if errorlevel 1 (
                pause
                exit /b 1
            )
            exit /b 0
        )
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
