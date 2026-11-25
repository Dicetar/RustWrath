param(
    [Parameter(Mandatory = $true)][string]$File,
    [string]$Out = (Join-Path $PSScriptRoot '..\reference\lua_globals_30300.tsv')
)
$ErrorActionPreference = 'Stop'
if (!(Test-Path -LiteralPath $File)) { throw "capture file not found: $File" }
$rows = New-Object System.Collections.Generic.List[string]
foreach ($line in Get-Content -LiteralPath $File) {
    if ($line -match '^\s*"((?:[^"\\]|\\.)*)\\t([A-Za-z]+)",?\s*$') {
        $rows.Add((@($Matches[1], $Matches[2], 'capture') -join [char]9))
    }
}
if (!$rows.Count) { throw "no rows parsed from $File; is it the RustWrathGlobalsCapture SavedVariables file?" }
$sorted = $rows | Sort-Object -Unique
$header = @(
    '# 3.3.5a _G captured from the stock 12340 client',
    (@('name', 'type', 'origin') -join [char]9)
)
$header + $sorted | Set-Content -LiteralPath $Out -Encoding utf8
Write-Host ("wrote {0} names to {1}" -f $sorted.Count, $Out)
