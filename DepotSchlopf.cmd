@echo off
setlocal EnableExtensions DisableDelayedExpansion
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
title DepotSchlopf - nicht schliessen
rem Der folgende PowerShell-Aufruf verwendet nur feste Befehle. daten/ liefert nur Daten.
set "DEPOT_START_ARG=%~1"
set "PRUEFE_BAUM=$ErrorActionPreference='Stop'; try { if (Test-Path -LiteralPath 'daten/update_offen.txt') { exit 2 }; $status=git status --porcelain; if ($LASTEXITCODE -ne 0 -or $status) { exit 2 }; $baum=git rev-parse --verify 'HEAD^{tree}'; if ($LASTEXITCODE -ne 0 -or $baum -cnotmatch '^(?:[0-9a-f]{40}|[0-9a-f]{64})$') { exit 2 }; if (('geprueft-v1;'+$baum) -cnotin (Get-Content -LiteralPath 'daten/geprueft.txt')) { exit 2 };"
set "PRUEFE_STAND=$name=$baum.Substring(0,16); if ((Get-Content -Raw -LiteralPath 'daten/programm/aktuell.txt').Trim() -cne $name) { exit 2 }; $ordner=Join-Path 'daten/programm' $name; $stand=@(Get-Content -LiteralPath (Join-Path $ordner 'stand.txt')); $programme=@('depot_waechter','depot_dienst','depot_sammler','depot_berater'); if ($stand.Count -ne 6 -or $stand[0] -cne 'programm-v1' -or $stand[1] -cne ('baum='+$baum)) { exit 2 };"
set "PRUEFE_DATEIEN=for ($i=0; $i -lt $programme.Count; $i++) { $programm=$programme[$i]; $m=[regex]::Match($stand[$i+2],('^'+$programm+'=([0-9a-f]{64})$')); if (!$m.Success) { exit 2 }; $pfad=Join-Path $ordner ($programm+'.exe'); if ((Get-FileHash -LiteralPath $pfad -Algorithm SHA256).Hash -ne $m.Groups[1].Value) { exit 2 } }; $waechter=(Resolve-Path -LiteralPath (Join-Path $ordner 'depot_waechter.exe')).Path } catch { exit 2 };"
set "STARTE_WAECHTER=if ($env:DEPOT_START_ARG) { & $waechter $env:DEPOT_START_ARG } else { & $waechter }; if ($LASTEXITCODE -eq 0) { exit 0 }; exit 1"
(
    if exist "daten\programm\aktuell.txt" (
        powershell.exe -NoProfile -NonInteractive -Command "%PRUEFE_BAUM% %PRUEFE_STAND% %PRUEFE_DATEIEN% %STARTE_WAECHTER%"
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
