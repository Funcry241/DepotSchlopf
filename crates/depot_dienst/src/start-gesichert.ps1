# Exit 2 allows DepotSchlopf.cmd to build with Cargo. Treat metadata in daten/ as data.
$ErrorActionPreference = 'Stop'
try {
    if (Test-Path -LiteralPath 'daten/update_offen.txt') { exit 2 }
    $status = git status --porcelain
    if ($LASTEXITCODE -ne 0 -or $status) { exit 2 }
    $baum = git rev-parse --verify 'HEAD^{tree}'
    if ($LASTEXITCODE -ne 0 -or $baum -cnotmatch '^(?:[0-9a-f]{40}|[0-9a-f]{64})$') { exit 2 }
    if ("geprueft-v1;$baum" -cnotin (Get-Content -LiteralPath 'daten/geprueft.txt')) { exit 2 }

    $name = $baum.Substring(0, 16)
    $aktuell = (Get-Content -Raw -LiteralPath 'daten/programm/aktuell.txt').Trim()
    if ($aktuell -cne $name) { exit 2 }
    $ordner = Join-Path 'daten/programm' $name
    $stand = @(Get-Content -LiteralPath (Join-Path $ordner 'stand.txt'))
    $programme = @('depot_waechter', 'depot_dienst', 'depot_sammler', 'depot_berater')
    if ($stand.Count -ne 6 -or $stand[0] -cne 'programm-v1' -or $stand[1] -cne "baum=$baum") { exit 2 }
    for ($i = 0; $i -lt $programme.Count; $i++) {
        $programm = $programme[$i]
        $zeile = $stand[$i + 2]
        $eintrag = [regex]::Match($zeile, "^$programm=([0-9a-f]{64})$")
        if (!$eintrag.Success) { exit 2 }
        $soll = $eintrag.Groups[1].Value
        $pfad = Join-Path $ordner "$programm.exe"
        if ((Get-FileHash -LiteralPath $pfad -Algorithm SHA256).Hash -ne $soll) { exit 2 }
    }
    $waechter = (Resolve-Path -LiteralPath (Join-Path $ordner 'depot_waechter.exe')).Path
} catch {
    exit 2
}
& $waechter @args
if ($LASTEXITCODE -eq 0) { exit 0 }
exit 1
