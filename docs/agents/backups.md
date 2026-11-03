# Workstation source backups

Windows Task Scheduler task `RustWrath source backup every 8 hours` runs
`tools/backup-project.ps1` every eight hours under the current user's interactive logon.
The machine must be running and the user logged in. StartWhenAvailable is enabled for missed
starts; no password is stored. The task does not run Cargo, Git pulls, or client processes.

Backups live outside the checkout in `D:/Projects/RustWrath-backups`. Keep at most three
completed copies total. Each copy contains a full-history/ref Git bundle, a zip of available
tracked and unignored source files including dirty edits, a manifest, working status and
restoration instructions. This is a source/history backup, not a copy of target or the install.
Live working-file capture is best-effort rather than a filesystem-wide atomic snapshot.

Excluded: credentials and .env files, game asset formats, build outputs, local client state,
raw verification dumps and filesystem links. Existing Git history is preserved as-is.
The script prunes only its own marked, directly contained backup directories after a new
backup completes; unrelated files in the destination are left alone.

Manual backup: `pwsh -NoProfile -File tools/backup-project.ps1`.
Inspect scheduling: `Get-ScheduledTaskInfo -TaskName 'RustWrath source backup every 8 hours'`.
Restore into a new directory using the copy's `RESTORE.txt`; never extract over active work.

Treat the schedule above as recorded workstation setup; inspect Task Scheduler before claiming
it is currently healthy. Three is the retention ceiling, not a reason to produce extra copies.

For disk pressure, distinguish source backups, Cargo target/deps/incremental caches, sccache,
launcher logs and linked game data before proposing cleanup. Linked archive sizes can
overstate unique storage. Never delete active build outputs, state, evidence or source as
routine documentation maintenance. For authorized cleanup, resolve targets, verify they
stay inside the intended directory, use one native shell end to end, and respect denied actions.
