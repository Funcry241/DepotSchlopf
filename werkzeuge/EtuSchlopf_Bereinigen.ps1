# EtuSchlopf_Bereinigen.ps1
# Raeumt erzeugte Rust-Bauzwischenstaende im EtuSchlopf-Ordner auf.
#
# Standard: NUR ANZEIGEN, nichts wird geloescht.
#   powershell -ExecutionPolicy Bypass -File EtuSchlopf_Bereinigen.ps1 -Ordner "C:\...\EtuSchlopf\20261115_01"
# Wirklich loeschen:
#   ... -Ausfuehren
# Optional zusaetzlich:
#   -Gruendlich            auch deps\ und build\ unter target\bc (naechster Bau dauert laenger)
#   -Behalten N            die N neuesten Ablehnungsbelege behalten (Standard 0 = alle weg)
#
# Ablehnungsbelege (target\etuschlopf_take_rejection_evidence_v1\rejected-r*) werden
# standardmaessig mit entfernt (werden nicht mehr gebraucht).
#
# Nie angefasst: alles ausserhalb von target\ (Quelltext, .mid, .wav, Cargo-Dateien,
# docs, tests, tools, Ergebnisse im Wurzelordner), target\strict\etuschlopf.exe,
# Ledger, Annahme-Belege, Logs, Export, Recovery, Quittungen in target\.

[CmdletBinding()]
param(
    [string]$Ordner = (Get-Location).Path,
    [switch]$Ausfuehren,
    [switch]$Gruendlich,
    [int]$Behalten = 0
)

$ErrorActionPreference = 'Stop'

function Abbruch($text) { Write-Host "ABBRUCH: $text" -ForegroundColor Red; exit 1 }

