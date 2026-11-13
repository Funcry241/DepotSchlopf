@echo off
setlocal EnableExtensions
cd /d "%~dp0"
rem DepotSchlopf – einziger Einstieg (ein Befehl).
rem   run.cmd                  alles: Gate pruefen, danach Assistenten-Export assistant_export_latest.zip
rem   run.cmd festschreibung   Festschreibung neu schreiben (nur fuer FestschreibungAenderung)
rem   run.cmd kurse [xetra|nyse]  Schlusskurse holen (alle oder nur einer Boerse), im Journal belegen und pruefen (nur lesend)
rem   run.cmd nachrichten      nur die Nachrichten holen und im Journal belegen (nur lesend)
rem   run.cmd termine          Termine der EZB holen und im Journal belegen (nur lesend)
rem   run.cmd papier           Papiertag mit Dachs und Maus und den heute belegten Schlusskursen (nur Papiergeld)
rem   run.cmd lage             Lagebild nach daten/lage.html (Bereitschaft, KI-Budget, Depot)
rem   run.cmd sichern          Daten auf das eingestellte Ziel sichern und pruefen
rem   run.cmd update pruefen   neue Staende auf main nennen
rem   run.cmd update           neuen Stand im Pruefordner mit dem Gate pruefen, nur dann vorwaerts einspielen
rem   run.cmd hilfe            Hilfe
where cargo >nul 2>&1
if errorlevel 1 (
    echo [DepotSchlopf] FEHLER: Rust fehlt. Bitte einmal von https://rustup.rs installieren. 1>&2
    exit /b 1
)
rem Ein Klammerblock: Aendert `run.cmd update` diese Datei, liest cmd danach nichts Neues nach.
(
    cargo run --quiet -p depot_lauf -- %*
    if errorlevel 2 exit /b 2
    if errorlevel 1 exit /b 1
    exit /b 0
)
