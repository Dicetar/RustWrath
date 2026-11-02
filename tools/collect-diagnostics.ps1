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
    [string]$OutFile
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
$metaPath   = Join-Path $RunDir 'launch-meta.txt'

$lines = New-Object System.Collections.Generic.List[string]
$shareMode = [System.IO.FileShare]::ReadWrite -bor [System.IO.FileShare]::Delete
foreach ($p in @($stderrPath, $stdoutPath)) {
    if (Test-Path -LiteralPath $p) {
        $fs = New-Object System.IO.FileStream($p, [System.IO.FileMode]::Open, [System.IO.FileAccess]::Read, $shareMode)
        $sr = New-Object System.IO.StreamReader($fs, [System.Text.Encoding]::UTF8, $true)
        try {
            while ($null -ne ($line = $sr.ReadLine())) {
                # Strip ANSI escape sequences for clean regex matching
                $clean = $line -replace '\x1b\[[0-9;]*[a-zA-Z]', ''
                $lines.Add($clean)
            }
        } finally {
            $sr.Dispose()
            $fs.Dispose()
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
    if ($line -match "attempt to call global '([^']+)'") {
        $name = $Matches[1]
        $missingGlobals[$name] = ($missingGlobals[$name] -as [int]) + 1
    }

    # 2. Missing widget methods: attempt to call method 'Bar' (a nil value)
    if ($line -match "attempt to call method '([^']+)'") {
        $name = $Matches[1]
        $missingMethods[$name] = ($missingMethods[$name] -as [int]) + 1
    }

    # 3. Missing table/frame fields: attempt to index field 'Baz' (a nil value) or global 'Baz'
    if ($line -match "attempt to index (?:field|global) '([^']+)' \(a nil value\)") {
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
        [void]$sb.AppendLine("- None detected.")
    } else {
        foreach ($entry in ($Map.GetEnumerator() | Sort-Object Value -Descending)) {
            [void]$sb.AppendLine("- ``$($entry.Key)`` (x$($entry.Value))")
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
[void]$report.Append((Format-Section -Title "DBC Schema / Record Mismatches" -Map $dbcFailures))
[void]$report.Append((Format-Section -Title "Asset / M2 / Skin / BLP Errors" -Map $assetFailures))
[void]$report.Append((Format-Section -Title "All Lua / UI Runtime Errors & Warnings" -Map $luaErrors))

$reportText = $report.ToString()
Set-Content -LiteralPath $OutFile -Value $reportText -Encoding UTF8

Write-Host $reportText
Write-Host "Saved diagnostic report to: $OutFile" -ForegroundColor Green