# --- Sicherheitspruefungen -------------------------------------------------
$root = (Resolve-Path -LiteralPath $Ordner).Path.TrimEnd('\')
$target = Join-Path $root 'target'
if (-not (Test-Path -LiteralPath (Join-Path $root 'Cargo.toml'))) { Abbruch "Keine Cargo.toml in $root - falscher Ordner?" }
if (-not (Test-Path -LiteralPath (Join-Path $root 'src'))) { Abbruch "Kein src\ in $root - falscher Ordner?" }
if (-not (Test-Path -LiteralPath (Join-Path $target 'CACHEDIR.TAG'))) { Abbruch "target\CACHEDIR.TAG fehlt - das ist kein Cargo-target-Ordner." }
if (((Get-Item -LiteralPath $target -Force).Attributes -band [IO.FileAttributes]::ReparsePoint)) { Abbruch "target\ ist eine Verknuepfung (Junction/Symlink)." }

if ($Ausfuehren) {
    $laufend = Get-Process -ErrorAction SilentlyContinue | Where-Object { $_.Name -match '^(cargo|rustc|clippy-driver|etuschlopf.*)$' }
    if ($laufend) { Abbruch ("Bitte erst beenden: " + (($laufend | Select-Object -ExpandProperty Name -Unique) -join ', ')) }
}

# --- Kandidaten sammeln ----------------------------------------------------
$kandidaten = New-Object System.Collections.Generic.List[object]
function Kandidat($pfad, $grund) {
    if (-not (Test-Path -LiteralPath $pfad -PathType Container)) { return }
    $voll = (Resolve-Path -LiteralPath $pfad).Path
    if (-not $voll.StartsWith($target + '\', [StringComparison]::OrdinalIgnoreCase)) { return }  # nur unter target\
    $item = Get-Item -LiteralPath $voll -Force
    if ($item.Attributes -band [IO.FileAttributes]::ReparsePoint) { Write-Host "Uebersprungen (Verknuepfung): $voll" -ForegroundColor Yellow; return }
    $kandidaten.Add([pscustomobject]@{ Pfad = $voll; Grund = $grund })
}

# 1) Inkrementelle Kompilierstaende: reiner Cache, cargo baut ihn neu auf.
Get-ChildItem -LiteralPath $target -Directory -Recurse -Force -Filter 'incremental' -ErrorAction SilentlyContinue |
    Where-Object { $_.FullName -notmatch '\\incremental\\' } |
    ForEach-Object { Kandidat $_.FullName 'Rust-Inkrementalcache' }

# 2) Eigener Cargo-Bauordner des WAV-Pitch-Audits (vollstaendig neu baubar).
Kandidat (Join-Path $target 'wav_pitch_external_sensor_audit\cargo_target') 'eigener Cargo-Bauordner (Audit)'

# 3) Bau-Zeitmessungen (HTML-Berichte).
Kandidat (Join-Path $target 'bc\cargo-timings') 'Cargo-Zeitberichte'

# 4) Optional: Abhaengigkeiten im bc-Bauordner.
if ($Gruendlich) {
    foreach ($p in 'debug', 'strict') {
        Kandidat (Join-Path $target "bc\$p\deps")  'bc-Abhaengigkeiten (neu baubar)'
        Kandidat (Join-Path $target "bc\$p\build") 'bc-Build-Skripte (neu baubar)'
    }
}

# 5) Ablehnungsbelege, die neuesten $Behalten bleiben.
if ($true) {
    $ev = Join-Path $target 'etuschlopf_take_rejection_evidence_v1'
    if (Test-Path -LiteralPath $ev) {
        $runs = Get-ChildItem -LiteralPath $ev -Directory -Force |
            Where-Object { $_.Name -match '^rejected-r(\d+)-[0-9a-f]+$' } |
            Sort-Object { [int]([regex]::Match($_.Name, '^rejected-r(\d+)-').Groups[1].Value) } -Descending
        $runs | Select-Object -Skip ([Math]::Max(0, $Behalten)) |
            ForEach-Object { Kandidat $_.FullName 'alter Ablehnungsbeleg' }
    }
}

# --- Anzeigen / Loeschen ---------------------------------------------------
function GroesseVon($pfad) {
    $s = (Get-ChildItem -LiteralPath $pfad -File -Recurse -Force -ErrorAction SilentlyContinue | Measure-Object Length -Sum).Sum
    if ($null -eq $s) { 0 } else { [int64]$s }
}
function GB($b) { '{0:N2} GB' -f ($b / 1GB) }

$summe = [int64]0
$modus = if ($Ausfuehren) { 'LOESCHEN' } else { 'NUR ANZEIGE (nichts wird geloescht)' }
Write-Host "EtuSchlopf-Bereinigung - $modus" -ForegroundColor Cyan
Write-Host "Ordner: $root`n"

foreach ($k in $kandidaten) {
    $g = GroesseVon $k.Pfad
    $summe += $g
    $rel = $k.Pfad.Substring($root.Length + 1)
    Write-Host ('{0,10}  {1}  [{2}]' -f (GB $g), $rel, $k.Grund)
    if ($Ausfuehren) {
        try { Remove-Item -LiteralPath $k.Pfad -Recurse -Force }
        catch { cmd /c "rd /s /q `"\\?\$($k.Pfad)`"" | Out-Null }
        if (Test-Path -LiteralPath $k.Pfad) { Write-Host "    -> konnte nicht vollstaendig geloescht werden" -ForegroundColor Yellow }
    }
}

Write-Host ''
if ($Ausfuehren) {
    Write-Host ("Freigegeben: etwa {0} in {1} Ordnern." -f (GB $summe), $kandidaten.Count) -ForegroundColor Green
    Write-Host 'Der naechste Bau dauert einmalig laenger (Cache wird neu aufgebaut).'
} else {
    Write-Host ("Wuerde etwa {0} in {1} Ordnern freigeben. Zum Loeschen mit -Ausfuehren starten." -f (GB $summe), $kandidaten.Count) -ForegroundColor Green
}
