$ErrorActionPreference = 'Stop'

# Self-contained test. It only writes below F:\AI\tmp\opencode and never touches a game
# install, benilla-config, SavedVariables or a running client. It does not launch anything.

$script:Passed = 0
$script:Failed = 0
$testRoot = Join-Path 'F:\AI\tmp\opencode' ('w-release-test-' + [Guid]::NewGuid().ToString('N'))
$modulePath = Join-Path $PSScriptRoot 'release-set.ps1'
$launchPath = Join-Path $PSScriptRoot 'launch-client.ps1'
$fixedStamp = '2026-01-02T03:04:05Z'

function Write-TestResult {
    param([string]$Name, [bool]$Ok, [string]$Detail = '')
    if ($Ok) {
        $script:Passed++
        Write-Host "PASS $Name"
    } else {
        $script:Failed++
        if ($Detail) {
            Write-Host "FAIL $Name -- $Detail"
        } else {
            Write-Host "FAIL $Name"
        }
    }
}

function Assert-True {
    param([bool]$Condition, [string]$Name, [string]$Detail = '')
    Write-TestResult -Name $Name -Ok $Condition -Detail $Detail
}

function Assert-Equal {
    param($Expected, $Actual, [string]$Name)
    $ok = [string]$Expected -eq [string]$Actual
    Write-TestResult -Name $Name -Ok $ok -Detail "expected '$Expected', got '$Actual'"
}

function Assert-Throws {
    param([scriptblock]$Action, [string]$Name)
    $threw = $false
    $message = ''
    try {
        & $Action | Out-Null
    } catch {
        $threw = $true
        $message = $_.Exception.Message
    }
    Write-TestResult -Name $Name -Ok $threw -Detail "expected a throw; message was '$message'"
    return $message
}

function New-TestTextFile {
    param([string]$Path, [string]$Text = 'test')
    $parent = Split-Path -Parent $Path
    if ($parent) { [void](New-Item -ItemType Directory -Path $parent -Force) }
    [IO.File]::WriteAllText($Path, $Text, (New-Object System.Text.UTF8Encoding($false)))
}

function New-TestBinaryFile {
    param([string]$Path, [byte[]]$Bytes = [byte[]](0..63))
    $parent = Split-Path -Parent $Path
    if ($parent) { [void](New-Item -ItemType Directory -Path $parent -Force) }
    [IO.File]::WriteAllBytes($Path, $Bytes)
}

function Assert-ParseOk {
    param([string]$Path, [string]$Name)
    $tokens = $null
    $errors = $null
    [void][System.Management.Automation.Language.Parser]::ParseFile($Path, [ref]$tokens, [ref]$errors)
    Assert-True ($errors.Count -eq 0) $Name (($errors | ForEach-Object { $_.Message }) -join '; ')
}

