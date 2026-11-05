<#
.SYNOPSIS
    Local, immutable Windows release sets for RustWrath.

.DESCRIPTION
    This file is a dot-sourceable function library. It does not launch a client and has no
    automatic invocation. Import it with:

        . .\tools\release-set.ps1

    Layout:

        <ReleaseRoot>\versions\<versionId>\release-manifest.json
        <ReleaseRoot>\versions\<versionId>\.ready
        <ReleaseRoot>\active-release.json

    SHA-256 values in a manifest are local integrity checks only. They detect accidental
    corruption or an incomplete copy. They are not signatures, not authentication, and are
    not protected against an attacker who can rewrite both the files and the manifest.

    Manifest schema version 1 is the only accepted schema. The only accepted hash algorithm
    is SHA256. Unknown schema versions and algorithms are rejected, never guessed.
#>

$script:RustWrathReleaseSchemaVersion = 1
$script:RustWrathReleaseHashAlgorithm = 'SHA256'
$script:RustWrathReleaseManifestName = 'release-manifest.json'
$script:RustWrathReleaseReadyName = '.ready'
$script:RustWrathReleaseIncompleteName = '.incomplete'
$script:RustWrathReleaseSelectorName = 'active-release.json'
$script:RustWrathReleaseIntegrityNote = 'SHA-256 values provide local integrity checking only; they are not signatures or authentication.'

function Assert-RwReleaseVersionId {
    param([Parameter(Mandatory)][string]$VersionId)

    if ($VersionId -notmatch '^[A-Za-z0-9][A-Za-z0-9._+-]{0,127}$') {
        throw "Invalid release version id '$VersionId'. Use 1-128 characters from A-Z, a-z, 0-9, dot, underscore, plus or hyphen; it must start with a letter or digit."
    }
}

function ConvertTo-RwRelativePath {
    param([Parameter(Mandatory)][string]$Path)

    $value = $Path.Trim() -replace '\\', '/'
    while ($value.StartsWith('./', [StringComparison]::Ordinal)) {
        $value = $value.Substring(2)
    }
    return $value
}

function Test-RwSafeRelativePath {
    param([Parameter(Mandatory)][string]$Path)

    $value = ConvertTo-RwRelativePath $Path
    if ([string]::IsNullOrWhiteSpace($value)) { return $false }
    if ($value.StartsWith('/')) { return $false }
    if ($value -match '^[A-Za-z]:') { return $false }
    if ($value.IndexOf(':') -ge 0) { return $false }
    if ($value.IndexOfAny([char[]]@([char]0, [char]10, [char]13)) -ge 0) { return $false }

    $segments = $value -split '/'
    foreach ($segment in $segments) {
        if ([string]::IsNullOrEmpty($segment)) { return $false }
        if ($segment -eq '.' -or $segment -eq '..') { return $false }
    }
    return $true
}

