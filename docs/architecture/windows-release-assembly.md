# Windows release assembly: W1 source handoff

This implements the first delivery slice in the [release backlog](release-backlog-2026-11-05.md):
assemble a version-specific directory from an existing binary and run it with a state directory
independent of that version. It is **source work, not a validated installer or a release gate pass**.
No game archives, addon files, credentials, server data or player settings are included.

`tools/assemble-windows-release.ps1` accepts an already built binary, an absolute destination
that does not exist and a version label. It copies the binary, launcher, collector, launcher
shortcut and WM asset manifest, verifies each copy against its source, then writes a versioned
SHA-256 inventory. It never overwrites an existing candidate or deletes an incomplete one.
The caller must select the correct binary and inspect and approve any distribution inputs.

When a release manifest exists, `tools/launch-client.ps1` selects `bin/benilla.exe`, checks the
five expected files against the manifest and prompts for an existing game Data folder. It sets
`BENILLA_HOME` to `%LOCALAPPDATA%\RustWrath\benilla-config`, or the absolute `-StateHome` path,
before starting the binary. Launcher and diagnostic collector logs use that same directory.
The ordinary checkout path continues to use `target/debug/benilla.exe` and the previous defaults.
The manifest detects inconsistent files; **it is not authenticated**, and neither the launcher
nor the manifest establishes who supplied a binary or whether it is compatible with a server.

Example for an authorized release operator, after independently producing the binary:

```powershell
.\tools\assemble-windows-release.ps1 -Binary 'C:\build\benilla.exe' -Destination 'C:\RustWrath\versions\candidate-1' -Version 'candidate-1'
```

Run the resulting `Launch RustWrath.cmd` interactively. On another machine the operator supplies
the game's existing Data directory when prompted. A different existing state folder can be used
from PowerShell via `tools\launch-client.ps1 -StateHome <absolute path>`; this permits manual,
owner-controlled migration without silently copying or rewriting the original settings.
Character data remains on the server. Each version must be retained separately by the operator.

Failure before manifest completion leaves an incomplete, unselected directory. A missing or
modified listed file makes a manifest-bearing release refuse launch; the previous version and
user data are not touched. This source does not provide an active-version selector, repair,
update authentication, disk-space preflight, atomic promotion, health confirmation, automatic
rollback, state migration or server/schema rollback. Those are W2/W3 work, not implied by this
assembly tool. Release state paths must not be aliases into a version directory; the lexical
check does not resolve reparse points.

No build, test, installer, client or server session was run for this change. Before release,
authorize and record Windows cases for clean assembly and launch; missing/modified binary and
manifest; missing/wrong game data; state path with spaces; custom state and existing settings;
two retained versions using the same state; interrupted assembly; offline launch; and a manual
return to the previous version. Separately implement and exercise authenticated manifests,
trusted version selection, crash recovery, migrations and the installation/upgrade/rollback
gates in the backlog. The owner must decide the release-signing and server/schema compatibility
policy before unattended activation is safe.
