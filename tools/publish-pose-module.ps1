param(
    [switch]$PassThrough,
    [switch]$LiftDemo,
    [string]$StateDirectory,
    [switch]$NoBuild
)
$ErrorActionPreference = 'Stop'
if ($PassThrough -and $LiftDemo) { throw 'Select one module implementation' }
$repo = [System.IO.Path]::GetFullPath((Join-Path $PSScriptRoot '..'))
if (-not $IsWindows -and $PSVersionTable.PSEdition -ne 'Desktop') { throw 'Windows DLL publisher only' }
if (-not $StateDirectory) { $StateDirectory = Join-Path $repo 'benilla-config\Hotpatch' }
$root = [System.IO.Path]::GetFullPath($StateDirectory)
if (-not $NoBuild) {
    Push-Location $repo
    try {
        $buildArguments = @('build','-p','benilla-pose-module')
        if ($PassThrough) { $buildArguments += @('--features','passthrough') }
        if ($LiftDemo) { $buildArguments += @('--features','lift-demo') }
        & cargo @buildArguments
        if ($LASTEXITCODE -ne 0) { throw 'Pose module build failed; publication unchanged' }
    } finally { Pop-Location }
}
$binary = Join-Path $repo 'target\debug\benilla_pose_module.dll'
if (-not (Test-Path -LiteralPath $binary)) { throw "Missing module: $binary" }
$digest = (Get-FileHash -Algorithm SHA1 -LiteralPath $binary).Hash.ToLowerInvariant()
$name = "pose-$digest.dll"
$null = New-Item -ItemType Directory -Path $root -Force
$lockPath = Join-Path $root 'publish.lock'
$lock = $null
$deadline = [DateTime]::UtcNow.AddSeconds(10)
while (-not $lock) {
    try { $lock = [System.IO.File]::Open($lockPath, 'CreateNew', 'Write', 'None') }
    catch [System.IO.IOException] {
        if ([DateTime]::UtcNow -ge $deadline) { throw 'Publication lock is busy; nothing published' }
        Start-Sleep -Milliseconds 50
    }
}
try {
    $destination = Join-Path $root $name
    if (Test-Path -LiteralPath $destination) {
        if ((Get-FileHash -Algorithm SHA1 -LiteralPath $destination).Hash.ToLowerInvariant() -ne $digest) { throw 'Immutable publication has been modified' }
    } else { Copy-Item -LiteralPath $binary -Destination $destination }
    $sourceHint = 'prebuilt-source-unverified'
    if (-not $NoBuild) {
        $sourceHint = (& git -C $repo rev-parse --short=12 HEAD).Trim()
        & git -C $repo diff --quiet HEAD --
        if ($LASTEXITCODE -ne 0) { $sourceHint += '-dirty' }
        $untrackedModuleSources = @(& git -C $repo ls-files --others --exclude-standard -- crates/benilla-pose-module crates/benilla-hotpatch-abi)
        if ($untrackedModuleSources.Count -gt 0 -and $sourceHint -notlike '*-dirty') { $sourceHint += '-dirty' }
    }
    $revision = "$sourceHint-$digest-" + (Get-Date -Format 'yyyyMMddTHHmmssfff')
    $text = "revision = `"$revision`"`nlibrary = `"$name`"`nsha1 = `"$digest`"`n"
    $temporary = Join-Path $root 'ready.next'
    $ready = Join-Path $root 'ready.toml'
    [System.IO.File]::WriteAllText($temporary, $text, [System.Text.UTF8Encoding]::new($false))
    if (Test-Path -LiteralPath $ready) { [System.IO.File]::Replace($temporary, $ready, [NullString]::Value) }
    else { [System.IO.File]::Move($temporary, $ready) }
    Write-Output "Published pose ABI 1: $revision -> $root"
} finally {
    $lock.Dispose()
    Remove-Item -LiteralPath $lockPath -ErrorAction SilentlyContinue
}