function Test-RwPathWithinRoot {
    param(
        [Parameter(Mandatory)][string]$Root,
        [Parameter(Mandatory)][string]$Candidate
    )

    $rootFull = [IO.Path]::GetFullPath($Root).TrimEnd([char[]]@('\', '/'))
    $candidateFull = [IO.Path]::GetFullPath($Candidate)
    if ($candidateFull.Equals($rootFull, [StringComparison]::OrdinalIgnoreCase)) { return $true }
    return $candidateFull.StartsWith($rootFull + [IO.Path]::DirectorySeparatorChar, [StringComparison]::OrdinalIgnoreCase)
}

function Join-RwPathWithinRoot {
    param(
        [Parameter(Mandatory)][string]$Root,
        [Parameter(Mandatory)][string]$RelativePath
    )

    if (!(Test-RwSafeRelativePath $RelativePath)) {
        throw "Unsafe relative path '$RelativePath'."
    }
    $candidate = Join-Path $Root ($RelativePath -replace '/', '\')
    if (!(Test-RwPathWithinRoot $Root $candidate)) {
        throw "Path '$RelativePath' escapes root '$Root'."
    }
    return [IO.Path]::GetFullPath($candidate)
}

function Test-RwReparsePointInPath {
    param(
        [Parameter(Mandatory)][string]$Root,
        [Parameter(Mandatory)][string]$Path
    )

    $rootFull = [IO.Path]::GetFullPath($Root).TrimEnd([char[]]@('\', '/'))
    $pathFull = [IO.Path]::GetFullPath($Path)
    if (!(Test-RwPathWithinRoot $rootFull $pathFull)) { return $true }

    $relative = $pathFull.Substring($rootFull.Length).TrimStart([char[]]@('\', '/'))
    $current = $rootFull
    foreach ($segment in ($relative -split '[\\/]')) {
        if ([string]::IsNullOrEmpty($segment)) { continue }
        $current = Join-Path $current $segment
        $item = Get-Item -LiteralPath $current -Force -ErrorAction SilentlyContinue
        if ($null -eq $item) { return $false }
        if (($item.Attributes -band [IO.FileAttributes]::ReparsePoint) -ne 0) { return $true }
    }
    return $false
}

function Get-RwSha256 {
    param([Parameter(Mandatory)][string]$Path)

    $hash = Get-FileHash -LiteralPath $Path -Algorithm SHA256 -ErrorAction Stop
    return ([string]$hash.Hash).ToUpperInvariant()
}

function Get-RwAvailableFreeBytes {
    param([Parameter(Mandatory)][string]$Path)

    $full = [IO.Path]::GetFullPath($Path)
    $driveRoot = [IO.Path]::GetPathRoot($full)
    if ([string]::IsNullOrWhiteSpace($driveRoot)) {
        throw "Cannot determine a drive for '$full'."
    }

    $drive = New-Object System.IO.DriveInfo($driveRoot)
    if (!$drive.IsReady) {
        throw "Drive '$driveRoot' is not ready; cannot check free space."
    }
    return [long]$drive.AvailableFreeSpace
}

function Assert-RwReleaseRootLocation {
    param([Parameter(Mandatory)][string]$Root)

    $blocked = @('benilla-config', 'SavedVariables', 'WTF')
    foreach ($segment in ($Root -split '[\\/]')) {
        if ($blocked -contains $segment) {
            throw "Release root '$Root' is inside a user configuration or saved-variable directory. Choose an immutable release location outside '$($segment)'."
        }
    }
}

function Get-RwUtcStamp {
    param([string]$Value = '')

    if ([string]::IsNullOrWhiteSpace($Value)) {
        return (Get-Date).ToUniversalTime().ToString('yyyy-MM-ddTHH:mm:ssZ')
    }

    $parsed = [DateTimeOffset]::MinValue
    $ok = [DateTimeOffset]::TryParse(
        $Value,
        [Globalization.CultureInfo]::InvariantCulture,
        [Globalization.DateTimeStyles]::AssumeUniversal,
        [ref]$parsed)
    if (!$ok) { throw "Invalid UTC timestamp '$Value'." }
    return $parsed.ToUniversalTime().ToString('yyyy-MM-ddTHH:mm:ssZ')
}

function Test-RwExcludedFileName {
    param(
        [Parameter(Mandatory)][string]$Name,
        [string[]]$ExtraName = @()
    )

    $fixed = @(
        $script:RustWrathReleaseManifestName,
        $script:RustWrathReleaseReadyName,
        $script:RustWrathReleaseIncompleteName,
        '.probe-identity',
        '.env'
    )
    foreach ($candidate in @($fixed + $ExtraName)) {
        if ($Name -ieq $candidate) { return $true }
    }

    $patterns = @(
        '*.mpq',
        '*.mpqi',
        '.env.*',
        'credentials',
        'credentials.*',
        '*.credentials',
        '*.credential',
        '*.secret',
        '*.token',
        '*.pem',
        '*.key',
        '*.pfx',
        '*.p12',
        '*.kdbx',
        'id_rsa',
        'id_rsa.*'
    )
    foreach ($pattern in $patterns) {
        if ($Name -like $pattern) { return $true }
    }
    return $false
}

function Get-RwCandidateFiles {
    param(
        [Parameter(Mandatory)][string]$Root,
        [string[]]$ExcludeName = @()
    )

    $rootFull = [IO.Path]::GetFullPath($Root).TrimEnd([char[]]@('\', '/'))
    $excludedDirectories = @(
        'benilla-config',
        'SavedVariables',
        'Logs',
        'WTF',
        'Cache',
        'Errors',
        '.git',
        '.svn',
        '.hg'
    )

    $results = New-Object 'System.Collections.Generic.List[object]'
    $seenPaths = New-Object 'System.Collections.Generic.HashSet[string]' ([StringComparer]::OrdinalIgnoreCase)
    $stack = New-Object System.Collections.Stack
    $stack.Push($rootFull)

    while ($stack.Count -gt 0) {
        $directory = [string]$stack.Pop()
        $children = @(Get-ChildItem -LiteralPath $directory -Force -ErrorAction Stop)
        foreach ($child in $children) {
            if (($child.Attributes -band [IO.FileAttributes]::ReparsePoint) -ne 0) {
                throw "Release candidate contains a symlink or reparse point: $($child.FullName)"
            }

            if ($child.PSIsContainer) {
                if ($excludedDirectories -contains $child.Name) { continue }
                $stack.Push($child.FullName)
                continue
            }

            if (Test-RwExcludedFileName -Name $child.Name -ExtraName $ExcludeName) { continue }

            if (!(Test-RwPathWithinRoot $rootFull $child.FullName)) {
                throw "Release candidate file escaped its source root: $($child.FullName)"
            }

            $relative = $child.FullName.Substring($rootFull.Length).TrimStart([char[]]@('\', '/')) -replace '\\', '/'
            if (!(Test-RwSafeRelativePath $relative)) {
                throw "Release candidate contains an unsafe relative path: $relative"
            }
            if (!($seenPaths.Add($relative))) {
                throw "Release candidate contains duplicate path '$relative' (Windows paths are case-insensitive)."
            }

            $entry = [pscustomobject][ordered]@{
                path = $relative
                size = [long]$child.Length
                sha256 = (Get-RwSha256 -Path $child.FullName)
            }
            [void]$results.Add($entry)
        }
    }

    $array = $results.ToArray()
    [Array]::Sort($array, [System.Comparison[object]]{
        param($left, $right)
        return [StringComparer]::Ordinal.Compare([string]$left.path, [string]$right.path)
    })
    return $array
}

function New-ReleaseManifest {
    [CmdletBinding()]
    param(
        [Parameter(Mandatory)][Alias('SourcePath')][string]$VersionPath,
        [Parameter(Mandatory)][string]$VersionId,
        [string]$SourceRevision = 'unrecorded',
        [string]$BuildIdentity = 'local',
        [string[]]$RequiredDataProfileIds = @(),
        [string]$CreatedAtUtc = '',
        [string]$ExecutablePath = 'benilla.exe',
        [string[]]$ExcludeName = @()
    )

    Assert-RwReleaseVersionId -VersionId $VersionId
    if (!(Test-Path -LiteralPath $VersionPath -PathType Container)) {
        throw "Release candidate directory is missing: $VersionPath"
    }
    if ([string]::IsNullOrWhiteSpace($SourceRevision)) { throw 'SourceRevision must not be empty.' }
    if ([string]::IsNullOrWhiteSpace($BuildIdentity)) { throw 'BuildIdentity must not be empty.' }

    $stamp = Get-RwUtcStamp -Value $CreatedAtUtc
    $files = @(Get-RwCandidateFiles -Root $VersionPath -ExcludeName $ExcludeName)
    if ($files.Count -eq 0) {
        throw "Release candidate '$VersionPath' contains no eligible files after exclusions."
    }

    $executableRelative = ConvertTo-RwRelativePath $ExecutablePath
    if (!(Test-RwSafeRelativePath $executableRelative)) {
        throw "Executable path is not a safe relative path: $ExecutablePath"
    }
    $executableEntries = @($files | Where-Object { $_.path -ieq $executableRelative })
    if ($executableEntries.Count -eq 0) {
        throw "Executable '$executableRelative' is not present in release candidate '$VersionPath'."
    }
    if ($executableEntries.Count -gt 1) {
        throw "Executable path '$executableRelative' matches more than one file."
    }

    $profiles = New-Object 'System.Collections.Generic.List[string]'
    $seenProfiles = New-Object 'System.Collections.Generic.HashSet[string]' ([StringComparer]::OrdinalIgnoreCase)
    foreach ($profile in @($RequiredDataProfileIds)) {
        $value = ([string]$profile).Trim()
        if (![string]::IsNullOrWhiteSpace($value) -and $seenProfiles.Add($value)) {
            [void]$profiles.Add($value)
        }
    }
    $profileArray = $profiles.ToArray()
    [Array]::Sort($profileArray, [StringComparer]::Ordinal)

    return [pscustomobject][ordered]@{
        schemaVersion = [int]$script:RustWrathReleaseSchemaVersion
        hashAlgorithm = [string]$script:RustWrathReleaseHashAlgorithm
        versionId = $VersionId
        createdAtUtc = $stamp
        sourceRevision = $SourceRevision
        buildIdentity = $BuildIdentity
        executablePath = $executableRelative
        requiredDataProfileIds = @($profileArray)
        integrityNote = [string]$script:RustWrathReleaseIntegrityNote
        files = @($files)
    }
}

function Write-ReleaseManifest {
    [CmdletBinding()]
    param(
        [Parameter(Mandatory)]$Manifest,
        [Parameter(Mandatory)][string]$Path
    )

    $fullPath = [IO.Path]::GetFullPath($Path)
    $parent = Split-Path -Parent $fullPath
    if (![string]::IsNullOrWhiteSpace($parent) -and !(Test-Path -LiteralPath $parent)) {
        [void](New-Item -ItemType Directory -Path $parent -Force)
    }

    $json = ($Manifest | ConvertTo-Json -Depth 12) + [Environment]::NewLine
    $utf8 = New-Object System.Text.UTF8Encoding($false)
    [IO.File]::WriteAllText($fullPath, $json, $utf8)

    return [pscustomobject][ordered]@{
        Path = $fullPath
        Sha256 = (Get-RwSha256 -Path $fullPath)
        Bytes = [long](Get-Item -LiteralPath $fullPath -Force).Length
    }
}

function Write-RwReadyMarker {
    param(
        [Parameter(Mandatory)][string]$Path,
        [Parameter(Mandatory)][string]$VersionId,
        [Parameter(Mandatory)][string]$ManifestSha256,
        [Parameter(Mandatory)][string]$CreatedAtUtc
    )

    $marker = [pscustomobject][ordered]@{
        schemaVersion = [int]$script:RustWrathReleaseSchemaVersion
        versionId = $VersionId
        manifestFile = [string]$script:RustWrathReleaseManifestName
        manifestSha256 = $ManifestSha256
        createdAtUtc = $CreatedAtUtc
        integrityNote = [string]$script:RustWrathReleaseIntegrityNote
    }
    $json = ($marker | ConvertTo-Json -Depth 6) + [Environment]::NewLine
    $utf8 = New-Object System.Text.UTF8Encoding($false)
    [IO.File]::WriteAllText([IO.Path]::GetFullPath($Path), $json, $utf8)
}

function Write-RwJsonAtomic {
    param(
        [Parameter(Mandatory)][string]$Path,
        [Parameter(Mandatory)]$Value
    )

    $fullPath = [IO.Path]::GetFullPath($Path)
    $parent = Split-Path -Parent $fullPath
    if ([string]::IsNullOrWhiteSpace($parent)) { $parent = (Get-Location).Path }
    if (!(Test-Path -LiteralPath $parent)) {
        [void](New-Item -ItemType Directory -Path $parent -Force)
    }

    $json = ($Value | ConvertTo-Json -Depth 10) + [Environment]::NewLine
    $bytes = (New-Object System.Text.UTF8Encoding($false)).GetBytes($json)
    $temp = Join-Path $parent ('.' + [IO.Path]::GetFileName($fullPath) + '.' + [Guid]::NewGuid().ToString('N') + '.tmp')

    $stream = $null
    try {
        $stream = [IO.File]::Open($temp, [IO.FileMode]::CreateNew, [IO.FileAccess]::Write, [IO.FileShare]::None)
        $stream.Write($bytes, 0, $bytes.Length)
        $stream.Flush($true)
        $stream.Dispose()
        $stream = $null

        if ([IO.File]::Exists($fullPath)) {
            [IO.File]::Replace($temp, $fullPath, [NullString]::Value)
        } else {
            [IO.File]::Move($temp, $fullPath)
        }
    } finally {
        if ($null -ne $stream) { $stream.Dispose() }
        if ([IO.File]::Exists($temp)) {
            Remove-Item -LiteralPath $temp -Force -ErrorAction SilentlyContinue
        }
    }
}

function New-RwReleaseTestResult {
    param(
        [string]$VersionId = '',
        [string]$VersionPath = '',
        [string]$ManifestPath = '',
        [string]$ManifestSha256 = ''
    )

    $result = [pscustomobject][ordered]@{
        IsValid = $false
        Status = 'Invalid'
        Code = 'Unknown'
        Message = ''
        VersionId = $VersionId
        VersionPath = $VersionPath
        ManifestPath = $ManifestPath
        ManifestSha256 = $ManifestSha256
        ExecutablePath = $null
        FileCount = 0
        TotalBytes = [long]0
        Errors = @()
        CheckedAtUtc = (Get-Date).ToUniversalTime().ToString('yyyy-MM-ddTHH:mm:ssZ')
    }
    $result.PSObject.TypeNames.Insert(0, 'RustWrath.ReleaseSetTestResult')
    return $result
}

function Complete-RwReleaseTestResult {
    param(
        [Parameter(Mandatory)]$Result,
        [Parameter(Mandatory)]$Errors
    )

    if ($Errors.Count -eq 0) {
        $Result.IsValid = $true
        $Result.Status = 'Valid'
        $Result.Code = 'OK'
        $Result.Message = 'Release set is valid.'
        $Result.Errors = @()
    } else {
        $Result.IsValid = $false
        $Result.Status = 'Invalid'
        $Result.Errors = @($Errors.ToArray())
        $first = [string]$Result.Errors[0]
        $separator = $first.IndexOf(':')
        if ($separator -gt 0) { $Result.Code = $first.Substring(0, $separator) } else { $Result.Code = $first }
        $Result.Message = ($Result.Errors -join '; ')
    }
    return $Result
}

function Test-ReleaseSet {
    [CmdletBinding()]
    param(
        [Parameter(Mandatory)][string]$ReleaseRoot,
        [string]$VersionId = '',
        [string]$VersionPath = '',
        [long]$MinimumFreeBytes = 0
    )

    $rootFull = [IO.Path]::GetFullPath($ReleaseRoot).TrimEnd([char[]]@('\', '/'))
    $result = New-RwReleaseTestResult -VersionId $VersionId
    $errors = New-Object 'System.Collections.Generic.List[string]'

    try {
        if ([string]::IsNullOrWhiteSpace($VersionPath)) {
            if ([string]::IsNullOrWhiteSpace($VersionId)) {
                [void]$errors.Add('MissingVersionId: Supply VersionId or VersionPath.')
                return (Complete-RwReleaseTestResult -Result $result -Errors $errors)
            }
            try { Assert-RwReleaseVersionId -VersionId $VersionId } catch {
                [void]$errors.Add("InvalidVersionId: $($_.Exception.Message)")
                return (Complete-RwReleaseTestResult -Result $result -Errors $errors)
            }
            $VersionPath = Join-Path (Join-Path $rootFull 'versions') $VersionId
        }

        $versionFull = [IO.Path]::GetFullPath($VersionPath).TrimEnd([char[]]@('\', '/'))
        $result.VersionPath = $versionFull
        $leaf = Split-Path -Leaf $versionFull
        if ([string]::IsNullOrWhiteSpace($result.VersionId)) { $result.VersionId = $leaf }

        if (!(Test-RwPathWithinRoot $rootFull $versionFull)) {
            [void]$errors.Add("PathEscape: Release version directory '$versionFull' escapes release root '$rootFull'.")
            return (Complete-RwReleaseTestResult -Result $result -Errors $errors)
        }
        if (!(Test-Path -LiteralPath $versionFull -PathType Container)) {
            [void]$errors.Add("MissingReleaseDirectory: Release version directory is missing: $versionFull")
            return (Complete-RwReleaseTestResult -Result $result -Errors $errors)
        }
        if ($leaf -notmatch '^[A-Za-z0-9][A-Za-z0-9._+-]{0,127}$') {
            [void]$errors.Add("InvalidVersionId: Directory name '$leaf' is not a valid release version id.")
        }

        $manifestPath = Join-Path $versionFull $script:RustWrathReleaseManifestName
        $readyPath = Join-Path $versionFull $script:RustWrathReleaseReadyName
        $incompletePath = Join-Path $versionFull $script:RustWrathReleaseIncompleteName
        $result.ManifestPath = $manifestPath

        if (Test-Path -LiteralPath $incompletePath) {
            [void]$errors.Add("IncompleteStaging: Release version contains '$($script:RustWrathReleaseIncompleteName)' and is not activatable.")
        }
        if (!(Test-Path -LiteralPath $manifestPath -PathType Leaf)) {
            [void]$errors.Add("MissingManifest: Release manifest is missing: $manifestPath")
        }
        if (!(Test-Path -LiteralPath $readyPath -PathType Leaf)) {
            [void]$errors.Add("MissingReadyMarker: Release ready marker is missing: $readyPath")
        }
        if ($errors.Count -gt 0) {
            return (Complete-RwReleaseTestResult -Result $result -Errors $errors)
        }

        $manifestHash = Get-RwSha256 -Path $manifestPath
        $result.ManifestSha256 = $manifestHash

        $manifest = $null
        try {
            $manifest = [IO.File]::ReadAllText($manifestPath) | ConvertFrom-Json
        } catch {
            [void]$errors.Add("InvalidManifestJson: $($_.Exception.Message)")
            return (Complete-RwReleaseTestResult -Result $result -Errors $errors)
        }
        if ($null -eq $manifest) {
            [void]$errors.Add('InvalidManifestJson: Manifest parsed as null.')
            return (Complete-RwReleaseTestResult -Result $result -Errors $errors)
        }

        $ready = $null
        try {
            $ready = [IO.File]::ReadAllText($readyPath) | ConvertFrom-Json
        } catch {
            [void]$errors.Add("InvalidReadyMarker: $($_.Exception.Message)")
            return (Complete-RwReleaseTestResult -Result $result -Errors $errors)
        }

        if ($null -eq $manifest.PSObject.Properties['schemaVersion']) {
            [void]$errors.Add('MissingSchemaVersion: Manifest has no schemaVersion.')
        } elseif ([int]$manifest.schemaVersion -ne [int]$script:RustWrathReleaseSchemaVersion) {
            [void]$errors.Add("UnsupportedSchemaVersion: Manifest schemaVersion '$($manifest.schemaVersion)' is not supported (expected $($script:RustWrathReleaseSchemaVersion)).")
        }
        if ($null -eq $manifest.PSObject.Properties['hashAlgorithm']) {
            [void]$errors.Add('MissingHashAlgorithm: Manifest has no hashAlgorithm.')
        } elseif ([string]$manifest.hashAlgorithm -ne [string]$script:RustWrathReleaseHashAlgorithm) {
            [void]$errors.Add("UnsupportedHashAlgorithm: Manifest hashAlgorithm '$($manifest.hashAlgorithm)' is not supported (expected $($script:RustWrathReleaseHashAlgorithm)).")
        }

        $manifestVersionId = [string]$manifest.versionId
        if ([string]::IsNullOrWhiteSpace($manifestVersionId)) {
            [void]$errors.Add('MissingVersionId: Manifest has no versionId.')
        } elseif ($manifestVersionId -ne $leaf) {
            [void]$errors.Add("VersionIdMismatch: Manifest versionId '$manifestVersionId' does not match directory '$leaf'.")
        } elseif ($manifestVersionId -ne $result.VersionId) {
            [void]$errors.Add("VersionIdMismatch: Manifest versionId '$manifestVersionId' does not match requested '$($result.VersionId)'.")
        }

        $created = [DateTimeOffset]::MinValue
        $createdOk = $false
        if ([string]::IsNullOrWhiteSpace([string]$manifest.createdAtUtc)) {
            [void]$errors.Add('MissingCreatedAtUtc: Manifest has no createdAtUtc.')
        } else {
            $createdOk = [DateTimeOffset]::TryParse(
                [string]$manifest.createdAtUtc,
                [Globalization.CultureInfo]::InvariantCulture,
                [Globalization.DateTimeStyles]::AssumeUniversal,
                [ref]$created)
            if (!$createdOk) {
                [void]$errors.Add("InvalidCreatedAtUtc: '$($manifest.createdAtUtc)' is not a parseable timestamp.")
            }
        }
        if ([string]::IsNullOrWhiteSpace([string]$manifest.sourceRevision)) {
            [void]$errors.Add('MissingSourceRevision: Manifest has no sourceRevision.')
        }
        if ([string]::IsNullOrWhiteSpace([string]$manifest.buildIdentity)) {
            [void]$errors.Add('MissingBuildIdentity: Manifest has no buildIdentity.')
        }
        if ($null -eq $manifest.PSObject.Properties['requiredDataProfileIds']) {
            [void]$errors.Add('MissingRequiredDataProfileIds: Manifest has no requiredDataProfileIds list.')
        } else {
            foreach ($profile in @($manifest.requiredDataProfileIds)) {
                if ([string]::IsNullOrWhiteSpace([string]$profile)) {
                    [void]$errors.Add('InvalidRequiredDataProfileId: requiredDataProfileIds contains an empty value.')
                }
            }
        }

        if ($null -eq $ready) {
            [void]$errors.Add('InvalidReadyMarker: Ready marker parsed as null.')
        } else {
            if ($null -eq $ready.PSObject.Properties['schemaVersion'] -or [int]$ready.schemaVersion -ne [int]$script:RustWrathReleaseSchemaVersion) {
                [void]$errors.Add('UnsupportedReadySchemaVersion: Ready marker schema is not supported.')
            }
            if ([string]$ready.versionId -ne $manifestVersionId) {
                [void]$errors.Add("ReadyVersionIdMismatch: Ready marker '$($ready.versionId)' does not match manifest '$manifestVersionId'.")
            }
            if ([string]$ready.manifestSha256 -ne $manifestHash) {
                [void]$errors.Add("ReadyManifestMismatch: Ready marker hash '$($ready.manifestSha256)' does not match manifest hash '$manifestHash'.")
            }
        }

        $files = @()
        if ($null -eq $manifest.PSObject.Properties['files']) {
            [void]$errors.Add('MissingFiles: Manifest has no files list.')
        } else {
            $files = @($manifest.files)
        }
        if ($files.Count -eq 0) {
            [void]$errors.Add('MissingFiles: Manifest files list is empty.')
        }

        $executableRelative = ConvertTo-RwRelativePath ([string]$manifest.executablePath)
        $result.ExecutablePath = $executableRelative
        if ([string]::IsNullOrWhiteSpace($executableRelative)) {
            [void]$errors.Add('MissingExecutable: Manifest has no executablePath.')
        } elseif (!(Test-RwSafeRelativePath $executableRelative)) {
            [void]$errors.Add("PathEscape: Executable path '$executableRelative' is not a safe relative path.")
        }

        $seen = New-Object 'System.Collections.Generic.HashSet[string]' ([StringComparer]::OrdinalIgnoreCase)
        $executableMatches = 0
        $totalBytes = [long]0

        foreach ($file in $files) {
            $relative = ConvertTo-RwRelativePath ([string]$file.path)
            if (!(Test-RwSafeRelativePath $relative)) {
                [void]$errors.Add("PathEscape: Manifest file path '$relative' escapes the version directory.")
                continue
            }
            if (!($seen.Add($relative))) {
                [void]$errors.Add("DuplicatePath: Manifest lists '$relative' more than once.")
                continue
            }
            if ($relative -ieq $script:RustWrathReleaseManifestName -or $relative -ieq $script:RustWrathReleaseReadyName -or $relative -ieq $script:RustWrathReleaseIncompleteName) {
                [void]$errors.Add("ManifestSelfReference: Manifest must not list its own control file '$relative'.")
                continue
            }

            $expectedSize = [long]0
            $sizeOk = $false
            if ($null -ne $file.PSObject.Properties['size']) {
                try {
                    $expectedSize = [long]$file.size
                    if (([double]$file.size) -eq ([double]$expectedSize) -and $expectedSize -ge 0) { $sizeOk = $true }
                } catch {
                    $sizeOk = $false
                }
            }
            if (!$sizeOk) {
                [void]$errors.Add("InvalidSize: File '$relative' has an invalid size.")
                continue
            }

            $expectedHash = ([string]$file.sha256).ToUpperInvariant()
            if ($expectedHash -notmatch '^[0-9A-F]{64}$') {
                [void]$errors.Add("InvalidHash: File '$relative' has an invalid SHA-256 value.")
                continue
            }

            $candidate = Join-Path $versionFull ($relative -replace '/', '\')
            if (!(Test-RwPathWithinRoot $versionFull $candidate)) {
                [void]$errors.Add("PathEscape: File '$relative' resolves outside the version directory.")
                continue
            }
            if (!(Test-Path -LiteralPath $candidate -PathType Leaf)) {
                [void]$errors.Add("MissingFile: File '$relative' is missing.")
                continue
            }

            $item = Get-Item -LiteralPath $candidate -Force
            if (($item.Attributes -band [IO.FileAttributes]::ReparsePoint) -ne 0 -or (Test-RwReparsePointInPath -Root $versionFull -Path $candidate)) {
                [void]$errors.Add("ReparsePointTraversal: File '$relative' is a symlink/reparse point or resolves through one.")
                continue
            }
            if ([long]$item.Length -ne $expectedSize) {
                [void]$errors.Add("SizeMismatch: File '$relative' is $($item.Length) bytes, expected $expectedSize.")
                continue
            }

            $actualHash = Get-RwSha256 -Path $candidate
            if ($actualHash -ne $expectedHash) {
                [void]$errors.Add("HashMismatch: File '$relative' has SHA-256 $actualHash, expected $expectedHash.")
                continue
            }

            if ($relative -ieq $executableRelative) { $executableMatches++ }
            $totalBytes += $expectedSize
        }

        $result.FileCount = $files.Count
        $result.TotalBytes = $totalBytes

        if (![string]::IsNullOrWhiteSpace($executableRelative) -and $executableMatches -eq 0) {
            [void]$errors.Add("MissingExecutable: Manifest executablePath '$executableRelative' is not in the verified file list.")
        }
        if ($executableMatches -gt 1) {
            [void]$errors.Add("DuplicateExecutable: Manifest executablePath '$executableRelative' matches more than one entry.")
        }

        if ($MinimumFreeBytes -gt 0) {
            try {
                $free = Get-RwAvailableFreeBytes -Path $versionFull
                if ($free -lt $MinimumFreeBytes) {
                    [void]$errors.Add("InsufficientDiskSpace: $free bytes available, $MinimumFreeBytes bytes required.")
                }
            } catch {
                [void]$errors.Add("DiskSpaceCheckFailed: $($_.Exception.Message)")
            }
        }
    } catch {
        [void]$errors.Add("UnexpectedError: $($_.Exception.Message)")
    }

    return (Complete-RwReleaseTestResult -Result $result -Errors $errors)
}

function Stage-ReleaseSet {
    [CmdletBinding()]
    param(
        [Parameter(Mandatory)][string]$SourcePath,
        [Parameter(Mandatory)][string]$ReleaseRoot,
        [Parameter(Mandatory)][string]$VersionId,
        [string]$SourceRevision = 'unrecorded',
        [string]$BuildIdentity = 'local',
        [string[]]$RequiredDataProfileIds = @(),
        [string]$CreatedAtUtc = '',
        [long]$MinimumFreeBytes = 0,
        [string]$ExecutablePath = 'benilla.exe',
        [string[]]$ExcludeName = @()
    )

    Assert-RwReleaseVersionId -VersionId $VersionId
    if (!(Test-Path -LiteralPath $SourcePath -PathType Container)) {
        throw "Release source directory is missing: $SourcePath"
    }

    $rootFull = [IO.Path]::GetFullPath($ReleaseRoot).TrimEnd([char[]]@('\', '/'))
    Assert-RwReleaseRootLocation -Root $rootFull
    $versionsDir = Join-Path $rootFull 'versions'
    $stagingRoot = Join-Path $rootFull 'staging'
    $finalPath = Join-Path $versionsDir $VersionId

    [void](New-Item -ItemType Directory -Path $versionsDir -Force)
    [void](New-Item -ItemType Directory -Path $stagingRoot -Force)

    if (Test-Path -LiteralPath $finalPath) {
        throw "Release version '$VersionId' already exists at '$finalPath'. Release sets are immutable; choose a new version id."
    }

    $manifest = New-ReleaseManifest -VersionPath $SourcePath -VersionId $VersionId `
        -SourceRevision $SourceRevision -BuildIdentity $BuildIdentity `
        -RequiredDataProfileIds $RequiredDataProfileIds -CreatedAtUtc $CreatedAtUtc `
        -ExecutablePath $ExecutablePath -ExcludeName $ExcludeName

    $totalBytes = [long]0
    foreach ($file in @($manifest.files)) { $totalBytes += [long]$file.size }
    if ($MinimumFreeBytes -gt 0 -and $MinimumFreeBytes -gt ([long]::MaxValue - $totalBytes)) {
        $requiredBytes = [long]::MaxValue
    } else {
        $requiredBytes = $totalBytes + [Math]::Max([long]0, $MinimumFreeBytes)
    }
    $availableBytes = Get-RwAvailableFreeBytes -Path $rootFull
    if ($requiredBytes -gt $availableBytes) {
        throw "Insufficient disk space for release '$VersionId': $requiredBytes bytes required, $availableBytes bytes available at '$rootFull'."
    }

    $stagingPath = Join-Path $stagingRoot ($VersionId + '.' + [Guid]::NewGuid().ToString('N'))
    [void](New-Item -ItemType Directory -Path $stagingPath -Force)
    $incompletePath = Join-Path $stagingPath $script:RustWrathReleaseIncompleteName
    [IO.File]::WriteAllText($incompletePath, "staging $VersionId`r`n", (New-Object System.Text.UTF8Encoding($false)))

    try {
        foreach ($file in @($manifest.files)) {
            $sourceFile = Join-RwPathWithinRoot -Root $SourcePath -RelativePath $file.path
            $destinationFile = Join-RwPathWithinRoot -Root $stagingPath -RelativePath $file.path
            $destinationParent = Split-Path -Parent $destinationFile
            if (![string]::IsNullOrWhiteSpace($destinationParent)) {
                [void](New-Item -ItemType Directory -Path $destinationParent -Force)
            }

            Copy-Item -LiteralPath $sourceFile -Destination $destinationFile -Force -ErrorAction Stop

            if (!(Test-Path -LiteralPath $destinationFile -PathType Leaf)) {
                throw "Copied file is missing: $($file.path)"
            }
            $copied = Get-Item -LiteralPath $destinationFile -Force
            if (($copied.Attributes -band [IO.FileAttributes]::ReparsePoint) -ne 0) {
                throw "Copied file became a reparse point: $($file.path)"
            }
            if ([long]$copied.Length -ne [long]$file.size) {
                throw "Copied file size mismatch for '$($file.path)': $($copied.Length) bytes, expected $($file.size)."
            }
            $copiedHash = Get-RwSha256 -Path $destinationFile
            if ($copiedHash -ne ([string]$file.sha256).ToUpperInvariant()) {
                throw "Copied file hash mismatch for '$($file.path)': $copiedHash, expected $($file.sha256)."
            }
        }

        $manifestPath = Join-Path $stagingPath $script:RustWrathReleaseManifestName
        $written = Write-ReleaseManifest -Manifest $manifest -Path $manifestPath
        Write-RwReadyMarker -Path (Join-Path $stagingPath $script:RustWrathReleaseReadyName) `
            -VersionId $VersionId -ManifestSha256 $written.Sha256 -CreatedAtUtc $manifest.createdAtUtc

        # The ready marker is written last. Remove the incomplete marker only after both
        # control files exist, then move the completed directory into versions atomically.
        Remove-Item -LiteralPath $incompletePath -Force
        if (Test-Path -LiteralPath $finalPath) {
            throw "Release version '$VersionId' appeared at '$finalPath' while staging."
        }
        Move-Item -LiteralPath $stagingPath -Destination $finalPath -ErrorAction Stop

        return [pscustomobject][ordered]@{
            Status = 'Staged'
            VersionId = $VersionId
            VersionPath = $finalPath
            ManifestPath = (Join-Path $finalPath $script:RustWrathReleaseManifestName)
            ManifestSha256 = $written.Sha256
            ReadyPath = (Join-Path $finalPath $script:RustWrathReleaseReadyName)
            FileCount = @($manifest.files).Count
            TotalBytes = $totalBytes
            SourceRevision = $SourceRevision
            BuildIdentity = $BuildIdentity
            CreatedAtUtc = $manifest.createdAtUtc
        } | ForEach-Object { $_.PSObject.TypeNames.Insert(0, 'RustWrath.ReleaseSetStageResult'); $_ }
    } catch {
        if (Test-Path -LiteralPath $stagingPath) {
            Remove-Item -LiteralPath $stagingPath -Recurse -Force -ErrorAction SilentlyContinue
        }
        throw
    }
}

function New-RwReleaseState {
    param(
        [string]$Status = 'None',
        [string]$SelectorPath = ''
    )

    $state = [pscustomobject][ordered]@{
        Status = $Status
        IsPresent = $false
        SelectorExists = $false
        SelectorPath = $SelectorPath
        VersionId = $null
        VersionPath = $null
        ManifestPath = $null
        ManifestSha256 = $null
        SelectedManifestSha256 = $null
        ManifestMatchesSelector = $false
        PreviousVersionId = $null
        PreviousVersionPath = $null
        PreviousManifestSha256 = $null
        ActivatedAtUtc = $null
        LastRecovery = $null
        RecoveredFromVersionId = $null
        Error = $null
    }
    $state.PSObject.TypeNames.Insert(0, 'RustWrath.ReleaseSetState')
    return $state
}

function Get-ActiveReleaseSet {
    [CmdletBinding()]
    param([Parameter(Mandatory)][string]$ReleaseRoot)

    $rootFull = [IO.Path]::GetFullPath($ReleaseRoot).TrimEnd([char[]]@('\', '/'))
    $selectorPath = Join-Path $rootFull $script:RustWrathReleaseSelectorName
    $state = New-RwReleaseState -Status 'None' -SelectorPath $selectorPath

    if (!(Test-Path -LiteralPath $selectorPath -PathType Leaf)) {
        return $state
    }
    $state.SelectorExists = $true

    $selector = $null
    try {
        $selector = [IO.File]::ReadAllText($selectorPath) | ConvertFrom-Json
    } catch {
        $state.Status = 'Invalid'
        $state.Error = "Active release selector is not valid JSON: $($_.Exception.Message)"
        return $state
    }
    if ($null -eq $selector) {
        $state.Status = 'Invalid'
        $state.Error = 'Active release selector parsed as null.'
        return $state
    }
    if ($null -eq $selector.PSObject.Properties['schemaVersion'] -or [int]$selector.schemaVersion -ne [int]$script:RustWrathReleaseSchemaVersion) {
        $state.Status = 'Invalid'
        $state.Error = "Active release selector schemaVersion '$($selector.schemaVersion)' is not supported."
        return $state
    }

    $versionId = [string]$selector.versionId
    try {
        Assert-RwReleaseVersionId -VersionId $versionId
    } catch {
        $state.Status = 'Invalid'
        $state.Error = $_.Exception.Message
        return $state
    }

    $versionPath = Join-Path (Join-Path $rootFull 'versions') $versionId
    if (!(Test-RwPathWithinRoot $rootFull $versionPath)) {
        $state.Status = 'Invalid'
        $state.Error = "Active release path escapes release root: $versionPath"
        return $state
    }

    $state.Status = 'Active'
    $state.IsPresent = $true
    $state.VersionId = $versionId
    $state.VersionPath = $versionPath
    $state.ManifestPath = Join-Path $versionPath $script:RustWrathReleaseManifestName
    $state.SelectedManifestSha256 = [string]$selector.manifestSha256
    $state.ActivatedAtUtc = [string]$selector.activatedAtUtc
    $state.PreviousVersionId = [string]$selector.previousVersionId
    $state.PreviousManifestSha256 = [string]$selector.previousManifestSha256
    $state.LastRecovery = $selector.lastRecovery
    if ($null -ne $selector.lastRecovery) {
        $state.RecoveredFromVersionId = [string]$selector.lastRecovery.recoveredFromVersionId
    }
    if (![string]::IsNullOrWhiteSpace($state.PreviousVersionId)) {
        try {
            Assert-RwReleaseVersionId -VersionId $state.PreviousVersionId
            $state.PreviousVersionPath = Join-Path (Join-Path $rootFull 'versions') $state.PreviousVersionId
        } catch {
            $state.Status = 'Invalid'
            $state.IsPresent = $false
            $state.Error = "Active release selector has an invalid previousVersionId: $($_.Exception.Message)"
            return $state
        }
    }
    if (Test-Path -LiteralPath $state.ManifestPath -PathType Leaf) {
        $state.ManifestSha256 = Get-RwSha256 -Path $state.ManifestPath
        $state.ManifestMatchesSelector = ($state.ManifestSha256 -eq $state.SelectedManifestSha256)
    }
    return $state
}

function Get-PreviousReleaseSet {
    [CmdletBinding()]
    param([Parameter(Mandatory)][string]$ReleaseRoot)

    $rootFull = [IO.Path]::GetFullPath($ReleaseRoot).TrimEnd([char[]]@('\', '/'))
    $active = Get-ActiveReleaseSet -ReleaseRoot $rootFull
    $state = New-RwReleaseState -Status 'None' -SelectorPath $active.SelectorPath

    if ($active.Status -eq 'Invalid') {
        $state.Status = 'Invalid'
        $state.SelectorExists = $active.SelectorExists
        $state.Error = $active.Error
        return $state
    }
    if (!$active.IsPresent -or [string]::IsNullOrWhiteSpace($active.PreviousVersionId)) {
        return $state
    }

    $state.Status = 'Previous'
    $state.IsPresent = $true
    $state.SelectorExists = $true
    $state.VersionId = $active.PreviousVersionId
    $state.VersionPath = $active.PreviousVersionPath
    $state.ManifestPath = Join-Path $active.PreviousVersionPath $script:RustWrathReleaseManifestName
    $state.PreviousManifestSha256 = $active.PreviousManifestSha256
    $state.ActivatedAtUtc = $active.ActivatedAtUtc
    if (Test-Path -LiteralPath $state.ManifestPath -PathType Leaf) {
        $state.ManifestSha256 = Get-RwSha256 -Path $state.ManifestPath
        $state.ManifestMatchesSelector = ($state.ManifestSha256 -eq $state.PreviousManifestSha256)
    }
    return $state
}

function Activate-ReleaseSet {
    [CmdletBinding()]
    param(
        [Parameter(Mandatory)][string]$ReleaseRoot,
        [Parameter(Mandatory)][string]$VersionId,
        [string]$VersionPath = '',
        [string]$Reason = 'manual',
        [string]$CreatedAtUtc = ''
    )

    $rootFull = [IO.Path]::GetFullPath($ReleaseRoot).TrimEnd([char[]]@('\', '/'))
    Assert-RwReleaseRootLocation -Root $rootFull
    $test = Test-ReleaseSet -ReleaseRoot $rootFull -VersionId $VersionId -VersionPath $VersionPath
    if (!$test.IsValid) {
        throw "Refusing to activate release '$VersionId': $($test.Message)"
    }

    $current = Get-ActiveReleaseSet -ReleaseRoot $rootFull
    if ($current.Status -eq 'Invalid') {
        throw "Refusing to activate release '$VersionId': active selector is invalid ($($current.Error))."
    }

    if ($current.IsPresent -and $current.VersionId -eq $test.VersionId -and
        $current.SelectedManifestSha256 -eq $test.ManifestSha256) {
        return [pscustomobject][ordered]@{
            Status = 'AlreadyActive'
            IsActivated = $false
            AlreadyActive = $true
            VersionId = $test.VersionId
            PreviousVersionId = $current.PreviousVersionId
            ManifestSha256 = $test.ManifestSha256
            VersionPath = $test.VersionPath
            SelectorPath = $current.SelectorPath
            Message = "Release '$($test.VersionId)' is already active."
        } | ForEach-Object { $_.PSObject.TypeNames.Insert(0, 'RustWrath.ReleaseSetActivationResult'); $_ }
    }

    $previousVersionId = $null
    $previousManifestSha256 = $null
    if ($current.IsPresent) {
        $currentTest = Test-ReleaseSet -ReleaseRoot $rootFull -VersionId $current.VersionId
        if ($currentTest.IsValid -and $currentTest.ManifestSha256 -eq $current.SelectedManifestSha256) {
            $previousVersionId = $current.VersionId
            $previousManifestSha256 = $currentTest.ManifestSha256
        } else {
            Write-Warning "Current release '$($current.VersionId)' is not valid or does not match the selector; it will not be recorded as previous known-good."
        }
    }

    $stamp = Get-RwUtcStamp -Value $CreatedAtUtc
    $selector = [pscustomobject][ordered]@{
        schemaVersion = [int]$script:RustWrathReleaseSchemaVersion
        versionId = $test.VersionId
        manifestSha256 = $test.ManifestSha256
        activatedAtUtc = $stamp
        previousVersionId = $previousVersionId
        previousManifestSha256 = $previousManifestSha256
        activationReason = $Reason
    }
    Write-RwJsonAtomic -Path $current.SelectorPath -Value $selector

    return [pscustomobject][ordered]@{
        Status = 'Activated'
        IsActivated = $true
        AlreadyActive = $false
        VersionId = $test.VersionId
        PreviousVersionId = $previousVersionId
        ManifestSha256 = $test.ManifestSha256
        VersionPath = $test.VersionPath
        SelectorPath = $current.SelectorPath
        Message = "Activated release '$($test.VersionId)'."
    } | ForEach-Object { $_.PSObject.TypeNames.Insert(0, 'RustWrath.ReleaseSetActivationResult'); $_ }
}

function Recover-ReleaseSet {
    [CmdletBinding()]
    param(
        [Parameter(Mandatory)][string]$ReleaseRoot,
        [string]$Reason = 'startup-failure',
        [string]$CreatedAtUtc = ''
    )

    $rootFull = [IO.Path]::GetFullPath($ReleaseRoot).TrimEnd([char[]]@('\', '/'))
    Assert-RwReleaseRootLocation -Root $rootFull
    $current = Get-ActiveReleaseSet -ReleaseRoot $rootFull

    if ($current.Status -eq 'Invalid') {
        throw "Cannot recover release set: active selector is invalid ($($current.Error)). Selector left unchanged."
    }
    if (!$current.IsPresent) {
        throw 'Cannot recover release set: no active selector exists. Selector left unchanged.'
    }

    $lastRecovery = $current.LastRecovery
    if ($null -ne $lastRecovery -and
        [string]$lastRecovery.recoveredToVersionId -eq $current.VersionId -and
        [string]$lastRecovery.recoveredFromVersionId -ne $current.VersionId) {
        return [pscustomobject][ordered]@{
            Status = 'AlreadyRecovered'
            Recovered = $false
            AlreadyRecovered = $true
            FromVersionId = [string]$lastRecovery.recoveredFromVersionId
            ToVersionId = $current.VersionId
            ManifestSha256 = $current.ManifestSha256
            SelectorPath = $current.SelectorPath
            Message = "Release set already recovered to '$($current.VersionId)'."
        } | ForEach-Object { $_.PSObject.TypeNames.Insert(0, 'RustWrath.ReleaseSetRecoveryResult'); $_ }
    }

    if ([string]::IsNullOrWhiteSpace($current.PreviousVersionId)) {
        throw "Cannot recover release set: no previous known-good version is recorded for '$($current.VersionId)'. Selector left unchanged."
    }

    $previous = Test-ReleaseSet -ReleaseRoot $rootFull -VersionId $current.PreviousVersionId
    if (!$previous.IsValid) {
        throw "Cannot recover to previous release '$($current.PreviousVersionId)': $($previous.Message). Selector left unchanged."
    }
    if (![string]::IsNullOrWhiteSpace($current.PreviousManifestSha256) -and
        $current.PreviousManifestSha256 -ne $previous.ManifestSha256) {
        throw "Cannot recover to previous release '$($current.PreviousVersionId)': manifest hash differs from the recorded known-good hash. Selector left unchanged."
    }

    $stamp = Get-RwUtcStamp -Value $CreatedAtUtc
    $selector = [pscustomobject][ordered]@{
        schemaVersion = [int]$script:RustWrathReleaseSchemaVersion
        versionId = $previous.VersionId
        manifestSha256 = $previous.ManifestSha256
        activatedAtUtc = $stamp
        previousVersionId = $null
        previousManifestSha256 = $null
        activationReason = 'recovery'
        lastRecovery = [pscustomobject][ordered]@{
            recoveredFromVersionId = $current.VersionId
            recoveredFromManifestSha256 = $current.SelectedManifestSha256
            recoveredToVersionId = $previous.VersionId
            recoveredAtUtc = $stamp
            reason = $Reason
        }
    }
    Write-RwJsonAtomic -Path $current.SelectorPath -Value $selector

    return [pscustomobject][ordered]@{
        Status = 'Recovered'
        Recovered = $true
        AlreadyRecovered = $false
        FromVersionId = $current.VersionId
        ToVersionId = $previous.VersionId
        ManifestSha256 = $previous.ManifestSha256
        SelectorPath = $current.SelectorPath
        Message = "Recovered release set from '$($current.VersionId)' to '$($previous.VersionId)'."
    } | ForEach-Object { $_.PSObject.TypeNames.Insert(0, 'RustWrath.ReleaseSetRecoveryResult'); $_ }
}
