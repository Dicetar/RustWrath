# Owner-run only. Reads generated reference tables; never writes to the reference.
[CmdletBinding()]
param(
    [string]$Extracted = 'D:\Projects\elden-ring-combat-rewrite-reference\src\sim\extracted.rs',
    [string]$Importer = '',
    [string]$Output = ''
)
$ErrorActionPreference = 'Stop'
$root = Split-Path $PSScriptRoot -Parent
if (!$Importer) { $Importer = Join-Path $root 'target\debug\import_kit.exe' }
if (!$Output) { $Output = Join-Path $root 'benilla-config\combat\kits' }
if (!(Test-Path -LiteralPath $Importer -PathType Leaf)) { throw "Build import_kit first: $Importer" }
if (!(Test-Path -LiteralPath $Extracted -PathType Leaf)) { throw "Missing generated table: $Extracted" }
$rows = @(Import-Csv -LiteralPath (Join-Path $PSScriptRoot 'combat-kits.csv'))
foreach ($row in $rows) {
    if ($row.name -notmatch '^[a-z-]+$' -or $row.grip -notin @('one', 'two')) {
        throw "Invalid kit table row: $($row.name)"
    }
}
New-Item -ItemType Directory -Force -Path $Output | Out-Null
foreach ($row in $rows) {
    $offHand = @()
    if ($row.grip -eq 'one') { $offHand += '--off-hand' }
    & $Importer --extracted $Extracted --weapon $row.weapon --grip $row.grip @offHand --out (Join-Path $Output "$($row.name).kit")
    if ($LASTEXITCODE -ne 0) { throw "import_kit failed for $($row.name): $LASTEXITCODE" }
}
