[CmdletBinding()]
param(
    [string]$DataPath = 'D:\WOW\world of warcraft 3.3.5a hd\data',
    [string]$AuthServer = '127.0.0.1:3724',
    [string]$WorldServer = '127.0.0.1:8095',
    [bool]$WithAddons = $true,
    [string]$ReleaseRoot = '',
    [switch]$UseReleaseSet,
    [string]$ConfigRoot = '',
    [int]$HealthCheckSeconds = 15,
    [switch]$SkipHealthCheck,
    [scriptblock]$HealthPredicate = $null
)
$ErrorActionPreference = 'Stop'
$repo = Split-Path -Parent $PSScriptRoot
$userRoot = if ([string]::IsNullOrWhiteSpace($ConfigRoot)) { $repo } else { [IO.Path]::GetFullPath($ConfigRoot) }
$releaseContext = $null

# The developer path is the default. A versioned release set is opt-in and additive.
$releaseRequested = $UseReleaseSet.IsPresent -or ![string]::IsNullOrWhiteSpace($ReleaseRoot)
$binary = Join-Path $repo 'target\debug\benilla.exe'
if ($releaseRequested) {
    $releaseModule = Join-Path $PSScriptRoot 'release-set.ps1'
    if (!(Test-Path -LiteralPath $releaseModule -PathType Leaf)) {
        throw "Release support is missing: $releaseModule"
    }
    . $releaseModule

    if ([string]::IsNullOrWhiteSpace($ReleaseRoot)) {
        throw 'Release set requested but -ReleaseRoot was not supplied. Refusing to fall back to the developer binary.'
    }
    $releaseRoot = [IO.Path]::GetFullPath($ReleaseRoot).TrimEnd([char[]]@('\', '/'))
    if (!(Test-Path -LiteralPath $releaseRoot -PathType Container)) {
        throw "Release root is missing: $releaseRoot"
    }

    $active = Get-ActiveReleaseSet -ReleaseRoot $releaseRoot
    if ($active.Status -eq 'Invalid') {
        throw "Active release selector is invalid: $($active.Error)"
    }
    if (!$active.IsPresent) {
        throw "No active release selector is present at '$($active.SelectorPath)'. Refusing to fall back to the developer binary."
    }

    $releaseTest = Test-ReleaseSet -ReleaseRoot $releaseRoot -VersionId $active.VersionId
    if (!$releaseTest.IsValid) {
        throw "Release set '$($active.VersionId)' failed validation ($($releaseTest.Code)): $($releaseTest.Message)"
    }
    if ([string]::IsNullOrWhiteSpace($active.SelectedManifestSha256) -or
        $active.SelectedManifestSha256 -ne $releaseTest.ManifestSha256) {
        throw "Release set '$($active.VersionId)' does not match the active selector manifest hash. Refusing to launch."
    }
    if ($active.PreviousVersionId) {
        $previous = Get-PreviousReleaseSet -ReleaseRoot $releaseRoot
        if ($previous.IsPresent) {
            $previousTest = Test-ReleaseSet -ReleaseRoot $releaseRoot -VersionId $previous.VersionId
            if (!$previousTest.IsValid) {
                Write-Warning "Previous known-good release '$($previous.VersionId)' is not currently valid: $($previousTest.Message)"
            }
        }
    }

    $binary = Join-Path $releaseTest.VersionPath ($releaseTest.ExecutablePath -replace '/', '\')
    $releaseContext = [pscustomobject][ordered]@{
        Root = $releaseRoot
        VersionId = $releaseTest.VersionId
        VersionPath = $releaseTest.VersionPath
        ManifestSha256 = $releaseTest.ManifestSha256
        PreviousVersionId = $active.PreviousVersionId
        PreviousManifestSha256 = $active.PreviousManifestSha256
        ExecutablePath = $releaseTest.ExecutablePath
        SelectorPath = $active.SelectorPath
    }
}

if (!(Test-Path -LiteralPath $binary)) {
    if ($releaseContext) {
        throw "Release executable is missing: $binary"
    }
    throw 'Client binary missing. Build once: cargo build -p benilla --features hotpatch'
}
if (Get-Process benilla,cargo,rustc -ErrorAction SilentlyContinue) {
    throw 'A client or build is already running. Close it or wait before launching.'
}
if (!(Test-Path -LiteralPath $DataPath)) { throw "Game data folder missing: $DataPath" }

# A normal player launch never inherits the probe's identity, commands or stop deadline.
foreach ($entry in @(Get-ChildItem Env: | Where-Object {
    $_.Name -like 'WOW_*' -or $_.Name -like 'WM_*'
})) {
    Remove-Item -LiteralPath "Env:$($entry.Name)"
}
$env:WOW_WRATH = '1'
$env:WOW_WRATH_LOGIN_UI = '1'
$env:WOW_WRATH_AUTH = $AuthServer
if ($WorldServer) { $env:WOW_WRATH_WORLD = $WorldServer }
$env:WOW_HOST = $AuthServer
$env:WOW_DATA = $DataPath
$env:WOW_STOCK_UI = '1'
$env:WOW_GM = 'off'
$env:WM_ASSET_MANIFEST = Join-Path $repo 'docs\profiles\wm-hd.mpqlist'
$env:WM_ASSET_PROFILE = 'wm-hd'

# Set addons path if present in game install
if ($WithAddons) {
    $gameRoot = Split-Path -Parent $DataPath
    $addonsFolder = Join-Path $gameRoot 'Interface\AddOns'
    if (Test-Path -LiteralPath $addonsFolder) {
        $env:WOW_ADDONS_PATH = $addonsFolder
    }
}

# This workstation's read-only shadow hides loose third-party addons. It shares installed
# archives through links; it is not a second copy of the game. Explicit addon launches use data.
$shadow = Join-Path $repo '.scratch\project-b\verification\2026-10-28-oc-alpha\shadow-data'
if (!$WithAddons -and (Test-Path -LiteralPath $shadow)) { $env:WOW_DATA = $shadow }

# Data, configuration and addons always come from user-owned locations. A release version
# directory is immutable and is never used as a working directory or data root.
$logBase = if ($releaseContext) { $userRoot } else { $repo }
$workingDirectory = if ($releaseContext) { $userRoot } else { $repo }
if (!(Test-Path -LiteralPath $workingDirectory -PathType Container)) {
    throw "User configuration root is missing: $workingDirectory"
}
$logs = Join-Path $logBase 'benilla-config\Logs\Launcher'
New-Item -ItemType Directory -Path $logs -Force | Out-Null
# Retain three launch generations, including the one about to start.
$previousRuns = @(Get-ChildItem -LiteralPath $logs -Directory | Sort-Object Name -Descending)
foreach ($old in @($previousRuns | Select-Object -Skip 2)) {
    $resolved = [IO.Path]::GetFullPath($old.FullName)
    if (!$resolved.StartsWith([IO.Path]::GetFullPath($logs) + '\', [StringComparison]::OrdinalIgnoreCase)) {
        throw 'Log retention path escaped launcher folder'
    }
    Remove-Item -LiteralPath $resolved -Recurse -Force
}
$run = Join-Path $logs (Get-Date -Format 'yyyyMMdd-HHmmss-fff')
New-Item -ItemType Directory -Path $run | Out-Null
Set-Location -LiteralPath $workingDirectory
$client = Start-Process -FilePath $binary -WorkingDirectory $workingDirectory -PassThru `
    -RedirectStandardOutput (Join-Path $run 'stdout.log') `
    -RedirectStandardError (Join-Path $run 'stderr.log')
$launchLines = @(
    "pid=$($client.Id)",
    "binary_sha256=$((Get-FileHash -LiteralPath $binary).Hash)"
)
if ($releaseContext) {
    $launchLines += @(
        "release_root=$($releaseContext.Root)",
        "release_version_id=$($releaseContext.VersionId)",
        "release_manifest_sha256=$($releaseContext.ManifestSha256)",
        "release_previous_version_id=$($releaseContext.PreviousVersionId)",
        "release_previous_manifest_sha256=$($releaseContext.PreviousManifestSha256)",
        "release_version_path=$($releaseContext.VersionPath)",
        "release_selector=$($releaseContext.SelectorPath)",
        "health_check_seconds=$HealthCheckSeconds"
    )
}
$launchLines | Set-Content -LiteralPath (Join-Path $run 'launch.txt')
$launchLog = Join-Path $run 'launch.txt'

$collector = Join-Path $PSScriptRoot 'collect-diagnostics.ps1'
if (Test-Path -LiteralPath $collector) {
    $watchCmd = "Wait-Process -Id $($client.Id) -ErrorAction SilentlyContinue; Start-Sleep -Milliseconds 500; & '$($collector.Replace("'", "''"))' -RunDir '$($run.Replace("'", "''"))' *> `$null"
    Start-Process -FilePath 'powershell.exe' -WindowStyle Hidden -ArgumentList @(
        '-NoProfile',
        '-ExecutionPolicy', 'Bypass',
        '-Command', $watchCmd
    ) | Out-Null
}

if ($releaseContext) {
    if ($SkipHealthCheck) {
        Add-Content -LiteralPath $launchLog -Value 'health_check=skipped'
    } elseif ($HealthCheckSeconds -le 0) {
        Add-Content -LiteralPath $launchLog -Value 'health_check=disabled'
    } else {
        $deadline = [DateTime]::UtcNow.AddSeconds($HealthCheckSeconds)
        $healthy = $false
        $lastPredicateError = $null
        while ([DateTime]::UtcNow -lt $deadline) {
            $healthProcess = Get-Process -Id $client.Id -ErrorAction SilentlyContinue
            if ($null -eq $healthProcess -or $healthProcess.HasExited) { break }
            if ($null -ne $HealthPredicate) {
                try {
                    $predicateResult = & $HealthPredicate ([pscustomobject][ordered]@{
                        Process = $healthProcess
                        RunDir = $run
                        VersionId = $releaseContext.VersionId
                        ManifestSha256 = $releaseContext.ManifestSha256
                    })
                    if ($predicateResult -eq $true) {
                        $healthy = $true
                        break
                    }
                } catch {
                    $lastPredicateError = $_.Exception.Message
                }
            }
            Start-Sleep -Milliseconds 250
        }

        $healthProcess = Get-Process -Id $client.Id -ErrorAction SilentlyContinue
        $processExited = ($null -eq $healthProcess -or $healthProcess.HasExited)
        $healthStatus = 'passed'
        if (!$healthy) {
            if ($processExited) {
                $healthStatus = 'process-exited'
            } elseif ($null -eq $HealthPredicate) {
                # No predicate: staying alive for the window is the startup success signal.
                $healthy = $true
                $healthStatus = 'running'
            } else {
                $healthStatus = 'predicate-failed'
            }
        }

        if (!$healthy) {
            if ($null -ne $healthProcess -and !$healthProcess.HasExited) {
                Stop-Process -Id $healthProcess.Id -Force -ErrorAction SilentlyContinue
                $healthStatus = "$healthStatus-stopped"
            }
            Add-Content -LiteralPath $launchLog -Value "health_check=$healthStatus"
            if ($lastPredicateError) {
                Add-Content -LiteralPath $launchLog -Value "health_predicate_error=$lastPredicateError"
            }

            try {
                $recovery = Recover-ReleaseSet -ReleaseRoot $releaseContext.Root -Reason "startup-health:$healthStatus"
                Add-Content -LiteralPath $launchLog -Value @(
                    'recovery_status=ok',
                    "recovery_already_applied=$($recovery.AlreadyRecovered)",
                    "recovered_from_version_id=$($recovery.FromVersionId)",
                    "recovered_to_version_id=$($recovery.ToVersionId)",
                    "recovered_manifest_sha256=$($recovery.ManifestSha256)"
                )
                Write-Host "Release '$($releaseContext.VersionId)' failed startup health check ($healthStatus). Rolled back to '$($recovery.ToVersionId)'."
            } catch {
                Add-Content -LiteralPath $launchLog -Value @(
                    'recovery_status=failed',
                    "recovery_error=$($_.Exception.Message)"
                )
                throw "Release '$($releaseContext.VersionId)' failed startup health check ($healthStatus) and rollback failed: $($_.Exception.Message)"
            }
            throw "Release '$($releaseContext.VersionId)' failed startup health check ($healthStatus). The selector was rolled back; no second client was started."
        }

        Add-Content -LiteralPath $launchLog -Value "health_check=$healthStatus"
    }
}

Write-Host "RustWrath started (PID $($client.Id)). Enter credentials in the game window."
Write-Host "Logs & auto-triage tickets will be saved to: $logs\latest-tickets.md when you close the game."
