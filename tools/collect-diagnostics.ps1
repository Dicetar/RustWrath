#Requires -Version 5.1
<#
.SYNOPSIS
    Scans the latest RustWrath launcher log (stderr.log + stdout.log) and extracts
    deduplicated in-game Lua errors, missing globals, missing widget methods,
    missing CVars, DBC schema mismatches, and M2/BLP asset load failures.

.PARAMETER RunDir
    Optional path to a specific launcher run directory under benilla-config/Logs/Launcher.
    Defaults to the most recent run directory.

.PARAMETER OutFile
    Optional markdown file path to write the structured diagnostic report to.
    Defaults to benilla-config/Logs/Launcher/latest-diagnostics.md.
#>
[CmdletBinding()]
param(
    [string]$RunDir,
    [string]$OutFile,
    # JSON array of human observations: category, description, optional evidence path.
    [string]$ObservationsFile
)

Set-StrictMode -Version Latest
$ErrorActionPreference = 'Stop'

$repoRoot = Split-Path -Parent $PSScriptRoot
$launcherRoot = Join-Path $repoRoot 'benilla-config\Logs\Launcher'

if (-not $RunDir) {
    if (-not (Test-Path -LiteralPath $launcherRoot)) {
        throw "No launcher logs directory found at: $launcherRoot"
    }
    $latest = Get-ChildItem -LiteralPath $launcherRoot -Directory |
        Sort-Object LastWriteTime -Descending |
        Select-Object -First 1
    if (-not $latest) {
        throw "No launcher run folders found under: $launcherRoot"
    }
    $RunDir = $latest.FullName
}

if (-not $OutFile) {
    $OutFile = Join-Path $launcherRoot 'latest-diagnostics.md'
}

$stderrPath = Join-Path $RunDir 'stderr.log'
$stdoutPath = Join-Path $RunDir 'stdout.log'
$metaPath   = Join-Path $RunDir 'launch.txt'
if (-not (Test-Path -LiteralPath $metaPath)) {
    $metaPath = Join-Path $RunDir 'launch-meta.txt'
}

$lines = New-Object System.Collections.Generic.List[string]
$locations = New-Object System.Collections.Generic.List[string]
$shareMode = [System.IO.FileShare]::ReadWrite -bor [System.IO.FileShare]::Delete
foreach ($p in @($stderrPath, $stdoutPath)) {
    if (Test-Path -LiteralPath $p) {
        $fs = New-Object System.IO.FileStream($p, [System.IO.FileMode]::Open, [System.IO.FileAccess]::Read, $shareMode)
        $sr = New-Object System.IO.StreamReader($fs, [System.Text.Encoding]::UTF8, $true)
        try {
            $lineNumber = 0
            while ($null -ne ($line = $sr.ReadLine())) {
                $lineNumber++
                # Strip ANSI escape sequences for clean regex matching
                $clean = $line -replace '\x1b\[[0-9;]*[a-zA-Z]', ''
                $lines.Add($clean)
                $locations.Add("$(Split-Path -Leaf $p):$lineNumber")
            }
        } finally {
            $sr.Dispose()
            $fs.Dispose()
        }
    }
}

if ($lines.Count -eq 0) { throw "No log lines found in $RunDir" }

# These are evidence classifiers, not gameplay success detectors. A normal request
# or server rejection does not prove a client defect; retain the exact source.
$domains = [ordered]@{
    Geometry = 'geometry|vertex|vertices|index buffer|mesh|skinn?ing|bone|rig_anim|transform|bounding|\bNaN\b|non-finite|liquid|MH2O'
    Textures = 'texture|\.blp|\bBLP\b|material|shader|mip|magenta'
    Quests = 'quest|gossip|QuestFrame|WatchFrame'
    LootBags = 'loot|inventory|container|bag slot|ui_bag|merchant'
    Abilities = 'spell|cast|cooldown|actionbar|action bar|ActionButton|attack|combat|possess'
    UI = 'FrameXML|GlueXML|ui_script|template|widget|frame type|frame strata|CVar'
    Movement = 'movement|spline|position|teleport|collision|ground|swim|jump'
    Connection = 'disconnect|socket|auth|realm|login|timeout|packet|opcode'
}
$findings = New-Object System.Collections.Generic.List[object]
$grouped = @{}
$failurePattern = '(?i)\b(ERROR|WARN|FATAL|panic|failed|failure|missing|unsupported|unavailable|invalid|rejected|disabled|refused|NaN)\b|runtime error|non-finite|cannot|can.t|not found|not implemented|another action is in progress'
for ($i = 0; $i -lt $lines.Count; $i++) {
    $line = $lines[$i]
    if ($line -notmatch $failurePattern) { continue }
    $categories = @($domains.Keys | Where-Object { $line -match $domains[$_] })
    if ($categories.Count -eq 0) { $categories = @('Other') }
    $message = ($line -replace '^\d{4}-\d{2}-\d{2}T\S+\s+(TRACE|DEBUG|INFO|WARN|ERROR)\s+', '').Trim()
    $key = ($categories -join ',') + '|' + $message
    if ($grouped.ContainsKey($key)) {
        $grouped[$key].count++
        $grouped[$key].last = $locations[$i]
    } else {
        $finding = [pscustomobject]@{
            categories = $categories; kind = 'logged diagnostic'; message = $message
            count = 1; first = $locations[$i]; last = $locations[$i]
        }
        $grouped[$key] = $finding
        $findings.Add($finding)
    }
}
$observations = @()
if ($ObservationsFile) {
    $observations = @(Get-Content -LiteralPath $ObservationsFile -Raw | ConvertFrom-Json)
    foreach ($observation in $observations) {
        if (-not $observation.category -or -not $observation.description) {
            throw 'Each observation needs category and description.'
        }
    }
}

