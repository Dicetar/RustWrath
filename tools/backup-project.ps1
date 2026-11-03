param(
    [string]$Repository = 'D:\Projects\project-b-client',
    [string]$Destination = 'D:\Projects\RustWrath-backups',
    [ValidateRange(1, 3)][int]$Retain = 3
)

$ErrorActionPreference = 'Stop'
$repoPath = (Resolve-Path -LiteralPath $Repository).Path
$backupRoot = [System.IO.Path]::GetFullPath($Destination).TrimEnd('\', '/')
if ($backupRoot.Equals($repoPath, [StringComparison]::OrdinalIgnoreCase) -or
    $backupRoot.StartsWith($repoPath.TrimEnd('\') + '\', [StringComparison]::OrdinalIgnoreCase)) {
    throw 'Backup destination must be outside the repository.'
}
[void][System.IO.Directory]::CreateDirectory($backupRoot)

function Invoke-RepoGit([string[]]$Arguments) {
    $start = [System.Diagnostics.ProcessStartInfo]::new()
    $start.FileName = (Get-Command git -CommandType Application | Select-Object -First 1).Source
    $start.WorkingDirectory = $repoPath
    $start.UseShellExecute = $false
    $start.CreateNoWindow = $true
    $start.RedirectStandardOutput = $true
    $start.RedirectStandardError = $true
    foreach ($argument in $Arguments) { [void]$start.ArgumentList.Add($argument) }
    $process = [System.Diagnostics.Process]::Start($start)
    $outputTask = $process.StandardOutput.ReadToEndAsync()
    $errorTask = $process.StandardError.ReadToEndAsync()
    $process.WaitForExit()
    $outputText = $outputTask.GetAwaiter().GetResult()
    $errorText = $errorTask.GetAwaiter().GetResult()
    if ($process.ExitCode -ne 0) { throw "git failed: $errorText" }
    $process.Dispose()
    return $outputText
}

function Assert-ManagedChild([string]$Path) {
    $absolute = [System.IO.Path]::GetFullPath($Path)
    if (-not $absolute.StartsWith($backupRoot + '\', [StringComparison]::OrdinalIgnoreCase) -or
        [System.IO.Path]::GetDirectoryName($absolute) -ne $backupRoot) {
        throw "Not a direct child of the backup directory: $absolute"
    }
    if ((Get-Item -LiteralPath $absolute).Attributes -band [IO.FileAttributes]::ReparsePoint) {
        throw "Refusing a linked backup directory: $absolute"
    }
}

$lock = [IO.File]::Open((Join-Path $backupRoot '.backup.lock'), 'OpenOrCreate', 'ReadWrite', 'None')
$stamp = Get-Date -Format 'yyyyMMdd-HHmmss-fff'
$pending = Join-Path $backupRoot ('.pending-' + $stamp)
$complete = Join-Path $backupRoot ('RustWrath-' + $stamp)
try {
    [void][IO.Directory]::CreateDirectory($pending)
    $head = (Invoke-RepoGit @('rev-parse', 'HEAD')).Trim()
    $branch = (Invoke-RepoGit @('branch', '--show-current')).Trim()
    $before = Invoke-RepoGit @('status', '--porcelain=v1', '-z')
    [void](Invoke-RepoGit @('bundle', 'create', (Join-Path $pending 'history.bundle'), '--all'))
    $files = (Invoke-RepoGit @('ls-files', '-z', '--cached', '--others', '--exclude-standard')).Split([char]0) |
        Where-Object { $_ } | Sort-Object -Unique
    $zip = [IO.Compression.ZipFile]::Open((Join-Path $pending 'working-tree.zip'), 'Create')
    $copied = 0
    $skipped = [Collections.Generic.List[string]]::new()
    try {
        foreach ($relative in $files) {
            $relative = $relative.Replace('\', '/')
            if ($relative -match '^(\.git/|\.probe-identity$|benilla-config/|target/|WoW/|extracted/|out/|\.scratch/project-b/verification/)' -or
                $relative -match '(?i)\.(mpq|m2|mdx|skin|anim|blp|dbc|exe|dll|pdb|pem|key)$' -or
                $relative -match '(^|/)\.env($|\.)') {
                $skipped.Add($relative)
                continue
            }
            $source = [IO.Path]::GetFullPath((Join-Path $repoPath $relative))
            if (-not $source.StartsWith($repoPath.TrimEnd('\') + '\', [StringComparison]::OrdinalIgnoreCase)) {
                throw 'Source path escaped repository.'
            }
            if (-not (Test-Path -LiteralPath $source -PathType Leaf)) { $skipped.Add($relative); continue }
            $cursor = $source
            $linked = $false
            while ($cursor -and $cursor -ne $repoPath) {
                if ((Get-Item -LiteralPath $cursor).Attributes -band [IO.FileAttributes]::ReparsePoint) {
                    $linked = $true; break
                }
                $cursor = [IO.Path]::GetDirectoryName($cursor)
            }
            if ($linked) { $skipped.Add($relative); continue }
            [void][IO.Compression.ZipFileExtensions]::CreateEntryFromFile($zip, $source, $relative, 'Fastest')
            $copied++
        }
    } finally { $zip.Dispose() }
    $manifest = [ordered]@{
        format = 'RustWrath-source-backup-v1'; created = (Get-Date -Format o)
        repository = $repoPath; branch = $branch; head = $head; files = $copied
        changedDuringCopy = ($before -ne (Invoke-RepoGit @('status', '--porcelain=v1', '-z')))
        skipped = @($skipped)
    }
    $manifest | ConvertTo-Json -Depth 4 | Set-Content -LiteralPath (Join-Path $pending 'manifest.json')
    @'
Restore into a NEW empty directory, not over a working checkout:
1. git clone -b <branch from manifest.json> <absolute path to history.bundle> <new directory>
2. Extract working-tree.zip over that new checkout to restore saved working files.
3. Compare manifest.head and skipped paths before treating this as an exact working snapshot.
Deleted working files are not in the zip; remove their restored tracked copies if needed.
Credentials, game assets, local state, build outputs and raw run dumps are excluded.
The bundle contains full Git history and refs; the zip contains available tracked/unignored source.
Live working-file copies are best-effort, not an atomic filesystem snapshot.
'@ | Set-Content -LiteralPath (Join-Path $pending 'RESTORE.txt')
    # Keep the status separately for restoration, never print source or credential contents.
    [IO.File]::WriteAllText((Join-Path $pending 'working-status.nul'), $before)
    Move-Item -LiteralPath $pending -Destination $complete
    $backups = @(Get-ChildItem -LiteralPath $backupRoot -Directory |
        Where-Object { $_.Name -match '^RustWrath-\d{8}-\d{6}-\d{3}$' -and
            (Test-Path -LiteralPath (Join-Path $_.FullName 'manifest.json')) } |
        Sort-Object Name -Descending)
    foreach ($old in ($backups | Select-Object -Skip $Retain)) {
        Assert-ManagedChild $old.FullName
        $marker = Get-Content -LiteralPath (Join-Path $old.FullName 'manifest.json') -Raw | ConvertFrom-Json
        if ($marker.format -ne 'RustWrath-source-backup-v1') { throw 'Unrecognized backup marker.' }
        Remove-Item -LiteralPath $old.FullName -Recurse -Force
    }
    $size = (Get-ChildItem -LiteralPath $complete -File | Measure-Object Length -Sum).Sum
    "Backup complete: $complete; files=$copied; bytes=$size; retained=$([Math]::Min($backups.Count,$Retain))"
} finally {
    if (Test-Path -LiteralPath $pending) {
        Assert-ManagedChild $pending
        Remove-Item -LiteralPath $pending -Recurse -Force
    }
    $lock.Dispose()
}
