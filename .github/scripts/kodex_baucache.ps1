param(
    [Parameter(Mandatory)]
    [ValidateSet('gate', 'update')]
    [string]$Art
)

$ErrorActionPreference = 'Stop'
$projekt = (Get-Location).ProviderPath
$bau = 'bau'
if ($Art -eq 'update') {
    $projekt = [IO.Path]::GetFullPath((Join-Path $env:RUNNER_TEMP 'depot-update-alt/projekt'))
    $bau = 'bau-update'
}
$bytes = [Text.Encoding]::UTF8.GetBytes($projekt)
$abdruck = [Convert]::ToHexString([Security.Cryptography.SHA256]::HashData($bytes)).ToLowerInvariant()
$ablage = Join-Path $env:RUNNER_TEMP ('depotschlopf-' + $abdruck.Substring(0, 16))
$pfad = Join-Path $ablage $bau
"pfad=$pfad" >> $env:GITHUB_OUTPUT