try {
    [void](New-Item -ItemType Directory -Path $testRoot -Force)
    . $modulePath

    Assert-ParseOk -Path $modulePath -Name 'release-set.ps1 parses'
    Assert-ParseOk -Path $launchPath -Name 'launch-client.ps1 parses'

    $candidate = Join-Path $testRoot 'candidate'
    New-TestBinaryFile -Path (Join-Path $candidate 'benilla.exe')
    New-TestTextFile -Path (Join-Path $candidate 'libs\foo.dll') -Text 'foo'
    New-TestTextFile -Path (Join-Path $candidate 'bin\helper.dll') -Text 'helper'
    New-TestTextFile -Path (Join-Path $candidate 'benilla-config\secret.txt') -Text 'must not ship'
    New-TestTextFile -Path (Join-Path $candidate 'SavedVariables\Account\SavedVariables.lua') -Text 'must not ship'
    New-TestTextFile -Path (Join-Path $candidate 'Logs\client.log') -Text 'must not ship'
    New-TestTextFile -Path (Join-Path $candidate 'Data\base.mpq') -Text 'must not ship'
    New-TestTextFile -Path (Join-Path $candidate 'credentials.txt') -Text 'must not ship'
    New-TestTextFile -Path (Join-Path $candidate '.probe-identity') -Text 'must not ship'

    # Manifest determinism and exclusions.
    $manifest1 = New-ReleaseManifest -VersionPath $candidate -VersionId 'v1' `
        -SourceRevision 'test-revision' -BuildIdentity 'test-build' `
        -RequiredDataProfileIds @('wm-hd', 'wm-hd', ' ') -CreatedAtUtc $fixedStamp
    $manifest2 = New-ReleaseManifest -VersionPath $candidate -VersionId 'v1' `
        -SourceRevision 'test-revision' -BuildIdentity 'test-build' `
        -RequiredDataProfileIds @('wm-hd', 'wm-hd', ' ') -CreatedAtUtc $fixedStamp
    Assert-Equal ($manifest1 | ConvertTo-Json -Depth 12) ($manifest2 | ConvertTo-Json -Depth 12) 'manifest is deterministic for the same inputs'
    Assert-Equal 'wm-hd' ($manifest1.requiredDataProfileIds -join ',') 'required data profile ids are normalized'
    Assert-Equal 'benilla.exe' $manifest1.executablePath 'manifest records the executable path'

    $paths = @($manifest1.files | ForEach-Object { $_.path })
    $sorted = $true
    for ($i = 1; $i -lt $paths.Count; $i++) {
        if ([StringComparer]::Ordinal.Compare([string]$paths[$i - 1], [string]$paths[$i]) -gt 0) {
            $sorted = $false
            break
        }
    }
    Assert-True $sorted 'manifest paths are sorted deterministically'
    Assert-True (($paths -contains 'benilla.exe') -and ($paths -contains 'libs/foo.dll') -and ($paths -contains 'bin/helper.dll')) 'manifest includes release files'
    $excluded = @('benilla-config/secret.txt', 'SavedVariables/Account/SavedVariables.lua', 'Logs/client.log', 'Data/base.mpq', 'credentials.txt', '.probe-identity')
    $leaked = @($excluded | Where-Object { $paths -contains $_ })
    Assert-True ($leaked.Count -eq 0) 'manifest excludes config, saved variables, logs, credentials and game archives' ($leaked -join ',')

    # Valid stage and activation.
    $releaseRoot = Join-Path $testRoot 'release-root'
    $stage1 = Stage-ReleaseSet -SourcePath $candidate -ReleaseRoot $releaseRoot -VersionId 'v1' `
        -SourceRevision 'test-revision' -BuildIdentity 'test-build' `
        -RequiredDataProfileIds @('wm-hd') -CreatedAtUtc $fixedStamp
    Assert-Equal 'Staged' $stage1.Status 'valid release stages'
    Assert-True (Test-Path -LiteralPath (Join-Path $releaseRoot 'versions\v1\release-manifest.json') -PathType Leaf) 'staged manifest exists'
    Assert-True (Test-Path -LiteralPath (Join-Path $releaseRoot 'versions\v1\.ready') -PathType Leaf) 'staged ready marker exists'
    Assert-True (!(Test-Path -LiteralPath (Join-Path $releaseRoot 'versions\v1\.incomplete'))) 'completed release has no incomplete marker'
    Assert-True (!(Test-Path -LiteralPath (Join-Path $releaseRoot 'versions\v1\benilla-config'))) 'staged version excludes benilla-config'
    Assert-True (!(Test-Path -LiteralPath (Join-Path $releaseRoot 'versions\v1\SavedVariables'))) 'staged version excludes SavedVariables'
    Assert-True (!(Test-Path -LiteralPath (Join-Path $releaseRoot 'versions\v1\Logs'))) 'staged version excludes Logs'
    Assert-True (!(Test-Path -LiteralPath (Join-Path $releaseRoot 'versions\v1\Data\base.mpq'))) 'staged version excludes game archives'

    $test1 = Test-ReleaseSet -ReleaseRoot $releaseRoot -VersionId 'v1'
    Assert-True $test1.IsValid 'valid release set passes Test-ReleaseSet'
    Assert-Equal 'Valid' $test1.Status 'valid release set has typed Valid status'
    Assert-Equal 'RustWrath.ReleaseSetTestResult' $test1.PSObject.TypeNames[0] 'Test-ReleaseSet returns a typed result'
    Assert-True ([long]$test1.FileCount -eq 3) 'Test-ReleaseSet counts verified files'
    Assert-Equal $stage1.ManifestSha256 $test1.ManifestSha256 'staged and tested manifest hashes agree'

    $activation1 = Activate-ReleaseSet -ReleaseRoot $releaseRoot -VersionId 'v1' -CreatedAtUtc $fixedStamp
    Assert-True $activation1.IsActivated 'valid release activates'
    Assert-Equal 'v1' $activation1.VersionId 'activation reports the active version'
    $active1 = Get-ActiveReleaseSet -ReleaseRoot $releaseRoot
    Assert-True $active1.IsPresent 'active selector resolves'
    Assert-Equal 'v1' $active1.VersionId 'active selector version is v1'
    Assert-True ($null -eq $active1.PreviousVersionId -or [string]::IsNullOrWhiteSpace([string]$active1.PreviousVersionId)) 'first activation has no previous version'

    # Hash mismatch refusal keeps the old selector.
    $stage2 = Stage-ReleaseSet -SourcePath $candidate -ReleaseRoot $releaseRoot -VersionId 'v2' `
        -SourceRevision 'test-revision' -BuildIdentity 'test-build' -CreatedAtUtc $fixedStamp
    $corruptBytes = New-Object 'byte[]' 64
    for ($i = 0; $i -lt $corruptBytes.Length; $i++) { $corruptBytes[$i] = 255 }
    New-TestBinaryFile -Path (Join-Path $releaseRoot 'versions\v2\benilla.exe') -Bytes $corruptBytes
    $test2 = Test-ReleaseSet -ReleaseRoot $releaseRoot -VersionId 'v2'
    Assert-True (!$test2.IsValid) 'corrupt release is rejected'
    Assert-Equal 'HashMismatch' $test2.Code 'corrupt release reports HashMismatch'
    [void](Assert-Throws -Action { Activate-ReleaseSet -ReleaseRoot $releaseRoot -VersionId 'v2' } -Name 'activating a corrupt release throws')
    $activeAfterBadHash = Get-ActiveReleaseSet -ReleaseRoot $releaseRoot
    Assert-Equal 'v1' $activeAfterBadHash.VersionId 'old selector survives a failed activation'
    $tempSelectors = @(Get-ChildItem -LiteralPath $releaseRoot -Filter 'active-release.json.*.tmp' -Force -ErrorAction SilentlyContinue)
    Assert-True ($tempSelectors.Count -eq 0) 'failed activation leaves no selector temp file'

    # Valid second activation records the previous known-good set.
    $stage3 = Stage-ReleaseSet -SourcePath $candidate -ReleaseRoot $releaseRoot -VersionId 'v3' `
        -SourceRevision 'test-revision' -BuildIdentity 'test-build' -CreatedAtUtc $fixedStamp
    $activation3 = Activate-ReleaseSet -ReleaseRoot $releaseRoot -VersionId 'v3' -CreatedAtUtc $fixedStamp
    Assert-Equal 'v1' $activation3.PreviousVersionId 'activation records the previous known-good version'
    $active3 = Get-ActiveReleaseSet -ReleaseRoot $releaseRoot
    Assert-Equal 'v3' $active3.VersionId 'second activation switches the selector'
    Assert-Equal 'v1' $active3.PreviousVersionId 'active state exposes the previous version'
    $previous3 = Get-PreviousReleaseSet -ReleaseRoot $releaseRoot
    Assert-True $previous3.IsPresent 'previous selector resolves'
    Assert-Equal 'v1' $previous3.VersionId 'previous selector is v1'

    # Missing file refusal.
    $stage4 = Stage-ReleaseSet -SourcePath $candidate -ReleaseRoot $releaseRoot -VersionId 'v4' `
        -SourceRevision 'test-revision' -BuildIdentity 'test-build' -CreatedAtUtc $fixedStamp
    Remove-Item -LiteralPath (Join-Path $releaseRoot 'versions\v4\libs\foo.dll') -Force
    $test4 = Test-ReleaseSet -ReleaseRoot $releaseRoot -VersionId 'v4'
    Assert-True (!$test4.IsValid) 'release with a missing file is rejected'
    Assert-Equal 'MissingFile' $test4.Code 'missing file reports MissingFile'
    [void](Assert-Throws -Action { Activate-ReleaseSet -ReleaseRoot $releaseRoot -VersionId 'v4' } -Name 'activating a release with a missing file throws')
    $activeAfterMissing = Get-ActiveReleaseSet -ReleaseRoot $releaseRoot
    Assert-Equal 'v3' $activeAfterMissing.VersionId 'old selector survives a missing-file activation'

    # Recovery to the previous known-good set, then idempotency.
    New-TestBinaryFile -Path (Join-Path $releaseRoot 'versions\v3\benilla.exe') -Bytes ([byte[]](7, 7, 7))
    $brokenActive = Test-ReleaseSet -ReleaseRoot $releaseRoot -VersionId 'v3'
    Assert-True (!$brokenActive.IsValid) 'broken active set fails validation'
    $recovery = Recover-ReleaseSet -ReleaseRoot $releaseRoot -Reason 'test-startup-failure' -CreatedAtUtc $fixedStamp
    Assert-True $recovery.Recovered 'recovery restores the previous set'
    Assert-Equal 'v3' $recovery.FromVersionId 'recovery reports the failed version'
    Assert-Equal 'v1' $recovery.ToVersionId 'recovery reports the restored version'
    $activeAfterRecovery = Get-ActiveReleaseSet -ReleaseRoot $releaseRoot
    Assert-Equal 'v1' $activeAfterRecovery.VersionId 'selector now points at v1 after recovery'
    Assert-Equal 'v3' $activeAfterRecovery.RecoveredFromVersionId 'recovered selector records the failed version'
    $recoveryAgain = Recover-ReleaseSet -ReleaseRoot $releaseRoot -Reason 'test-startup-failure' -CreatedAtUtc $fixedStamp
    Assert-True $recoveryAgain.AlreadyRecovered 'recovery is idempotent'
    Assert-Equal 'v1' $recoveryAgain.ToVersionId 'idempotent recovery stays on the known-good version'

    # Missing previous set is an explicit failure and leaves the selector untouched.
    $missingRoot = Join-Path $testRoot 'missing-previous'
    $missingStage = Stage-ReleaseSet -SourcePath $candidate -ReleaseRoot $missingRoot -VersionId 'w1' `
        -SourceRevision 'test-revision' -BuildIdentity 'test-build' -CreatedAtUtc $fixedStamp
    $missingSelector = [pscustomobject][ordered]@{
        schemaVersion = 1
        versionId = 'w1'
        manifestSha256 = $missingStage.ManifestSha256
        activatedAtUtc = $fixedStamp
        previousVersionId = 'missing-release'
        previousManifestSha256 = $null
        activationReason = 'test'
    }
    $missingSelectorPath = Join-Path $missingRoot 'active-release.json'
    Write-RwJsonAtomic -Path $missingSelectorPath -Value $missingSelector
    $selectorHashBefore = (Get-FileHash -LiteralPath $missingSelectorPath -Algorithm SHA256).Hash
    $missingMessage = Assert-Throws -Action { Recover-ReleaseSet -ReleaseRoot $missingRoot } -Name 'recovery with a missing previous set throws'
    Assert-True ($missingMessage -match 'previous') 'missing previous failure is explicit' $missingMessage
    $selectorHashAfter = (Get-FileHash -LiteralPath $missingSelectorPath -Algorithm SHA256).Hash
    Assert-Equal $selectorHashBefore $selectorHashAfter 'missing previous failure leaves the selector untouched'
    $missingActive = Get-ActiveReleaseSet -ReleaseRoot $missingRoot
    Assert-Equal 'w1' $missingActive.VersionId 'missing previous failure keeps the current version'

    # Path-escape refusal in a hand-written manifest.
    $escapeRoot = Join-Path $testRoot 'escape-root'
    $escapeVersion = Join-Path $escapeRoot 'versions\evil'
    New-TestBinaryFile -Path (Join-Path $escapeVersion 'benilla.exe')
    $escapeExeHash = (Get-FileHash -LiteralPath (Join-Path $escapeVersion 'benilla.exe') -Algorithm SHA256).Hash
    $escapeManifest = [pscustomobject][ordered]@{
        schemaVersion = 1
        hashAlgorithm = 'SHA256'
        versionId = 'evil'
        createdAtUtc = $fixedStamp
        sourceRevision = 'test'
        buildIdentity = 'test'
        executablePath = 'benilla.exe'
        requiredDataProfileIds = @()
        integrityNote = 'local test only'
        files = @(
            [pscustomobject][ordered]@{ path = 'benilla.exe'; size = 64; sha256 = $escapeExeHash },
            [pscustomobject][ordered]@{ path = '../escaped.txt'; size = 1; sha256 = ('A' * 64) }
        )
    }
    $escapeManifestPath = Join-Path $escapeVersion 'release-manifest.json'
    $escapeWritten = Write-ReleaseManifest -Manifest $escapeManifest -Path $escapeManifestPath
    Write-RwReadyMarker -Path (Join-Path $escapeVersion '.ready') -VersionId 'evil' -ManifestSha256 $escapeWritten.Sha256 -CreatedAtUtc $fixedStamp
    $escapeTest = Test-ReleaseSet -ReleaseRoot $escapeRoot -VersionId 'evil'
    Assert-True (!$escapeTest.IsValid) 'manifest path escape is rejected'
    Assert-Equal 'PathEscape' $escapeTest.Code 'path escape reports PathEscape'

    # Disk-space refusal before copying.
    $diskRoot = Join-Path $testRoot 'disk-root'
    $diskMessage = Assert-Throws -Action {
        Stage-ReleaseSet -SourcePath $candidate -ReleaseRoot $diskRoot -VersionId 'vdisk' `
            -SourceRevision 'test-revision' -BuildIdentity 'test-build' -CreatedAtUtc $fixedStamp `
            -MinimumFreeBytes ([long]::MaxValue)
    } -Name 'staging with an impossible free-space requirement throws'
    Assert-True ($diskMessage -match 'Insufficient disk space') 'disk-space refusal names the problem' $diskMessage
    Assert-True (!(Test-Path -LiteralPath (Join-Path $diskRoot 'versions\vdisk'))) 'disk-space refusal leaves no release version'

    # Unknown schema version refusal.
    $schemaRoot = Join-Path $testRoot 'schema-root'
    $schemaStage = Stage-ReleaseSet -SourcePath $candidate -ReleaseRoot $schemaRoot -VersionId 's1' `
        -SourceRevision 'test-revision' -BuildIdentity 'test-build' -CreatedAtUtc $fixedStamp
    $schemaManifestPath = Join-Path $schemaRoot 'versions\s1\release-manifest.json'
    $schemaManifest = [IO.File]::ReadAllText($schemaManifestPath) | ConvertFrom-Json
    $schemaManifest.schemaVersion = 2
    $schemaWritten = Write-ReleaseManifest -Manifest $schemaManifest -Path $schemaManifestPath
    Write-RwReadyMarker -Path (Join-Path $schemaRoot 'versions\s1\.ready') -VersionId 's1' -ManifestSha256 $schemaWritten.Sha256 -CreatedAtUtc $fixedStamp
    $schemaTest = Test-ReleaseSet -ReleaseRoot $schemaRoot -VersionId 's1'
    Assert-True (!$schemaTest.IsValid) 'unknown manifest schema version is rejected'
    Assert-Equal 'UnsupportedSchemaVersion' $schemaTest.Code 'unknown schema reports UnsupportedSchemaVersion'
}
finally {
    if (Test-Path -LiteralPath $testRoot) {
        Remove-Item -LiteralPath $testRoot -Recurse -Force -ErrorAction SilentlyContinue
    }
}

Write-Host "Release-set tests: $script:Passed passed, $script:Failed failed."
if ($script:Failed -gt 0) {
    throw "$script:Failed release-set test assertion(s) failed."
}
