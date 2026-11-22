[CmdletBinding()]
param(
    [string]$DataPath = 'D:\Projects\wow\server\world of warcraft 3.3.5a hd\data',
    [string]$AuthServer = '127.0.0.1:3724',
    [string]$WorldServer = '127.0.0.1:8095',
    [bool]$WithAddons = $true,
    [switch]$BuildLatest,
    [string]$StateHome,
    [string]$ReleaseRoot = '',
    [switch]$UseReleaseSet,
    [string]$ConfigRoot = '',
    [int]$HealthCheckSeconds = 15,
    [switch]$SkipHealthCheck,
    [scriptblock]$HealthPredicate = $null,
    [switch]$WithMulticlass,
    [string]$MulticlassGluePath = 'D:\Projects\wow\server\wm-project\client_extensions\multiclass\multiclass_glue.lua',
    [bool]$WithMovementHotpatch = $true,
    # Whole-client live code (ADR-0002): dx builds the dev client with `live_code` and patches
    # every saved source edit into it. Runs in this window; close the game or Ctrl+C to stop.
    [switch]$LiveCode
)
$ErrorActionPreference = 'Stop'
$repo = Split-Path -Parent $PSScriptRoot
$userRoot = if ([string]::IsNullOrWhiteSpace($ConfigRoot)) { $repo } else { [IO.Path]::GetFullPath($ConfigRoot) }
$releaseContext = $null

# The developer path is the default. A versioned release set is opt-in and additive.
$releaseRequested = $UseReleaseSet.IsPresent -or ![string]::IsNullOrWhiteSpace($ReleaseRoot)
# Every developer launch first drops build output nobody is using (idle agent-worktree targets,
# old incremental sessions, idle profiles), so builds cannot fill the disk. Never fatal.
$prune = Join-Path $PSScriptRoot 'prune-build-output.ps1'
if (!$releaseRequested -and (Test-Path -LiteralPath $prune)) {
    try { & $prune } catch { Write-Warning "build-output prune failed: $($_.Exception.Message)" }
}
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

