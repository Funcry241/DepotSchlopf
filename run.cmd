@echo off
setlocal EnableExtensions
cd /d "%~dp0"
rem DepotSchlopf – einziger Einstieg (ein Befehl).
rem   run.cmd                  alles: Gate pruefen, danach Assistenten-Export assistant_export_latest.zip
rem   run.cmd festschreibung   Festschreibung neu schreiben (nur fuer FestschreibungAenderung)
rem   run.cmd kurse            Schlusskurse holen, im Journal belegen und pruefen (nur lesend)
rem   run.cmd hilfe            Hilfe
where cargo >nul 2>&1
if errorlevel 1 (
    echo [DepotSchlopf] FEHLER: Rust fehlt. Bitte einmal von https://rustup.rs installieren. 1>&2
    exit /b 1
)
cargo run --quiet -p depot_lauf -- %*
exit /b %errorlevel%
