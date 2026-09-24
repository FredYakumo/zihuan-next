#Requires -Version 5.1
<#
.SYNOPSIS
    Build the zihuan_next binary and package a distributable release archive
    with the same layout as the CI release package (see .github/workflows/build.yml).

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
    if (-not (Test-Path 'target/release/zihuan_next.exe')) {
        throw 'zihuan-next: target/release/zihuan_next.exe not found; run without -SkipBuild first.'
    }
    Write-Host 'zihuan-next: skipping cargo build (-SkipBuild)'
}
elseif ($Variant -eq 'cuda') {
    Write-Host 'zihuan-next: building CUDA binary'
    & (Join-Path $repoRoot 'scripts/cargo-cuda.ps1') -Release
    if ($LASTEXITCODE -ne 0) { throw 'zihuan-next: cargo CUDA build failed' }
}
else {
    Write-Host 'zihuan-next: building CPU binary'
    & cargo build --release -p zihuan_service --bin zihuan_next
    if ($LASTEXITCODE -ne 0) { throw 'zihuan-next: cargo build failed' }
}

$version = 'dev'
if (Get-Command git -ErrorAction SilentlyContinue) {
    $gitDescribe = & git describe --tags --always --dirty 2>$null
    if ($LASTEXITCODE -eq 0 -and $gitDescribe) {
        $version = ([string]$gitDescribe).Trim()
    }
}

$packageRoot = 'package/zihuan_next'
$archive = 'package/zihuan_next-' + $version + '-Windows-x86_64-' + $Variant + '.zip'
if (Test-Path $packageRoot) { Remove-Item -Recurse -Force $packageRoot }
if (Test-Path $archive) { Remove-Item -Force $archive }
New-Item -ItemType Directory -Force -Path "$packageRoot/dag_nodes", "$packageRoot/dynamic_script_engine", "$packageRoot/sub_agents", "$packageRoot/scheduled_jobs" | Out-Null
Copy-Item 'target/release/zihuan_next.exe' $packageRoot
Copy-Item 'package.json' $packageRoot
Copy-Item 'dag_nodes/*' "$packageRoot/dag_nodes" -Recurse
Copy-Item 'sub_agents/*' "$packageRoot/sub_agents" -Recurse
Copy-Item 'scheduled_jobs/*' "$packageRoot/scheduled_jobs" -Recurse
Get-ChildItem $packageRoot -Recurse -Directory -Filter '__pycache__' | Remove-Item -Recurse -Force
$runtimeFiles = @('engine.mjs', 'engine_runtime.py', 'zihuan_sdk.mjs', 'zihuan_sdk.py', 'package.json')
foreach ($file in $runtimeFiles) {
    Copy-Item "dynamic_script_engine/$file" "$packageRoot/dynamic_script_engine"
}
Copy-Item 'build_support/release/pyproject.toml' "$packageRoot/pyproject.toml"
Compress-Archive -Path $packageRoot -DestinationPath $archive -Force

Write-Host "zihuan-next: package created: $archive"
