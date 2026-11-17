[CmdletBinding()]
param(
    # Where the dx source is unpacked and patched. Outside the repo: it is a build tree, not source.
    [string]$WorkRoot = (Join-Path $env:LOCALAPPDATA 'RustWrath\dx-src')
)
# Installs the dioxus CLI (dx) that `launch-client.ps1 -LiveCode` drives, with the RustWrath patch
# in tools/live-code. Stock dx 0.7.10 watches only the tip crate's direct dependencies and fails a
# patch that reaches a workspace binary outside the client build, so it can patch `benilla` and
# `benilla-app` and nothing below them. ADR-0002, "Live code", has the details.
#
# Stop dx first: Windows cannot replace a running dx.exe.
$ErrorActionPreference = 'Stop'

# dx must match the subsecond the client links (Cargo.lock). The hash pins the crates.io archive
# the patch was written against.
$version = '0.7.10'
$crateSha256 = '1FCB6578719380A2094F115BBBDE340C59B65D2BF1987B8DDBBB9834DB6716E6'
$patch = Join-Path $PSScriptRoot "live-code\dx-${version}-workspace-crates.patch"
if (!(Test-Path -LiteralPath $patch -PathType Leaf)) {
    throw "The dx patch is missing: $patch"
}
if (Get-Process -Name dx -ErrorAction SilentlyContinue) {
    throw 'dx is running. Stop it (Ctrl+C in its window) and run this again.'
}
foreach ($tool in 'cargo', 'git') {
    if (!(Get-Command $tool -ErrorAction SilentlyContinue)) { throw "$tool is required and was not found in PATH." }
}

New-Item -ItemType Directory -Force -Path $WorkRoot | Out-Null
$archive = Join-Path $WorkRoot "dioxus-cli-$version.crate"

# Cargo's own download is reused when present; otherwise the archive comes from crates.io.
$cargoHome = if ($env:CARGO_HOME) { $env:CARGO_HOME } else { Join-Path $env:USERPROFILE '.cargo' }
$cached = Get-ChildItem -Path (Join-Path $cargoHome 'registry\cache') -Filter "dioxus-cli-$version.crate" -Recurse -ErrorAction SilentlyContinue |
    Select-Object -First 1
if ($cached) {
    Copy-Item -LiteralPath $cached.FullName -Destination $archive -Force
} else {
    Invoke-WebRequest -Uri "https://static.crates.io/crates/dioxus-cli/dioxus-cli-$version.crate" -OutFile $archive
}
$actual = (Get-FileHash -LiteralPath $archive -Algorithm SHA256).Hash
if ($actual -ne $crateSha256) {
    throw "dioxus-cli-$version.crate has SHA-256 $actual, expected $crateSha256. Not building it."
}

# A fresh tree every time, so a half-applied earlier run cannot leak in.
$source = Join-Path $WorkRoot "dioxus-cli-$version"
$resolvedRoot = [IO.Path]::GetFullPath($WorkRoot).TrimEnd('\') + '\'
if (Test-Path -LiteralPath $source) {
    $resolved = [IO.Path]::GetFullPath($source)
    if (!$resolved.StartsWith($resolvedRoot, [StringComparison]::OrdinalIgnoreCase)) {
        throw "Refusing to remove ${resolved}: it is outside $resolvedRoot"
    }
    Remove-Item -LiteralPath $resolved -Recurse -Force
}
# Windows' own tar: Git's GNU tar reads `C:` in a path as a remote host.
$tar = Join-Path $env:SystemRoot 'System32\tar.exe'
& $tar -xzf $archive -C $WorkRoot
if ($LASTEXITCODE -ne 0) { throw "tar could not unpack $archive" }

Push-Location -LiteralPath $source
try {
    & git apply --whitespace=nowarn $patch
    if ($LASTEXITCODE -ne 0) { throw "The patch did not apply to dioxus-cli $version." }
} finally {
    Pop-Location
}

# The published lockfile keeps dx's own dependencies as released. sccache is bypassed, as for any
# tool install (AGENTS.md).
Write-Host "Building patched dx $version. This takes several minutes."
$wrapper = $env:RUSTC_WRAPPER
try {
    $env:RUSTC_WRAPPER = ''
    & cargo install --path $source --locked --force
    if ($LASTEXITCODE -ne 0) { throw "cargo install failed for the patched dx (exit $LASTEXITCODE)." }
} finally {
    $env:RUSTC_WRAPPER = $wrapper
}

# The launcher reads this marker to tell the patched dx from a stock one of the same version.
$dx = Join-Path $cargoHome 'bin\dx.exe'
if (!(Test-Path -LiteralPath $dx -PathType Leaf)) { throw "cargo install finished but $dx is missing." }
$patchSha256 = (Get-FileHash -LiteralPath $patch -Algorithm SHA256).Hash
Set-Content -LiteralPath (Join-Path $cargoHome 'bin\dx.rustwrath-patch') -Value $patchSha256
Write-Host "Patched dx $version installed at $dx."
