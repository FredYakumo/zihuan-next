# Checks zihuan-next dynamic script plugins (syntax + plugin conventions).
# Usage: scripts/check_plugins.ps1 [plugin-or-directory] [-Full]
param(
    [Parameter(Position = 0)][string]$Target,
    [switch]$Full
)

$ErrorActionPreference = 'Stop'

# The executable test filters out the Windows Store python stub, which exists on PATH but
# only prints an install hint and exits non-zero. ErrorAction is relaxed around the probe
# because PS 5.1 turns native stderr output into errors under 'Stop'.
$python = $null
foreach ($candidate in @('python', 'py')) {
    $command = Get-Command $candidate -ErrorAction SilentlyContinue
    if (-not $command) { continue }
    $ErrorActionPreference = 'Continue'
    & $command.Source -c 'import sys' 2>$null
    $exitCode = $LASTEXITCODE
    $ErrorActionPreference = 'Stop'
    if ($exitCode -eq 0) {
        $python = $command.Source
        break
    }
}
if (-not $python) {
    Write-Error 'Python 3 not found. Install Python first.'
    exit 1
}

[Console]::OutputEncoding = [System.Text.Encoding]::UTF8

$checkerArgs = @()
if ($Target) { $checkerArgs += $Target }
if ($Full) { $checkerArgs += '--full' }

& $python (Join-Path $PSScriptRoot 'check_plugins.py') @checkerArgs
exit $LASTEXITCODE
