#Requires -Version 5.1
<#
.SYNOPSIS
    Scans the latest RustWrath launcher log (stderr.log + stdout.log) AND the latest
    in-game flight-recorder snapshot (Logs/InteractionDiagnostics/interaction-*.jsonl),
    extracts deduplicated issues, maps each issue to its owning Rust crate/module in crates/,
    and generates ready-to-resolve repair tickets in latest-diagnostics.md,
    latest-tickets.md, and latest-diagnostics.json.

.PARAMETER RunDir
    Optional path to a specific launcher run directory under benilla-config/Logs/Launcher.
    Defaults to the most recent run directory.

.PARAMETER OutFile
    Optional markdown file path to write the structured diagnostic report to.
    Defaults to benilla-config/Logs/Launcher/latest-diagnostics.md.

.PARAMETER ObservationsFile
    Optional JSON array of human observations: category, description, optional evidence path.
#>
[CmdletBinding()]
param(
    [string]$RunDir,
    [string]$OutFile,
    [string]$ObservationsFile
)

Set-StrictMode -Version Latest
$ErrorActionPreference = 'Stop'

$repoRoot = Split-Path -Parent $PSScriptRoot
$launcherRoot = Join-Path $repoRoot 'benilla-config\Logs\Launcher'
$interactionRoot = Join-Path $repoRoot 'benilla-config\Logs\InteractionDiagnostics'

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
$ticketsPath = Join-Path (Split-Path -Parent $OutFile) 'latest-tickets.md'

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

# Locate the latest InteractionDiagnostics flight-recorder snapshot (.jsonl) if present
$snapshotPath = $null
$snapshotHeader = $null
if (Test-Path -LiteralPath $interactionRoot) {
    $runStartTime = (Get-Item -LiteralPath $RunDir).CreationTimeUtc.AddSeconds(-5)
    $candidateSnapshots = @(Get-ChildItem -LiteralPath $interactionRoot -Filter 'interaction-*.jsonl' -File |
        Sort-Object LastWriteTimeUtc -Descending)
    $matchingSnapshot = $candidateSnapshots | Where-Object { $_.LastWriteTimeUtc -ge $runStartTime } | Select-Object -First 1
    if (-not $matchingSnapshot -and $candidateSnapshots.Count -gt 0) {
        $matchingSnapshot = $candidateSnapshots[0]
    }
    if ($matchingSnapshot) {
        $snapshotPath = $matchingSnapshot.FullName
        $fs = New-Object System.IO.FileStream($snapshotPath, [System.IO.FileMode]::Open, [System.IO.FileAccess]::Read, $shareMode)
        $sr = New-Object System.IO.StreamReader($fs, [System.Text.Encoding]::UTF8, $true)
        try {
            $firstLine = $sr.ReadLine()
            if ($firstLine) {
                $snapshotHeader = $firstLine | ConvertFrom-Json
            }
        } catch {
            $snapshotHeader = $null
        } finally {
            $sr.Dispose()
            $fs.Dispose()
        }
    }
}

if ($lines.Count -eq 0 -and -not $snapshotHeader) {
    throw "No log lines or flight-recorder snapshots found in $RunDir"
}

# Subsystem classifiers and owning source paths
$domains = [ordered]@{
    Geometry   = 'geometry|vertex|vertices|index buffer|mesh|skinn?ing|bone|rig_anim|transform|bounding|\bNaN\b|non-finite|liquid|MH2O'
    Textures   = 'texture|\.blp|\bBLP\b|material|shader|mip|magenta'
    Quests     = 'quest|gossip|QuestFrame|WatchFrame'
    LootBags   = 'loot|inventory|container|bag slot|ui_bag|merchant'
    Abilities  = 'spell|cast|cooldown|actionbar|action bar|ActionButton|attack|combat|possess'
    UI         = 'FrameXML|GlueXML|ui_script|template|widget|frame type|frame strata|CVar'
    Movement   = 'movement|spline|position|teleport|collision|ground|swim|jump'
    Connection = 'disconnect|socket|auth|realm|login|timeout|packet|opcode'
}

