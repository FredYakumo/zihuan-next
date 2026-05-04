$ErrorActionPreference = 'Stop'

function Get-MsvcClPath {
    $programFilesX86 = [Environment]::GetFolderPath('ProgramFilesX86')
    $vswhere = Join-Path $programFilesX86 'Microsoft Visual Studio\Installer\vswhere.exe'
    if (Test-Path $vswhere) {
        $installPath = (& $vswhere -latest -products * -requires Microsoft.VisualStudio.Component.VC.Tools.x86.x64 -property installationPath) | Select-Object -First 1
        if ($installPath) {
            $glob = Join-Path $installPath 'VC\Tools\MSVC\*\bin\Hostx64\x64\cl.exe'
            $candidate = Get-ChildItem -Path $glob -File -ErrorAction SilentlyContinue |
                Sort-Object -Property FullName -Descending |
                Select-Object -First 1
            if ($candidate) {
                return $candidate.FullName
            }
        }
    }

    $fallbackGlobs = @(
        'C:\Program Files\Microsoft Visual Studio\*\*\VC\Tools\MSVC\*\bin\Hostx64\x64\cl.exe',
        'C:\Program Files (x86)\Microsoft Visual Studio\*\*\VC\Tools\MSVC\*\bin\Hostx64\x64\cl.exe'
    )

    foreach ($g in $fallbackGlobs) {
        $candidate = Get-ChildItem -Path $g -File -ErrorAction SilentlyContinue |
            Sort-Object -Property FullName -Descending |
            Select-Object -First 1
        if ($candidate) {
            return $candidate.FullName
        }
    }

    $fromPath = Get-Command cl.exe -ErrorAction SilentlyContinue
    if ($fromPath -and $fromPath.Source) {
        return $fromPath.Source
    }

    return $null
}

function Ensure-CandleCudaFeature {
    param([string[]]$Args)

    if (-not $Args -or $Args.Count -eq 0) {
        return @('build', '--features', 'candle-cuda')
    }

    if ($Args -contains '--all-features') {
        return $Args
    }

    $result = New-Object System.Collections.Generic.List[string]
    $hasFeatureFlag = $false

    for ($i = 0; $i -lt $Args.Count; $i++) {
        $arg = $Args[$i]

        if ($arg -eq '--features') {
            $hasFeatureFlag = $true
            $result.Add($arg)
            if ($i + 1 -lt $Args.Count) {
                $featureValue = $Args[$i + 1]
                if ($featureValue -notmatch '(^|,)candle-cuda($|,)') {
                    $featureValue = "$featureValue,candle-cuda"
                }
                $result.Add($featureValue)
                $i++
            }
            else {
                $result.Add('candle-cuda')
            }
            continue
        }

        if ($arg.StartsWith('--features=')) {
            $hasFeatureFlag = $true
            $featureValue = $arg.Substring(11)
            if ($featureValue -notmatch '(^|,)candle-cuda($|,)') {
                $featureValue = "$featureValue,candle-cuda"
            }
            $result.Add("--features=$featureValue")
            continue
        }

        $result.Add($arg)
    }

    if (-not $hasFeatureFlag) {
        $result.Add('--features')
        $result.Add('candle-cuda')
    }

    return $result.ToArray()
}

$clPath = Get-MsvcClPath
if (-not $clPath) {
    Write-Error 'MSVC cl.exe not found. Install Visual Studio Build Tools with the C++ workload.'
    exit 1
}

$env:NVCC_CCBIN = $clPath
$rawArgs = @()
if ($args -and $args.Count -gt 0) {
    $rawArgs = $args
}

$finalArgs = Ensure-CandleCudaFeature -Args $rawArgs

Write-Host "zihuan-next: NVCC_CCBIN=$clPath"
Write-Host "zihuan-next: cargo $($finalArgs -join ' ')"

& cargo @finalArgs
exit $LASTEXITCODE
