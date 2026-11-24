#Requires -Version 5.1
<#
.SYNOPSIS
    Build the zihuan_next binary plus the zihuan_composer CLI and
    zihuan_composer_gui desktop binaries, and package a distributable release
    archive (see .github/workflows/build.yml and .github/workflows/composer.yml).

.PARAMETER Variant
    'cpu' or 'cuda'. When omitted an interactive prompt is shown.
    Metal is not available on Windows; use build.sh on macOS.

.PARAMETER SkipFrontend
    Skip the pnpm webui build. Requires an existing webui/dist.

.PARAMETER SkipBuild
    Skip cargo and repackage the existing target/release/zihuan_next.exe.
#>
param(
    [ValidateSet('cpu', 'cuda')]
    [string]$Variant,
    [switch]$SkipFrontend,
    [switch]$SkipBuild
)

$ErrorActionPreference = 'Stop'

$repoRoot = $PSScriptRoot
Set-Location $repoRoot

function Assert-Command {
    param([string]$Name, [string]$Hint)
    if (-not (Get-Command $Name -ErrorAction SilentlyContinue)) {
        throw "zihuan-next: '$Name' not found on PATH. $Hint"
    }
}

if (-not $Variant) {
    Write-Host 'zihuan-next: select build variant'
    Write-Host '  1) CPU'
    Write-Host '  2) CUDA (candle-cuda, requires CUDA toolkit + MSVC)'
    $choice = Read-Host 'Enter choice [1]'
    if ($choice -eq '2') { $Variant = 'cuda' } else { $Variant = 'cpu' }
}
Write-Host "zihuan-next: variant=$Variant"

Assert-Command 'cargo' 'Install Rust via rustup.'
if (-not $SkipFrontend) {
    Assert-Command 'pnpm' 'Install pnpm (e.g. corepack enable).'
}
if ($Variant -eq 'cuda' -and -not $SkipBuild) {
    Assert-Command 'nvcc' 'Install the CUDA toolkit and put nvcc on PATH.'
}

if (-not $SkipFrontend) {
    Write-Host 'zihuan-next: building webui'
    Push-Location (Join-Path $repoRoot 'webui')
    try {
        & pnpm install --frozen-lockfile
        if ($LASTEXITCODE -ne 0) { throw 'zihuan-next: pnpm install failed' }
        & pnpm run build
        if ($LASTEXITCODE -ne 0) { throw 'zihuan-next: pnpm build failed' }
    }
    finally {
        Pop-Location
    }
}
else {
    if (-not (Test-Path 'webui/dist')) {
        throw 'zihuan-next: webui/dist not found; run without -SkipFrontend first.'
    }
    Write-Host 'zihuan-next: skipping webui build (-SkipFrontend)'
}

if ($SkipBuild) {
    foreach ($artifact in @('target/release/zihuan_next.exe', 'target/release/zihuan_composer.exe', 'target/release/zihuan_composer_gui.exe')) {
        if (-not (Test-Path $artifact)) {
            throw "zihuan-next: $artifact not found; run without -SkipBuild first."
        }
    }
    Write-Host 'zihuan-next: skipping cargo build (-SkipBuild)'
}
else {
    if ($Variant -eq 'cuda') {
        Write-Host 'zihuan-next: building CUDA binary'
        & (Join-Path $repoRoot 'scripts/cargo-cuda.ps1') -Release
        if ($LASTEXITCODE -ne 0) { throw 'zihuan-next: cargo CUDA build failed' }
    }
    else {
        Write-Host 'zihuan-next: building CPU binary'
        & cargo build --release -p zihuan_service --bin zihuan_next
        if ($LASTEXITCODE -ne 0) { throw 'zihuan-next: cargo build failed' }
    }
    Write-Host 'zihuan-next: building composer binaries'
    & cargo build --release -p zihuan_composer -p zihuan_composer_gui
    if ($LASTEXITCODE -ne 0) { throw 'zihuan-next: cargo composer build failed' }
}

# The version string is decided once in zihuan_service/build.rs and stamped into the
# binary, so ask the binary instead of re-deriving it from git here.
$version = 'dev'
$builtBinary = 'target/release/zihuan_next.exe'
if (Test-Path $builtBinary) {
    $reportedVersion = & $builtBinary --version 2>$null | Select-Object -Last 1
    if ($LASTEXITCODE -eq 0 -and $reportedVersion) {
        $version = ([string]$reportedVersion).Trim() -split '\s+' | Select-Object -Last 1
    }
}

$packageRoot = 'package/zihuan_next'
$archive = 'package/zihuan_next-' + $version + '-Windows-x86_64-' + $Variant + '.zip'
if (Test-Path $packageRoot) { Remove-Item -Recurse -Force $packageRoot }
if (Test-Path $archive) { Remove-Item -Force $archive }
New-Item -ItemType Directory -Force -Path "$packageRoot/dag_nodes", "$packageRoot/dynamic_script_engine", "$packageRoot/sub_agents", "$packageRoot/scheduled_jobs", "$packageRoot/plugins" | Out-Null
Copy-Item 'target/release/zihuan_next.exe' $packageRoot
Copy-Item 'target/release/zihuan_composer.exe' $packageRoot
Copy-Item 'target/release/zihuan_composer_gui.exe' $packageRoot
Copy-Item 'package.json' $packageRoot
Copy-Item 'dag_nodes/*' "$packageRoot/dag_nodes" -Recurse
Copy-Item 'sub_agents/*' "$packageRoot/sub_agents" -Recurse
Copy-Item 'scheduled_jobs/*' "$packageRoot/scheduled_jobs" -Recurse
Copy-Item 'plugins/*' "$packageRoot/plugins" -Recurse
Get-ChildItem $packageRoot -Recurse -Directory -Filter '__pycache__' | Remove-Item -Recurse -Force
Get-ChildItem "$packageRoot/plugins" -Recurse -Directory -Filter '.venv' | Remove-Item -Recurse -Force
Get-ChildItem "$packageRoot/plugins" -Recurse -File -Include 'uv.lock', '.python-version' | Remove-Item -Force
$runtimeFiles = @('engine.mjs', 'engine_runtime.py', 'zihuan_sdk.mjs', 'zihuan_sdk.py', 'package.json')
foreach ($file in $runtimeFiles) {
    Copy-Item "dynamic_script_engine/$file" "$packageRoot/dynamic_script_engine"
}
Copy-Item 'build_support/release/pyproject.toml' "$packageRoot/pyproject.toml"
Compress-Archive -Path $packageRoot -DestinationPath $archive -Force

Write-Host "zihuan-next: package created: $archive"
