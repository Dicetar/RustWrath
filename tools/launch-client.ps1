[CmdletBinding()]
param(
    [string]$DataPath = 'D:\WOW\world of warcraft 3.3.5a hd\data',
    [string]$AuthServer = '127.0.0.1:3724',
    [string]$WorldServer = '127.0.0.1:8095',
    [bool]$WithAddons = $true,
    [string]$StateHome
)
$ErrorActionPreference = 'Stop'
$repo = Split-Path -Parent $PSScriptRoot
$releaseBinary = Join-Path $repo 'bin\benilla.exe'
$isRelease = Test-Path -LiteralPath (Join-Path $repo 'release-manifest.json') -PathType Leaf
$binary = if ($isRelease) { $releaseBinary } else { Join-Path $repo 'target\debug\benilla.exe' }
if (!(Test-Path -LiteralPath $binary)) {
    throw "Client binary missing: $binary"
}
if (Get-Process benilla,cargo,rustc -ErrorAction SilentlyContinue) {
    throw 'A client or build is already running. Close it or wait before launching.'
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

$stateRoot = if ($env:BENILLA_HOME) { $env:BENILLA_HOME } else { Join-Path $repo 'benilla-config' }
$logs = Join-Path $stateRoot 'Logs\Launcher'
New-Item -ItemType Directory -Path $logs -Force | Out-Null
# Retain three launch generations, including the one about to start.
$previous = @(Get-ChildItem -LiteralPath $logs -Directory | Sort-Object Name -Descending)
foreach ($old in @($previous | Select-Object -Skip 2)) {
    $resolved = [IO.Path]::GetFullPath($old.FullName)
    if (!$resolved.StartsWith([IO.Path]::GetFullPath($logs) + '\', [StringComparison]::OrdinalIgnoreCase)) {
        throw 'Log retention path escaped launcher folder'
    }
    Remove-Item -LiteralPath $resolved -Recurse -Force
}
$run = Join-Path $logs (Get-Date -Format 'yyyyMMdd-HHmmss-fff')
New-Item -ItemType Directory -Path $run | Out-Null
Set-Location -LiteralPath $repo
$client = Start-Process -FilePath $binary -WorkingDirectory $repo -PassThru `
    -RedirectStandardOutput (Join-Path $run 'stdout.log') `
    -RedirectStandardError (Join-Path $run 'stderr.log')
@("pid=$($client.Id)", "binary_sha256=$((Get-FileHash -LiteralPath $binary).Hash)") |
    Set-Content -LiteralPath (Join-Path $run 'launch.txt')

$collector = Join-Path $PSScriptRoot 'collect-diagnostics.ps1'
if (Test-Path -LiteralPath $collector) {
    $watchCmd = "Wait-Process -Id $($client.Id) -ErrorAction SilentlyContinue; Start-Sleep -Milliseconds 500; & '$($collector.Replace("'", "''"))' -RunDir '$($run.Replace("'", "''"))' *> `$null"
    Start-Process -FilePath 'powershell.exe' -WindowStyle Hidden -ArgumentList @(
        '-NoProfile',
        '-ExecutionPolicy', 'Bypass',
        '-Command', $watchCmd
    ) | Out-Null
}
Write-Host "RustWrath started (PID $($client.Id)). Enter credentials in the game window."
Write-Host "Logs & auto-triage tickets will be saved to: $logs\latest-tickets.md when you close the game."
