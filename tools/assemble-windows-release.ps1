<# Assemble an offline, version-specific candidate from an already built binary
   (or build the release binary first under sccache when -BuildRelease is supplied).
   This script does not install, activate, sign, or distribute a release. #>
[CmdletBinding()]
param(
    [Parameter(Mandatory = $false)][string]$Binary = '',
    [Parameter(Mandatory = $true)][string]$Destination,
    [Parameter(Mandatory = $true)][string]$Version,
    [switch]$BuildRelease
)
Set-StrictMode -Version Latest
$ErrorActionPreference = 'Stop'
$sourceRoot = Split-Path -Parent $PSScriptRoot
if ($Version -notmatch '^[A-Za-z0-9][A-Za-z0-9._-]{0,79}$') { throw 'Invalid version label' }
if (![IO.Path]::IsPathRooted($Destination)) { throw 'Destination must be an absolute path' }
if ($BuildRelease) {
    if (Get-Process benilla,cargo,rustc -ErrorAction SilentlyContinue) {
        throw 'Close the running client or wait for the active build before building a release candidate.'
    }
    $cache = Get-Command sccache -ErrorAction Stop
    $previousWrapper = $env:RUSTC_WRAPPER
    $previousIncremental = $env:CARGO_INCREMENTAL
    try {
        $env:RUSTC_WRAPPER = $cache.Source
        $env:CARGO_INCREMENTAL = '0'
        & cargo build --release --manifest-path (Join-Path $sourceRoot 'Cargo.toml') --target-dir (Join-Path $sourceRoot 'target') -p benilla --features hotpatch
        if ($LASTEXITCODE -ne 0) {
            throw 'RustWrath release build failed.'
        }
    } finally {
        $env:RUSTC_WRAPPER = $previousWrapper
        $env:CARGO_INCREMENTAL = $previousIncremental
    }
    if ([string]::IsNullOrWhiteSpace($Binary)) {
        $Binary = Join-Path $sourceRoot 'target\release\benilla.exe'
    }
}
if ([string]::IsNullOrWhiteSpace($Binary)) { throw 'Supply -Binary or -BuildRelease' }
if (!(Test-Path -LiteralPath $Binary -PathType Leaf)) { throw "Binary missing: $Binary" }
$destinationPath = [IO.Path]::GetFullPath($Destination)
if (Test-Path -LiteralPath $destinationPath) { throw "Destination already exists: $destinationPath" }
$parent = Split-Path -Parent $destinationPath
if (!(Test-Path -LiteralPath $parent -PathType Container)) { throw "Destination parent missing: $parent" }
$binaryPath = (Resolve-Path -LiteralPath $Binary).Path
$sources = @(
    @{ path = 'bin/benilla.exe'; source = $binaryPath },
    @{ path = 'tools/launch-client.ps1'; source = (Join-Path $sourceRoot 'tools\launch-client.ps1') },
    @{ path = 'tools/collect-diagnostics.ps1'; source = (Join-Path $sourceRoot 'tools\collect-diagnostics.ps1') },
    @{ path = 'docs/profiles/wm-hd.mpqlist'; source = (Join-Path $sourceRoot 'docs\profiles\wm-hd.mpqlist') },
    @{ path = 'Launch RustWrath.cmd'; source = (Join-Path $sourceRoot 'Launch RustWrath.cmd') }
)
foreach ($entry in $sources) {
    if (!(Test-Path -LiteralPath $entry.source -PathType Leaf)) { throw "Release input missing: $($entry.source)" }
}
# Destination is newly created and never removed on failure. An incomplete candidate has no
# manifest and must be inspected/discarded by its owner; an existing candidate is never replaced.
New-Item -ItemType Directory -Path $destinationPath -ErrorAction Stop | Out-Null
$files = @()
foreach ($entry in $sources) {
    $target = Join-Path $destinationPath $entry.path
    $directory = Split-Path -Parent $target
    if (!(Test-Path -LiteralPath $directory)) { New-Item -ItemType Directory -Path $directory | Out-Null }
    Copy-Item -LiteralPath $entry.source -Destination $target -ErrorAction Stop
    $sourceHash = (Get-FileHash -LiteralPath $entry.source -Algorithm SHA256).Hash
    $targetHash = (Get-FileHash -LiteralPath $target -Algorithm SHA256).Hash
    if ($sourceHash -ne $targetHash) { throw "Copy verification failed: $($entry.path)" }
    $files += [ordered]@{ path = $entry.path; sha256 = $targetHash }
}
$manifest = [ordered]@{ format = 1; version = $Version; files = $files }
$manifest | ConvertTo-Json -Depth 4 | Set-Content -LiteralPath (Join-Path $destinationPath 'release-manifest.json') -Encoding UTF8
Write-Host "Assembled candidate $Version at $destinationPath"
Write-Host 'The manifest detects accidental changes; it is not a signature or trusted update channel.'