$isRelease = Test-Path -LiteralPath (Join-Path $repo 'release-manifest.json') -PathType Leaf
if ($isRelease -and $releaseRequested) { throw 'Choose an assembled release or a release set, not both' }
if ($isRelease) { $binary = Join-Path $repo 'bin\benilla.exe' }
if ($LiveCode -and ($isRelease -or $releaseRequested)) {
    throw 'Live code patches a source build; it cannot run an assembled release or a release set.'
}
if ($LiveCode -and $BuildLatest) {
    Write-Host 'Live code builds through dx; -BuildLatest is skipped.'
}
# A client or build of this checkout: a client exe under it, or a compiler writing into its own
# `target`. Agents build in their own worktrees with their own targets; those do not block a launch.
function Test-OwnBuildOrClient([string]$Root) {
    $own = [IO.Path]::GetFullPath($Root).TrimEnd('\') + '\'
    $target = $own + 'target\'
    foreach ($p in Get-CimInstance Win32_Process -Filter "Name LIKE 'benilla%' OR Name = 'rustc.exe' OR Name = 'cargo.exe'") {
        if ($p.Name -like 'benilla*') {
            if ($p.ExecutablePath -and $p.ExecutablePath.StartsWith($own, [StringComparison]::OrdinalIgnoreCase)) { return $true }
        } elseif ($p.CommandLine -and $p.CommandLine.IndexOf($target, [StringComparison]::OrdinalIgnoreCase) -ge 0) {
            return $true
        } elseif ($p.CommandLine -and $p.CommandLine.IndexOf($own + 'Cargo.toml', [StringComparison]::OrdinalIgnoreCase) -ge 0) {
            return $true
        }
    }
    return $false
}
if ($BuildLatest -and !$LiveCode -and !$isRelease -and !$releaseRequested) {
    if (Test-OwnBuildOrClient $repo) {
        throw 'Close the running client or wait for the active build before rebuilding.'
    }
    Write-Host 'Building the current RustWrath source before launch...'
    # A launcher started from Explorer or an old shell can inherit a PATH without the user's Rust
    # entries. Re-read the registry PATH a new terminal would get, then try rustup's default home.
    $cargo = Get-Command cargo -CommandType Application -ErrorAction SilentlyContinue | Select-Object -First 1
    if (!$cargo) {
        $current = $env:Path -split ';'
        $fresh = foreach ($scope in 'Machine', 'User') {
            $value = [Environment]::GetEnvironmentVariable('Path', $scope)
            if ($value) { [Environment]::ExpandEnvironmentVariables($value) -split ';' }
        }
        $missing = @($fresh | Where-Object { $_ -and $current -notcontains $_ })
        if ($missing.Count) { $env:Path = (@($current) + $missing) -join ';' }
        $cargo = Get-Command cargo -CommandType Application -ErrorAction SilentlyContinue | Select-Object -First 1
    }
    $cargoHome = if ($env:CARGO_HOME) { $env:CARGO_HOME } else { Join-Path $env:USERPROFILE '.cargo' }
    if (!$cargo) {
        $cargoExe = Join-Path $cargoHome 'bin\cargo.exe'
        if (Test-Path -LiteralPath $cargoExe -PathType Leaf) {
            $env:Path = (Join-Path $cargoHome 'bin') + ';' + $env:Path
            $cargo = Get-Command $cargoExe -ErrorAction SilentlyContinue
        }
    }
    if (!$cargo) {
        throw "cargo was not found in PATH, in the registry PATH, or at $cargoHome\bin\cargo.exe.`nInstall Rust with rustup (https://rustup.rs), then run this launcher again."
    }
    $cache = Get-Command sccache -ErrorAction SilentlyContinue
    if (!$cache) {
        $cargoBin = Join-Path $env:USERPROFILE '.cargo\bin\sccache.exe'
        if (Test-Path -LiteralPath $cargoBin -PathType Leaf) {
            $cache = Get-Command $cargoBin -ErrorAction SilentlyContinue
        }
    }
    if (!$cache) {
        throw "sccache is required by .cargo/config.toml, but was not found in PATH or at $env:USERPROFILE\.cargo\bin\sccache.exe.`nInstall it by running:`n  `$env:RUSTC_WRAPPER=''; cargo install sccache --locked"
    }
    $previousWrapper = $env:RUSTC_WRAPPER
    $previousIncremental = $env:CARGO_INCREMENTAL
    try {
        $env:RUSTC_WRAPPER = $cache.Source
        $env:CARGO_INCREMENTAL = '0'
        & $cargo.Source build --manifest-path (Join-Path $repo 'Cargo.toml') --target-dir (Join-Path $repo 'target') -p benilla --features hotpatch
        if ($LASTEXITCODE -ne 0) {
            throw 'RustWrath build failed. See the build output above.'
        }
    } finally {
        $env:RUSTC_WRAPPER = $previousWrapper
        $env:CARGO_INCREMENTAL = $previousIncremental
    }
}
if (!$LiveCode -and !(Test-Path -LiteralPath $binary)) {
    if ($releaseContext) {
        throw "Release executable is missing: $binary"
    }
    throw 'Client binary missing. Build once: cargo build -p benilla --features hotpatch'
}
if (Test-OwnBuildOrClient $repo) {
    throw 'A client or build of this checkout is already running. Close it or wait before launching.'
}
if ($isRelease -and !$PSBoundParameters.ContainsKey('DataPath')) {
    # Release users choose their own existing game data; never assume the developer's path.
    $DataPath = Read-Host 'Path to your existing WoW 3.3.5a Data folder'
}
if (!(Test-Path -LiteralPath $DataPath -PathType Container)) { throw "Game data folder missing: $DataPath" }
$DataPath = (Resolve-Path -LiteralPath $DataPath).Path

if ($isRelease) {
    $manifest = Get-Content -LiteralPath (Join-Path $repo 'release-manifest.json') -Raw | ConvertFrom-Json
    if ($manifest.format -ne 1 -or !$manifest.files) { throw 'Unsupported or empty release manifest' }
    foreach ($entry in $manifest.files) {
        # Assembly records a fixed list. Reject traversal and alternate data streams before reading.
        if ($entry.path -notin @('bin/benilla.exe', 'tools/launch-client.ps1', 'tools/collect-diagnostics.ps1', 'docs/profiles/wm-hd.mpqlist', 'Launch RustWrath.cmd')) {
            throw "Unexpected release file: $($entry.path)"
        }
        $file = Join-Path $repo $entry.path
        if (!(Test-Path -LiteralPath $file -PathType Leaf) -or
            (Get-FileHash -LiteralPath $file -Algorithm SHA256).Hash -ne $entry.sha256) {
            throw "Release file missing or changed: $($entry.path)"
        }
    }
    if (@($manifest.files).Count -ne 5 -or @($manifest.files.path | Select-Object -Unique).Count -ne 5) {
        throw 'Release manifest does not enumerate the required files exactly once'
    }
    if (!$StateHome) {
        if (!$env:LOCALAPPDATA) { throw 'LOCALAPPDATA is unavailable; pass -StateHome explicitly' }
        $StateHome = Join-Path $env:LOCALAPPDATA 'RustWrath\benilla-config'
    }
}
if ($StateHome) {
    if (![IO.Path]::IsPathRooted($StateHome)) { throw 'StateHome must be an absolute path' }
    $statePath = [IO.Path]::GetFullPath($StateHome)
    $stateCompare = $statePath.TrimEnd('\', '/')
    $releasePath = [IO.Path]::GetFullPath($repo).TrimEnd('\', '/')
    if ($isRelease -and ($stateCompare.Equals($releasePath, [StringComparison]::OrdinalIgnoreCase) -or
        $stateCompare.StartsWith($releasePath + '\', [StringComparison]::OrdinalIgnoreCase))) {
        throw 'StateHome must be outside the release directory'
    }
    $env:BENILLA_HOME = $statePath
}
if (!(Test-Path -LiteralPath $DataPath)) { throw "Game data folder missing: $DataPath" }
if ($WithMulticlass) {
    if (!$WithAddons) { throw 'Multiclass requires -WithAddons $true' }
    $multiclassDir = Join-Path (Split-Path -Parent $DataPath) 'Interface\AddOns\WM_Multiclass'
    $multiclassToc = Join-Path $multiclassDir 'WM_Multiclass.toc'
    if (!(Test-Path -LiteralPath $multiclassToc -PathType Leaf)) {
        throw "Required addon missing: $multiclassToc"
    }
    if (!(Test-Path -LiteralPath $MulticlassGluePath -PathType Leaf)) {
        $bundledGlue = Join-Path $multiclassDir 'multiclass_glue.lua'
        if (Test-Path -LiteralPath $bundledGlue -PathType Leaf) {
            $MulticlassGluePath = $bundledGlue
        } else {
            throw "Multiclass Glue script missing: $MulticlassGluePath"
        }
    }
}

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
$env:WOW_HOTPATCH_MOVEMENT = if ($WithMovementHotpatch) { '1' } else { '0' }
$env:WM_ASSET_MANIFEST = Join-Path $repo 'docs\profiles\wm-hd.mpqlist'
$env:WM_ASSET_PROFILE = 'wm-hd'
if ($WithMulticlass) {
    $env:WOW_WRATH_MULTICLASS = '1'
    $env:WOW_MULTICLASS_GLUE = (Resolve-Path -LiteralPath $MulticlassGluePath).Path
}

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
$stateRoot = if ($env:BENILLA_HOME) { $env:BENILLA_HOME } else { Join-Path $repo 'benilla-config' }
if ($env:BENILLA_HOME) { $logs = Join-Path $stateRoot 'Logs\Launcher' }
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
if ($LiveCode) {
    # dx's build goes through Cargo, which the repo config points at sccache.
    $cache = Get-Command sccache -ErrorAction SilentlyContinue
    if (!$cache) {
        $cargoBin = Join-Path $env:USERPROFILE '.cargo\bin\sccache.exe'
        if (Test-Path -LiteralPath $cargoBin -PathType Leaf) { $cache = Get-Command $cargoBin -ErrorAction SilentlyContinue }
    }
    if (!$cache) {
        throw "sccache is required by .cargo/config.toml, but was not found in PATH or at $env:USERPROFILE\.cargo\bin\sccache.exe.`nInstall it by running:`n  `$env:RUSTC_WRAPPER=''; cargo install sccache --locked"
    }
    $env:RUSTC_WRAPPER = $cache.Source
    # dx patches only a client whose subsecond matches it exactly; the lock names the client's.
    $lock = Get-Content -LiteralPath (Join-Path $repo 'Cargo.lock') -Raw
    $subsecond = [regex]::Match($lock, 'name = "subsecond"\r?\nversion = "([^"]+)"')
    $wanted = if ($subsecond.Success) { $subsecond.Groups[1].Value } else { $null }
    $dx = Get-Command dx -CommandType Application -ErrorAction SilentlyContinue | Select-Object -First 1
    if (!$dx) {
        $dxBin = Join-Path $env:USERPROFILE '.cargo\bin\dx.exe'
        if (Test-Path -LiteralPath $dxBin -PathType Leaf) { $dx = Get-Command $dxBin -ErrorAction SilentlyContinue }
    }
    # The patched dx (tools/install-live-dx.ps1) exists for one version; stock dx patches only
    # `benilla` and `benilla-app`, so another version falls back to a stock install.
    $patchedVersion = '0.7.10'
    $dxPatch = Join-Path $PSScriptRoot "live-code\dx-${patchedVersion}-workspace-crates.patch"
    $install = if (!$wanted -or $wanted -eq $patchedVersion) {
        "& '$(Join-Path $PSScriptRoot 'install-live-dx.ps1')'"
    } else {
        "`$env:RUSTC_WRAPPER=''; cargo install dioxus-cli --version $wanted --locked"
    }
    if (!$dx) {
        throw "Live code needs the dioxus CLI (dx). Install it by running:`n  $install"
    }
    $dxVersion = [regex]::Match((& $dx.Source --version | Out-String), '\d+\.\d+\.\d+[^\s)]*').Value
    if ($wanted -and $dxVersion -ne $wanted) {
        throw "dx $dxVersion cannot patch a client built with subsecond $wanted. Install the matching CLI:`n  $install"
    }
    if (!$wanted) {
        Write-Warning 'Cargo.lock has no subsecond yet: the first live-code build adds it. If dx then refuses to patch, install the CLI the lock names and launch again.'
    }
    # install-live-dx.ps1 leaves the patch's hash beside dx.exe; stock dx has none.
    $dxPatched = 'stock'
    if ($dxVersion -eq $patchedVersion) {
        $marker = Join-Path (Split-Path -Parent $dx.Source) 'dx.rustwrath-patch'
        $expected = (Get-FileHash -LiteralPath $dxPatch -Algorithm SHA256).Hash
        if ((Test-Path -LiteralPath $marker -PathType Leaf) -and ((Get-Content -LiteralPath $marker -Raw).Trim() -eq $expected)) {
            $dxPatched = $expected
        } else {
            Write-Warning "This dx is stock or an older patch: edits outside benilla and benilla-app will not patch the running client. Stop here and run:`n  $install"
        }
    }
    @('mode=live_code', "dx=$($dx.Source)", "dx_version=$dxVersion", "dx_patch=$dxPatched", "subsecond=$wanted") |
        Set-Content -LiteralPath (Join-Path $run 'launch.txt')
    Write-Host 'Live code: dx builds the dev client and starts it here. Save a source file to patch the running client.'
    Write-Host 'A changed struct layout, static, or system/plugin registration still needs a restart (ADR-0002, "Live code").'
    # dx.log keeps dx's own trace (which files and crates each patch saw), unlike the TUI.
    & $dx.Source serve --hot-patch --package benilla --bin benilla --features 'hotpatch,live_code' `
        --log-to-file (Join-Path $run 'dx.log')
    exit $LASTEXITCODE
}
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
