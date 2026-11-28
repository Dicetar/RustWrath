# P0 Windows private-alpha packaging and first launch

Status: in progress
Owner: OpenCode / DeepSeek V4.1 Flash
Branch: fix/windows-alpha-launch
Issue: none
Next: reconcile the launcher, assembled package and launch record against the source; write the tester procedure and evidence handoff

Field: P0 Windows packaging and first launch (F17 / W1), manual distribution only.
Scope: `Launch RustWrath.cmd`, `tools/launch-client.ps1`,
`tools/assemble-windows-release.ps1`, the two profile manifests and the launch docs.
Out of scope: automated update/repair, release-set recovery redesign, gameplay, any build,
client, server or test run.

## What exists now

Revalidated against `origin/main` at `aa8f4a9c7` after fetching. The earlier review's
two findings are both still true in source:

- `Launch RustWrath.cmd` passes `-BuildLatest`; in a checkout the launcher builds
  `target/debug/benilla.exe` before starting it. `docs/launching.md` says it does not
  rebuild. `docs/agents/local-launch.md` already describes the build correctly.
- `tools/launch-client.ps1` declares `[bool]$WithAddons = $true`. The default loads the
  supplied data root's sibling `Interface\AddOns`; the shadow data root is used only
  when `-WithAddons $false`. The guide describes the shadow as the default and says
  "pass -WithAddons" to use the data root, which is already the default.

The assembled package keeps an immutable copy of the launcher and manifest, selects
`bin/benilla.exe`, prompts for Data when `-DataPath` is not bound, and validates the five
listed files. Its `launch.txt` currently records only `pid` and `binary_sha256`; the
package version, manifest hash, asset profile/manifest, effective addon mode and endpoints
are not in the record.

The selected `wm-hd` manifest is a hard archive list. `benilla_profile::Profile::open`
fails at boot when any named archive is absent, so a tester with a stock 3.3.5a Data folder
cannot boot the current package even though `docs/profiles/base.mpqlist` exists.

## Next step

Make the profile and addon choices explicit, preflight the selected data profile with a
name-the-archive failure, copy both profile manifests into the package, extend the launch
record to the candidate identity fields, and update the launch guide to the shipped
shortcut and the exact second-machine PowerShell command.

## Verification

Source inspection only. No build, client, server or test run. A second-machine first launch
remains unproven.