$missingGlobals   = @{}
$missingMethods   = @{}
$missingFields    = @{}
$missingCvars     = @{}
$luaErrors        = @{}
$dbcFailures      = @{}
$assetFailures    = @{}
$uiLoadFailures   = @{}

foreach ($line in $lines) {
    # 1. Missing global functions: attempt to call global 'Foo' (a nil value)
    if ($line -match 'attempt to call global [''\x60"]([^''\x60"]+)[''\x60"] \(a nil value\)') {
        $name = $Matches[1]
        $missingGlobals[$name] = ($missingGlobals[$name] -as [int]) + 1
    }

    # 2. Missing widget methods: attempt to call method 'Bar' (a nil value)
    if ($line -match 'attempt to call method [''\x60"]([^''\x60"]+)[''\x60"] \(a nil value\)') {
        $name = $Matches[1]
        $missingMethods[$name] = ($missingMethods[$name] -as [int]) + 1
    }

    # 3. Missing table/frame fields: attempt to index field 'Baz' (a nil value) or global 'Baz'
    if ($line -match 'attempt to index (?:field|global) [''\x60"]([^''\x60"]+)[''\x60"] \(a nil value\)') {
        $name = $Matches[1]
        $missingFields[$name] = ($missingFields[$name] -as [int]) + 1
    }

    # 4. Missing CVars: Couldn't find CVar named 'foo'
    if ($line -match "Couldn't find CVar named '([^']+)'") {
        $name = $Matches[1]
        $missingCvars[$name] = ($missingCvars[$name] -as [int]) + 1
    }

    # 5. DBC / schema failures
    if ($line -match '(?i)(DBC|\.dbc|schema field count|record size|TaxiNodes|Exhaustion|LoadingScreens|Spell\.dbc|ChrClasses|ChrRaces)') {
        if ($line -match '\b(ERROR|WARN)\b') {
            $msg = ($line -replace '^\d{4}-\d{2}-\d{2}T[^\s]+\s+(ERROR|WARN)\s+', '').Trim()
            $dbcFailures[$msg] = ($dbcFailures[$msg] -as [int]) + 1
        }
    }

    # 6. Asset / M2 / .skin / .anim / BLP failures
    if ($line -match '(?i)(Failed to load asset|parsing skin|companion skin|not a SKIN|M2|BLP|\.skin|\.m2|\.blp|\.wmo|\.adt)') {
        if ($line -match '\b(ERROR|WARN)\b') {
            $msg = ($line -replace '^\d{4}-\d{2}-\d{2}T[^\s]+\s+(ERROR|WARN)\s+', '').Trim()
            $assetFailures[$msg] = ($assetFailures[$msg] -as [int]) + 1
        }
    }

    # 7. UI XML / Lua load or handler failures
    if ($line -match '(?i)(Interface\\|FrameXML|GlueXML|runtime error:|Lua error|ui_script:)') {
        if ($line -match '\b(ERROR|WARN)\b' -or $line -match 'runtime error:') {
            $msg = ($line -replace '^\d{4}-\d{2}-\d{2}T[^\s]+\s+(ERROR|WARN)\s+', '').Trim()
            $luaErrors[$msg] = ($luaErrors[$msg] -as [int]) + 1
        }
    }

    # 8. Summary lines from glue / ingame UI loader
    if ($line -match 'ui_script: (stock glue up|in-game UI up).*') {
        $msg = $Matches[0]
        $uiLoadFailures[$msg] = ($uiLoadFailures[$msg] -as [int]) + 1
    }
}

