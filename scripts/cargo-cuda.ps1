param([switch]$Release)

$ErrorActionPreference = 'Stop'

function Get-CudaVersion {
    $versionOutput = & nvcc --version 2>$null
    if ($LASTEXITCODE -ne 0) {
        return $null
    }

    $match = [regex]::Match(($versionOutput -join "`n"), 'release\s+(\d+\.\d+)')
    if ($match.Success) {
        return [Version]$match.Groups[1].Value
    }

    return $null
}

function Select-MsvcClPath {
    param([System.IO.FileInfo[]]$Candidates, [Version]$CudaVersion)

    $orderedCandidates = $Candidates | Sort-Object -Property FullName -Descending
    if ($CudaVersion -and $CudaVersion -lt [Version]'13.2') {
        $compatibleCandidate = $orderedCandidates | Where-Object {
            $versionMatch = [regex]::Match($_.FullName, 'MSVC\\(\d+\.\d+)')
            $versionMatch.Success -and [Version]$versionMatch.Groups[1].Value -lt [Version]'14.50'
        } | Select-Object -First 1
        if ($compatibleCandidate) {
            return $compatibleCandidate.FullName
        }
    }

    return ($orderedCandidates | Select-Object -First 1).FullName
}

function Get-MsvcClPath {
    $cudaVersion = Get-CudaVersion
    $programFilesX86 = [Environment]::GetFolderPath('ProgramFilesX86')
    $vswhere = Join-Path $programFilesX86 'Microsoft Visual Studio\Installer\vswhere.exe'
    if (Test-Path $vswhere) {
        $installPath = (& $vswhere -latest -products * -requires Microsoft.VisualStudio.Component.VC.Tools.x86.x64 -property installationPath) | Select-Object -First 1
        if ($installPath) {
            $glob = Join-Path $installPath 'VC\Tools\MSVC\*\bin\Hostx64\x64\cl.exe'
            $candidates = @(Get-ChildItem -Path $glob -File -ErrorAction SilentlyContinue)
            if ($candidates) {
                return Select-MsvcClPath -Candidates $candidates -CudaVersion $cudaVersion
            }
        }
    }

    $fallbackGlobs = @(
        'C:\Program Files\Microsoft Visual Studio\*\*\VC\Tools\MSVC\*\bin\Hostx64\x64\cl.exe',
        'C:\Program Files (x86)\Microsoft Visual Studio\*\*\VC\Tools\MSVC\*\bin\Hostx64\x64\cl.exe'
    )

    foreach ($g in $fallbackGlobs) {
        $candidates = @(Get-ChildItem -Path $g -File -ErrorAction SilentlyContinue)
        if ($candidates) {
            return Select-MsvcClPath -Candidates $candidates -CudaVersion $cudaVersion
        }
    }

    $fromPath = Get-Command cl.exe -ErrorAction SilentlyContinue
    if ($fromPath -and $fromPath.Source) {
        return $fromPath.Source
    }

    return $null
}

function Ensure-CandleCudaFeature {
    param([switch]$Release, [string[]]$Args)

    if (-not $Args -or $Args.Count -eq 0) {
        $base = @('build', '-p', 'zihuan_service', '--bin', 'zihuan_next', '--features', 'candle-cuda')
        if ($Release) { $base += '--release' }
        return $base
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

    if ($Release -and $result -notcontains '--release') {
        $result.Add('--release')
    }

    return $result.ToArray()
}

$clPath = Get-MsvcClPath
if (-not $clPath) {
    Write-Error 'MSVC cl.exe not found. Install Visual Studio Build Tools with the C++ workload.'
    exit 1
}

$env:NVCC_CCBIN = $clPath
$msvcRoot = (Get-Item -LiteralPath $clPath).Directory.Parent.Parent.Parent.FullName
$msvcInclude = Join-Path $msvcRoot 'include'
$env:VCToolsInstallDir = "$msvcRoot\"
$env:VCToolsVersion = Split-Path -Path $msvcRoot -Leaf
$env:VCINSTALLDIR = "$((Get-Item -LiteralPath $msvcRoot).Directory.Parent.Parent.Parent.FullName)\"
if (Test-Path $msvcInclude) {
    $env:INCLUDE = "$msvcInclude;$env:INCLUDE"
}
$rawArgs = @()
if ($args -and $args.Count -gt 0) {
    $rawArgs = @($args | Where-Object { $_ -ne '-Release' })
}

$finalArgs = Ensure-CandleCudaFeature -Release:$Release -Args $rawArgs

Write-Host "zihuan-next: NVCC_CCBIN=$clPath"
Write-Host "zihuan-next: MSVC include=$msvcInclude"
Write-Host "zihuan-next: cargo $($finalArgs -join ' ')"

& cargo @finalArgs
exit $LASTEXITCODE
