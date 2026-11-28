# Windows release assembly: W1 source handoff

This implements the first delivery slice in the [release backlog](https://github.com/Dicetar/RustWrath/blob/838b03f89a5d32271ef6bec5cb9674773595c767/docs/architecture/release-backlog-2026-11-05.md):
assemble a version-specific directory from an existing binary and run it with a state directory
independent of that version. It is **source work, not a validated installer or a release gate pass**.
No game archives, addon files, credentials, server data or player settings are included.

`tools/assemble-windows-release.ps1` accepts an already built binary, an absolute destination
that does not exist and a version label. It copies the binary, launcher, collector, launcher
shortcut and both asset-profile manifests (`base.mpqlist` and `wm-hd.mpqlist`), verifies each
copy against its source, then writes a versioned SHA-256 inventory. It never overwrites an
existing candidate or deletes an incomplete one. The caller must select the correct binary and
inspect and approve any distribution inputs.

When a release manifest exists, `tools/launch-client.ps1` selects `bin/benilla.exe`, checks the
six expected files against the manifest and prompts for an existing game Data folder. It selects
the archive chain with `-AssetProfile` (default `wm-hd`, optional `base`), reads
`docs/profiles/<profile>.mpqlist`, and checks that every archive the manifest names exists in the
effective data root (the supplied Data folder, or the workstation shadow when
`-WithAddons $false` selects it) before starting the binary. It sets `BENILLA_HOME` to
`%LOCALAPPDATA%\RustWrath\benilla-config`, or the absolute `-StateHome` path, before starting the
binary, and runs the client from that state root rather than from the immutable candidate
directory. Launcher and diagnostic collector logs use the same directory. The ordinary checkout
path continues to use `target/debug/benilla.exe` and the previous defaults.

The launch record (`launch.txt` in the run directory) now carries the mode, candidate directory,
executable path and SHA-256, package version and `release-manifest.json` SHA-256, selected asset
profile and manifest SHA-256, input and effective data roots, checked manifest archive count,
addon mode and explicit corpus path, UI mode, movement/multiclass flags, auth and world
endpoints, and state root. It never records credentials. The manifest detects inconsistent
files; **it is not authenticated**, and neither the launcher nor the manifest establishes who
supplied a binary or whether it is compatible with a server. An assembled manifest has no stored
expected hash of itself: editing only its `version` or `format` is not detected. The release-set
path has a selector manifest hash; this package path does not.

Example for an authorized release operator, after independently producing the binary:

```powershell
.\tools\assemble-windows-release.ps1 -Binary 'C:\build\benilla.exe' -Destination 'C:\RustWrath\versions\candidate-1' -Version 'candidate-1'
```

`-BuildRelease` can build the binary first, but it uses
`cargo build --release -p benilla --features hotpatch`. `hotpatch` enables the `dev` feature,
so that convenience is a hotpatch-enabled build, not the `--no-default-features` player build.
The owner must choose whether that is acceptable for the alpha; do not describe it as a
player-only build.

Run the resulting `Launch RustWrath.cmd` interactively for a same-machine package. It uses the
local endpoint defaults. For a second machine with a remote realm, use the exact PowerShell
command in [the launch guide](../launching.md#private-alpha-on-a-second-windows-machine); the
double-click shortcut has no endpoint fields. On another machine the operator supplies the game's
existing Data directory. A different existing state folder can be used from PowerShell via
`tools\launch-client.ps1 -StateHome <absolute path>`; this permits manual, owner-controlled
migration without silently copying or rewriting the original settings. Character data remains on
the server. Each version must be retained separately by the operator.

Failure before manifest completion leaves an incomplete, unselected directory. The launcher
also refuses a package-shaped directory that has `tools/launch-client.ps1` and no `Cargo.toml`
but no `release-manifest.json`, so a tester does not mistake an interrupted assembly for a
candidate. A missing or modified listed file makes a manifest-bearing release refuse launch; the
previous version and user data are not touched. The Data preflight checks archive presence only;
it does not validate MPQ contents, and it cannot prove the archives are the ones a server
expects. This source does
not provide an active-version selector, repair, update authentication, disk-space preflight,
atomic promotion, health confirmation, automatic rollback, state migration or server/schema
rollback. Those are W2/W3 work, not implied by this assembly tool. Release state paths must not
be aliases into a version directory; the lexical check does not resolve reparse points. The
candidate does not bundle the Microsoft Visual C++ Redistributable, a GPU driver or game data.

No build, test, installer, client or server session was run for the original assembly change.
The 2026-11-28 profile selection, data preflight and launch-record change was also source-only:
the two PowerShell scripts passed a syntax parse, and a read-only check confirmed this
workstation's HD Data folder satisfies both shipped profiles. Before release, authorize and
record Windows cases for clean assembly and launch; missing/modified binary and manifest;
missing/wrong game data; state path with spaces; custom state and existing settings; two retained
versions using the same state; interrupted assembly; offline launch; and a manual return to the
previous version. A second-machine first launch, the remote world-address override, a clean
machine without the Visual C++ runtime, and a read-only candidate directory remain unrun.
Separately implement and exercise authenticated manifests, trusted version selection, crash
recovery, migrations and the installation/upgrade/rollback gates in the backlog. The owner
must decide the release-signing and server/schema compatibility policy before unattended
activation is safe.
