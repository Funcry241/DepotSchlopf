@echo off
setlocal EnableExtensions
cd /d "%~dp0"
rem DepotSchlopf – einziger Einstieg.
rem   run.cmd kodex            Gate: Struktur, Festschreibung, Quellscan, Essenz, Format, Clippy, Tests
rem   run.cmd festschreibung   Festschreibung aus dem aktuellen Baum neu schreiben (nur fuer FestschreibungAenderung)
if /I "%~1"=="kodex" (
    cargo run --quiet -p depot_kodex -- --voll %2 %3
    exit /b %errorlevel%
)
if /I "%~1"=="festschreibung" (
    cargo run --quiet -p depot_kodex -- festschreibung
    exit /b %errorlevel%
)
echo Aufruf: run.cmd kodex ^| run.cmd festschreibung 1>&2
exit /b 2