$domainSourceMap = @{
    Geometry   = @('crates/benilla-world/src/model_render.rs', 'crates/benilla-world/src/rig_anim.rs', 'crates/benilla-formats/src/m2/')
    Textures   = @('crates/benilla-assets/src/', 'crates/benilla-formats/src/blp.rs', 'crates/benilla-app/src/shaders/')
    Quests     = @('crates/benilla-app/src/ui_quest/', 'crates/benilla-app/src/ui_quest_log.rs', 'crates/benilla-app/src/ui_gossip/', 'crates/benilla-protocol-wrath/src/gameplay.rs')
    LootBags   = @('crates/benilla-app/src/ui_loot/', 'crates/benilla-app/src/ui_merchant/', 'crates/benilla-app/src/ui_items/')
    Abilities  = @('crates/benilla-app/src/spell/', 'crates/benilla-app/src/ui_action/', 'crates/benilla-app/src/ui_cast.rs', 'crates/benilla-app/src/wrath.rs')
    UI         = @('crates/benilla-ui/src/script/', 'crates/benilla-app/src/ui_script/', 'crates/benilla-ui/src/xml/')
    Movement   = @('crates/benilla-app/src/player/', 'crates/benilla-app/src/wrath.rs', 'crates/benilla-world/src/')
    Connection = @('crates/benilla-app/src/wrath.rs', 'crates/benilla-protocol-wrath/src/', 'crates/benilla-app/src/glue_session/')
    Other      = @('crates/benilla-app/src/')
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

# Build Ready-to-Resolve Tickets from extracted evidence + flight recorder
$readyTickets = New-Object System.Collections.Generic.List[object]
$ticketIndex = 0

function Add-ReadyTicket {
    param(
        [string]$Category,
        [string]$Title,
        [string]$Label,
        [string[]]$TargetFiles,
        [string[]]$Evidence,
        [string]$Prompt
    )
    if ($Evidence.Count -eq 0) { return }
    $script:ticketIndex++
    $id = ('AUTO-{0:D2}' -f $script:ticketIndex)
    $readyTickets.Add([pscustomobject]@{
        id          = $id
        category    = $Category
        title       = $Title
        label       = $Label
        targetFiles = $TargetFiles
        evidence    = @($Evidence | Select-Object -First 15)
        prompt      = $Prompt
    })
}

if ($missingGlobals.Count -gt 0) {
    $ev = @($missingGlobals.GetEnumerator() | Sort-Object Value -Descending | ForEach-Object { "$($_.Key) (x$($_.Value))" })
    $names = @($missingGlobals.Keys | Select-Object -First 10) -join ', '
    Add-ReadyTicket `
        -Category 'UI / Lua Globals' `
        -Title "Implement missing Wrath Lua global functions ($($missingGlobals.Count) unique)" `
        -Label 'ready-for-agent' `
        -TargetFiles @('crates/benilla-ui/src/script/', 'crates/benilla-app/src/ui_script/') `
        -Evidence $ev `
        -Prompt "Implement the missing 3.3.5a Lua global functions observed at runtime ($names) in their owning script module under crates/benilla-ui/src/script/ or crates/benilla-app/src/, following the 12340 reference contract."
}

if ($missingMethods.Count -gt 0) {
    $ev = @($missingMethods.GetEnumerator() | Sort-Object Value -Descending | ForEach-Object { "$($_.Key) (x$($_.Value))" })
    $names = @($missingMethods.Keys | Select-Object -First 10) -join ', '
    Add-ReadyTicket `
        -Category 'UI / Widget Methods' `
        -Title "Implement missing FrameXML widget methods ($($missingMethods.Count) unique)" `
        -Label 'ready-for-agent' `
        -TargetFiles @('crates/benilla-ui/src/script/widget/') `
        -Evidence $ev `
        -Prompt "Implement the missing FrameXML widget methods ($names) on the appropriate widget tables in crates/benilla-ui/src/script/widget/ against the 12340 reference."
}

if ($missingFields.Count -gt 0) {
    $ev = @($missingFields.GetEnumerator() | Sort-Object Value -Descending | ForEach-Object { "$($_.Key) (x$($_.Value))" })
    $names = @($missingFields.Keys | Select-Object -First 10) -join ', '
    Add-ReadyTicket `
        -Category 'UI / Frame & Table Fields' `
        -Title "Resolve nil field/global indexing in stock UI or addons ($($missingFields.Count) unique)" `
        -Label 'ready-for-agent' `
        -TargetFiles @('crates/benilla-ui/src/script/', 'crates/benilla-app/src/ui_script/') `
        -Evidence $ev `
        -Prompt "Trace and fix the nil field/global accesses ($names) in crates/benilla-ui/src/script/ or crates/benilla-app/src/ui_script/ using the logged Lua stack lines."
}

if ($missingCvars.Count -gt 0) {
    $ev = @($missingCvars.GetEnumerator() | Sort-Object Value -Descending | ForEach-Object { "$($_.Key) (x$($_.Value))" })
    $names = @($missingCvars.Keys | Select-Object -First 10) -join ', '
    Add-ReadyTicket `
        -Category 'CVars' `
        -Title "Register missing 3.3.5a CVars ($($missingCvars.Count) unique)" `
        -Label 'ready-for-agent' `
        -TargetFiles @('crates/benilla-app/src/cvars/') `
        -Evidence $ev `
        -Prompt "Register the missing 3.3.5a CVars ($names) with their stock 12340 default values in crates/benilla-app/src/cvars/."
}

if ($luaErrors.Count -gt 0) {
    $ev = @($luaErrors.GetEnumerator() | Sort-Object Value -Descending | ForEach-Object { "$($_.Key) (x$($_.Value))" })
    Add-ReadyTicket `
        -Category 'UI / Lua Runtime' `
        -Title "Fix Lua / FrameXML runtime errors ($($luaErrors.Count) unique)" `
        -Label 'ready-for-agent' `
        -TargetFiles @('crates/benilla-ui/src/script/', 'crates/benilla-app/src/ui_script/') `
        -Evidence $ev `
        -Prompt "Fix the logged Lua/FrameXML runtime errors in crates/benilla-ui or crates/benilla-app by inspecting the exact file:line callers in the diagnostic report."
}

if ($dbcFailures.Count -gt 0) {
    $ev = @($dbcFailures.GetEnumerator() | Sort-Object Value -Descending | ForEach-Object { "$($_.Key) (x$($_.Value))" })
    Add-ReadyTicket `
        -Category 'DBC / Formats' `
        -Title "Fix DBC schema / record layout mismatches ($($dbcFailures.Count) unique)" `
        -Label 'ready-for-agent' `
        -TargetFiles @('crates/benilla-formats/src/dbc.rs', 'crates/benilla-app/src/') `
        -Evidence $ev `
        -Prompt "Repair the 3.3.5a (12340) DBC record reader / field count mismatches reported in the diagnostic log."
}

if ($assetFailures.Count -gt 0) {
    $ev = @($assetFailures.GetEnumerator() | Sort-Object Value -Descending | ForEach-Object { "$($_.Key) (x$($_.Value))" })
    Add-ReadyTicket `
        -Category 'Assets / M2 / BLP' `
        -Title "Fix M2 / Skin / BLP / WMO asset load failures ($($assetFailures.Count) unique)" `
        -Label 'ready-for-agent' `
        -TargetFiles @('crates/benilla-formats/src/', 'crates/benilla-assets/src/') `
        -Evidence $ev `
        -Prompt "Investigate and fix the M2/Skin/BLP/WMO asset load failures recorded in the diagnostic log."
}

# Flight-recorder incident & correlation tickets
if ($snapshotHeader) {
    $incidentLines = New-Object System.Collections.Generic.List[string]
    if ($snapshotHeader.PSObject.Properties['incidents'] -and $snapshotHeader.incidents) {
        foreach ($inc in $snapshotHeader.incidents) {
            $rec = $inc.record
            if ($rec) {
                $stageJson = ($rec.terminal_stage | ConvertTo-Json -Compress)
                $subjJson = ($rec.subject | ConvertTo-Json -Compress)
                $incidentLines.Add("outcome=$($rec.outcome) confidence=$($rec.confidence) stage=$stageJson subject=$subjJson")
            }
        }
    }
    if ($incidentLines.Count -gt 0) {
        Add-ReadyTicket `
            -Category 'Flight Recorder / Incidents' `
            -Title "Triage and resolve $($incidentLines.Count) recorded interaction incident(s)" `
            -Label 'ready-for-agent' `
            -TargetFiles @('crates/benilla-app/src/quest_trace.rs', 'crates/benilla-app/src/wrath.rs', 'crates/benilla-app/src/ui_quest/') `
            -Evidence @($incidentLines.ToArray()) `
            -Prompt "Inspect the flight-recorder incidents in latest-diagnostics.md (from $snapshotPath), trace the terminal_stage and action chain, and fix any client-side refusal, decode rejection, or unconfirmed write defect."
    }

    $questIssues = New-Object System.Collections.Generic.List[string]
    foreach ($prop in @('slot_correlations', 'turn_in_correlations', 'panel_correlations', 'quest_ui_correlations')) {
        if ($snapshotHeader.PSObject.Properties[$prop] -and $snapshotHeader.$prop) {
            foreach ($line in $snapshotHeader.$prop) {
                if ($line -match 'mismatch|unverified|incomplete|ambiguous|refused|failed|without a retained') {
                    $questIssues.Add("[$prop] $line")
                }
            }
        }
    }
    if ($questIssues.Count -gt 0) {
        Add-ReadyTicket `
            -Category 'Quests / Correlations' `
            -Title "Investigate $($questIssues.Count) quest/journal/turn-in correlation anomaly(ies)" `
            -Label 'ready-for-agent' `
            -TargetFiles @('crates/benilla-app/src/ui_quest/', 'crates/benilla-app/src/ui_quest_log.rs', 'crates/benilla-protocol-wrath/src/gameplay.rs') `
            -Evidence @($questIssues.ToArray()) `
            -Prompt "Review the quest/journal/turn-in correlation anomalies captured by the flight recorder and fix any projection or packet-handling gap in crates/benilla-app/src/ui_quest/ or ui_quest_log.rs."
    }

    $castLootIssues = New-Object System.Collections.Generic.List[string]
    foreach ($prop in @('cast_correlations', 'loot_vendor_correlations')) {
        if ($snapshotHeader.PSObject.Properties[$prop] -and $snapshotHeader.$prop) {
            foreach ($line in $snapshotHeader.$prop) {
                if ($line -match 'Failed|Refusal|unverified|ambiguous|unavailable|rejected|error') {
                    $castLootIssues.Add("[$prop] $line")
                }
            }
        }
    }
    if ($castLootIssues.Count -gt 0) {
        Add-ReadyTicket `
            -Category 'Combat / Cast / Loot / Vendor' `
            -Title "Investigate $($castLootIssues.Count) cast or loot/vendor correlation anomaly(ies)" `
            -Label 'ready-for-agent' `
            -TargetFiles @('crates/benilla-app/src/spell/', 'crates/benilla-app/src/ui_loot/', 'crates/benilla-app/src/ui_merchant/', 'crates/benilla-app/src/wrath.rs') `
            -Evidence @($castLootIssues.ToArray()) `
            -Prompt "Inspect the cast and loot/vendor correlation anomalies from the flight recorder and repair any client-side state or protocol mismatch."
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

# Build dedicated Ready-to-Resolve Tickets markdown
$ticketsReport = New-Object System.Text.StringBuilder
[void]$ticketsReport.AppendLine("# RustWrath Prepped Issue Queue (Auto-Generated)")
[void]$ticketsReport.AppendLine("- **Run Directory:** ``$RunDir``")
if ($snapshotPath) {
    [void]$ticketsReport.AppendLine("- **Flight Recorder Snapshot:** ``$snapshotPath``")
} else {
    [void]$ticketsReport.AppendLine("- **Flight Recorder Snapshot:** None found (close the game cleanly or click 'Save local report' in Ctrl+Shift+D)")
}
[void]$ticketsReport.AppendLine("- **Generated:** $(Get-Date -Format o)")
[void]$ticketsReport.AppendLine("- **Total Prepped Tickets:** $($readyTickets.Count)")
[void]$ticketsReport.AppendLine()

if ($readyTickets.Count -eq 0) {
    [void]$ticketsReport.AppendLine("No automatic error tickets were triggered in this run.")
} else {
    foreach ($t in $readyTickets) {
        [void]$ticketsReport.AppendLine("## [$($t.id)] $($t.title)")
        [void]$ticketsReport.AppendLine("- **Category:** $($t.category)")
        [void]$ticketsReport.AppendLine("- **Triage Label:** ``$($t.label)``")
        [void]$ticketsReport.AppendLine("- **Target Files:** " + (($t.targetFiles | ForEach-Object { "``$_``" }) -join ', '))
        [void]$ticketsReport.AppendLine("- **Agent Prompt:**")
        [void]$ticketsReport.AppendLine("  > $($t.prompt)")
        [void]$ticketsReport.AppendLine("- **Captured Evidence:**")
        foreach ($evLine in $t.evidence) {
            [void]$ticketsReport.AppendLine("  - ``$evLine``")
        }
        [void]$ticketsReport.AppendLine()
    }
}
Set-Content -LiteralPath $ticketsPath -Value $ticketsReport.ToString() -Encoding UTF8

# Build comprehensive Diagnostic Report
$report = New-Object System.Text.StringBuilder
[void]$report.AppendLine("# RustWrath In-Game Diagnostic & Triage Report")
[void]$report.AppendLine("- **Run Directory:** ``$RunDir``")
if ($snapshotPath) {
    [void]$report.AppendLine("- **Flight Recorder Snapshot:** ``$snapshotPath`` (run_id: ``$($snapshotHeader.run_id)``, received: $($snapshotHeader.received), dropped: $($snapshotHeader.dropped))")
} else {
    [void]$report.AppendLine("- **Flight Recorder Snapshot:** None found")
}
[void]$report.AppendLine("- **Prepped Tickets File:** ``$ticketsPath`` ($($readyTickets.Count) ready)")
[void]$report.AppendLine("- **Generated:** $(Get-Date -Format o)")
[void]$report.AppendLine("- **Total Log Lines Scanned:** $($lines.Count)")
[void]$report.AppendLine()
[void]$report.AppendLine('## Coverage and limits')
[void]$report.AppendLine('This report combines stdout/stderr log scanning with the in-game InteractionDiagnostics flight recorder. A loaded UI is not proof that its controls work. Requests, rejections and guarded features need triage; they are not all client bugs. Missing geometry, wrong textures, invisible loot and unresponsive controls can be silent. Record those as observations with screenshots and reproduction steps.')
[void]$report.AppendLine()
[void]$report.AppendLine('## Prepped Resolution Tickets Summary')
if ($readyTickets.Count -eq 0) {
    [void]$report.AppendLine('- No automatic failure tickets triggered.')
} else {
    [void]$report.AppendLine('| Ticket | Category | Title | Target Files |')
    [void]$report.AppendLine('| --- | --- | --- | --- |')
    foreach ($t in $readyTickets) {
        $filesStr = ($t.targetFiles | ForEach-Object { "``$_``" }) -join ', '
        [void]$report.AppendLine("| **$($t.id)** | $($t.category) | $($t.title) | $filesStr |")
    }
}
[void]$report.AppendLine()
[void]$report.AppendLine('| Area | Unique diagnostic records | Owning Modules | Acceptance |')
[void]$report.AppendLine('| --- | ---: | --- | --- |')
foreach ($domain in $domains.Keys) {
    $count = @($findings | Where-Object { $_.categories -contains $domain }).Count
    $mods = ($domainSourceMap[$domain] | ForEach-Object { "``$_``" }) -join ', '
    [void]$report.AppendLine("| $domain | $count | $mods | Unverified by log scan |")
}
[void]$report.AppendLine()

if ($snapshotHeader) {
    [void]$report.AppendLine('## In-Game Flight Recorder Summary (InteractionDiagnostics)')
    [void]$report.AppendLine("- **Schema:** ``$($snapshotHeader.schema)`` v$($snapshotHeader.version) | **Mode:** ``$($snapshotHeader.protocol_mode)`` | **Build SHA:** ``$($snapshotHeader.build_git_sha)``")
    [void]$report.AppendLine("- **Events:** received=$($snapshotHeader.received), evicted=$($snapshotHeader.evicted), dropped=$($snapshotHeader.dropped), upstream_gaps=$($snapshotHeader.upstream_gap_events)")
    [void]$report.AppendLine()
    foreach ($sectionProp in @(
        @{ Name = 'Recorded Incidents'; Prop = 'incidents'; IsIncident = $true },
        @{ Name = 'Action Summaries'; Prop = 'action_summaries'; IsIncident = $false },
        @{ Name = 'Quest Panel Correlations'; Prop = 'panel_correlations'; IsIncident = $false },
        @{ Name = 'Quest Slot Correlations'; Prop = 'slot_correlations'; IsIncident = $false },
        @{ Name = 'Quest Turn-In Correlations'; Prop = 'turn_in_correlations'; IsIncident = $false },
        @{ Name = 'Quest UI Correlations'; Prop = 'quest_ui_correlations'; IsIncident = $false },
        @{ Name = 'UI Error Correlations'; Prop = 'ui_error_correlations'; IsIncident = $false },
        @{ Name = 'UI Click Outcomes'; Prop = 'ui_click_outcomes'; IsIncident = $false },
        @{ Name = 'Cast Correlations'; Prop = 'cast_correlations'; IsIncident = $false },
        @{ Name = 'Loot & Vendor Correlations'; Prop = 'loot_vendor_correlations'; IsIncident = $false },
        @{ Name = 'Session Transitions'; Prop = 'session_transition_summaries'; IsIncident = $false },
        @{ Name = 'Hotpatch Attempts'; Prop = 'hotpatch_attempts'; IsIncident = $false }
    )) {
        $propName = $sectionProp.Prop
        if ($snapshotHeader.PSObject.Properties[$propName] -and @($snapshotHeader.$propName).Count -gt 0) {
            [void]$report.AppendLine("### Flight Recorder: $($sectionProp.Name)")
            foreach ($item in $snapshotHeader.$propName) {
                if ($sectionProp.IsIncident) {
                    $rec = $item.record
                    $stageStr = ($rec.terminal_stage | ConvertTo-Json -Compress)
                    [void]$report.AppendLine("- ``outcome=$($rec.outcome) confidence=$($rec.confidence) interaction=$($rec.subject.interaction) op=$($rec.subject.operation) quest=$($rec.subject.quest) stage=$stageStr``")
                } else {
                    [void]$report.AppendLine("- ``$item``")
                }
            }
            [void]$report.AppendLine()
        }
    }
}

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
    schemaVersion          = 2
    runDirectory           = $RunDir
    flightRecorderSnapshot = $snapshotPath
    generated              = (Get-Date -Format o)
    scannedLines           = $lines.Count
    readyTickets           = @($readyTickets.ToArray())
    findings               = @($findings.ToArray())
    observations           = $observations
    acceptance             = 'Unverified: log evidence, flight-recorder correlations, and human reports only'
} | ConvertTo-Json -Depth 10 | Set-Content -LiteralPath $jsonPath -Encoding UTF8

Write-Host $reportText
Write-Host "Saved diagnostic report to: $OutFile" -ForegroundColor Green
Write-Host "Saved prepped repair tickets to: $ticketsPath" -ForegroundColor Cyan
Write-Host "Saved machine-readable triage JSON to: $jsonPath" -ForegroundColor Cyan