function Format-Section {
    param(
        [string]$Title,
        [hashtable]$Map
    )
    $sb = New-Object System.Text.StringBuilder
    [void]$sb.AppendLine("## $Title ($($Map.Count) unique)")
    if ($Map.Count -eq 0) {
        [void]$sb.AppendLine("- No matching log evidence; behavior is unverified.")
    } else {
        foreach ($entry in ($Map.GetEnumerator() | Sort-Object Value -Descending)) {
            [void]$sb.AppendLine('- ``' + $entry.Key + '`` (x' + $entry.Value + ')')
        }
    }
    [void]$sb.AppendLine()
    return $sb.ToString()
}

$report = New-Object System.Text.StringBuilder
[void]$report.AppendLine("# RustWrath In-Game Diagnostic Report")
[void]$report.AppendLine("- **Run Directory:** ``$RunDir``")
[void]$report.AppendLine("- **Generated:** $(Get-Date -Format o)")
[void]$report.AppendLine("- **Total Log Lines Scanned:** $($lines.Count)")
[void]$report.AppendLine()
[void]$report.AppendLine('## Coverage and limits')
[void]$report.AppendLine('This is a log snapshot, not an automated playthrough or image analysis. A loaded UI is not proof that its controls work. Requests, rejections and guarded features need triage; they are not all client bugs. Missing geometry, wrong textures, invisible loot and unresponsive controls can be silent. Record those as observations with screenshots and reproduction steps.')
[void]$report.AppendLine()
[void]$report.AppendLine('| Area | Unique diagnostic records | Acceptance |')
[void]$report.AppendLine('| --- | ---: | --- |')
foreach ($domain in $domains.Keys) {
    $count = @($findings | Where-Object { $_.categories -contains $domain }).Count
    [void]$report.AppendLine("| $domain | $count | Unverified by log scan |")
}
[void]$report.AppendLine()
[void]$report.AppendLine('## Player observations (reported, not automatically verified)')
foreach ($observation in $observations) {
    [void]$report.AppendLine("- [$($observation.category)] $($observation.description)")
    if ($observation.PSObject.Properties['evidence']) {
        [void]$report.AppendLine("  Evidence: $($observation.evidence)")
    }
}
if ($observations.Count -eq 0) { [void]$report.AppendLine('- No observations supplied.') }
[void]$report.AppendLine()
[void]$report.AppendLine('## Diagnostic evidence by subsystem')
foreach ($domain in @($domains.Keys) + @('Other')) {
    [void]$report.AppendLine("### $domain")
    foreach ($finding in @($findings | Where-Object { $_.categories -contains $domain } | Sort-Object count -Descending)) {
        [void]$report.AppendLine('- ``' + $finding.message + '`` (x' + $finding.count + '; ' + $finding.first + ' through ' + $finding.last + ')')
    }
    [void]$report.AppendLine()
}

if (Test-Path -LiteralPath $metaPath) {
    [void]$report.AppendLine("## Launch Metadata")
    [void]$report.AppendLine('```text')
    [void]$report.AppendLine((Get-Content -LiteralPath $metaPath -Raw).Trim())
    [void]$report.AppendLine('```')
    [void]$report.AppendLine()
}

[void]$report.Append((Format-Section -Title "UI Startup Summary" -Map $uiLoadFailures))
[void]$report.Append((Format-Section -Title "Missing Global Functions (Lua)" -Map $missingGlobals))
[void]$report.Append((Format-Section -Title "Missing Widget Methods (Lua)" -Map $missingMethods))
[void]$report.Append((Format-Section -Title "Missing Frame / Table Fields (Lua)" -Map $missingFields))
[void]$report.Append((Format-Section -Title "Missing CVars" -Map $missingCvars))
[void]$report.Append((Format-Section -Title "DBC Load / Schema Diagnostics" -Map $dbcFailures))
[void]$report.Append((Format-Section -Title "Asset / M2 / Skin / BLP Errors" -Map $assetFailures))
[void]$report.Append((Format-Section -Title "All Lua / UI Runtime Errors & Warnings" -Map $luaErrors))

$reportText = $report.ToString()
Set-Content -LiteralPath $OutFile -Value $reportText -Encoding UTF8
$jsonPath = [System.IO.Path]::ChangeExtension($OutFile, '.json')
[pscustomobject]@{
    schemaVersion = 1; runDirectory = $RunDir; generated = (Get-Date -Format o)
    scannedLines = $lines.Count; findings = @($findings.ToArray()); observations = $observations
    acceptance = 'Unverified: log evidence and human reports only'
} | ConvertTo-Json -Depth 8 | Set-Content -LiteralPath $jsonPath -Encoding UTF8

Write-Host $reportText
Write-Host "Saved diagnostic report to: $OutFile" -ForegroundColor Green
