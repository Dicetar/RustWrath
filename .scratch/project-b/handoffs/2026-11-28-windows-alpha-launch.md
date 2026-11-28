# P0 Windows private-alpha packaging and first launch

Status: ready to merge
Owner: OpenCode / DeepSeek V4.1 Flash
Branch: fix/windows-alpha-launch
Issue: none
Next: read the final CI run; second-machine launch remains owner/tester work

Field: P0 Windows packaging and first launch (F17 / W1), manual distribution only.
Scope: `Launch RustWrath.cmd`, `tools/launch-client.ps1`,
`tools/assemble-windows-release.ps1`, the two profile manifests and the launch docs.
Out of scope: automated update/repair, release-set recovery redesign, gameplay, any build,
client, server or test run.

## Revalidated findings

Fetched `origin`; based on `origin/main` `aa8f4a9c7`. The earlier review's two findings were
still true in source:

- `Launch RustWrath.cmd` passes `-BuildLatest`; a checkout launch builds
  `target/debug/benilla.exe` first. `docs/launching.md` said it did not rebuild.
- `tools/launch-client.ps1` declares `[bool]$WithAddons = $true`. The default loads the
  supplied data root's sibling `Interface\AddOns`; the shadow data root is used only with
  `-WithAddons $false`. The guide described the shadow as the default and said "pass
  -WithAddons", which is both already the default and invalid for a `[bool]` parameter without
  a value.

A second machine with the package but the owner's layout absent has no `D:\Projects\wow`
Data default in release mode (it prompts), no shadow, no local auth/world and no Cargo. The
package's selected `wm-hd` manifest is a hard archive list: `benilla_profile::Profile::open`
fails the boot when any named archive is absent. A stock Data folder therefore could not boot
the package even though `docs/profiles/base.mpqlist` existed.

## What changed

- `tools/launch-client.ps1`:
  - `-AssetProfile` selects `base` or `wm-hd` (default `wm-hd`); the selected profile file is
    required and hashed. Existing checkout behaviour is unchanged with the default.
  - A Data preflight reads the selected `.mpqlist` and names missing or `.mpq.disabled`
    archives before the client starts. It checks presence, not MPQ contents.
  - The effective data root, addon mode/path, UI mode, endpoints, state root and candidate
    identity are written to `launch.txt`; credentials are never read into it.
  - An assembled release runs from the state root, not from the immutable package directory.
  - A package-shaped directory with no `release-manifest.json` is refused as an interrupted
    assembly.
- `tools/assemble-windows-release.ps1` copies both profile manifests; the release manifest now
  enumerates six files.
- `docs/launching.md`: shortcut rebuild corrected; addon default and shadow limits corrected;
  `-AssetProfile` documented; one exact second-machine procedure with the remote auth/world
  command and the advertised-address rule.
- `docs/architecture/windows-release-assembly.md`: six-file package, profile selection,
  preflight, launch-record fields, state-root working directory, the `-BuildRelease` `dev`
  caveat, and the unrun second-machine cases.

## `launch.txt` identity fields

`mode`, `started_utc`, `pid`, `candidate_dir`, `binary`, `binary_sha256`,
`package_version`, `package_manifest_sha256`, `asset_profile`, `asset_manifest`,
`asset_manifest_sha256`, `data_input`, `data_effective`, `manifest_archives`,
`addon_mode`, `addons_path`, `ui_mode`, `movement_hotpatch`, `multiclass`,
`auth_server`, `world_server`, `world_override`, `state_home`, `logs`, `run_dir`.
Release-set launches retain their existing release/selector/health fields.

## Verification

- Source inspection of the shortcut, launcher, assembly script, profile loader
  (`crates/benilla-profile/src/lib.rs`), client asset registration, addon roots
  (`crates/benilla-app/src/ui_script/addons.rs`), endpoint selection
  (`crates/benilla-app/src/wrath.rs`), state resolution and diagnostics collector.
- Static PowerShell parse of both changed scripts: 0 errors. No execution of either script.
- Read-only check of this workstation's HD Data folder: `base` missing=0, `wm-hd` missing=0;
  the sibling `Interface\AddOns` exists with 73 addon directories.
- No Cargo build, client launch, test suite, server action or second-machine access.
- CI: not run yet. The branch's single code push is the final push for this task.

## Not established

- A clean or second Windows machine has not run the candidate; the remote advertised-address
  path and the explicit world override are source-established only.
- The Microsoft Visual C++ 2015-2022 runtime requirement is inferred from the MSVC target and is
  not bundled; a machine without it is unverified.
- A read-only candidate directory, SmartScreen/MOTW handling, firewall/NAT and the signed-in
  server's actual realmlist address are unverified.
- `-BuildRelease` builds with `--features hotpatch`; `hotpatch` enables `dev`, so the candidate
  is not the `--no-default-features` player build. This is recorded as an owner decision, not
  changed here.
- `-WithAddons $false` is not a no-addons sandbox: the client still discovers addons when
  `WOW_ADDONS_PATH` is unset. The tester path uses the explicit default corpus.
- The packaged launcher still passes `-BuildLatest`; release mode skips the build. The shortcut
  has no endpoint fields, so a remote tester must use the PowerShell procedure.
- An assembled `release-manifest.json` is not self-authenticated: editing only its `version` or
  `format` is not refused. `package_manifest_sha256` in `launch.txt` records what actually ran;
  the release-set path has a selector hash, the assembled package does not.

## Coordination

- `fix/wrath-addon-drop-in` (Codex, unpushed, deferred by the 2026-11-28 integration baseline)
  also edits `tools/launch-client.ps1` with `-AddOnsPath`/`-AddonCompatibleBags`. If it lands,
  rebase its launcher hunk on this branch rather than overwriting the profile/preflight/record
  changes.
- `fix/rustwrath-p0-session-entry-reconnect` owns the session entry/reconnect path; this branch
  only changes launch/packaging identity and does not touch that code.

## Tester procedure

[The launch guide](../../../docs/launching.md#private-alpha-on-a-second-windows-machine) holds the exact
command. Summary: copy the candidate intact; provide the matching `Data` folder and sibling
`Interface\AddOns`; run `tools\launch-client.ps1` with `-DataPath`, `-AssetProfile wm-hd`
(or `base`), `-WithAddons $true`, `-AuthServer host:3724` and either `-WorldServer ''`
(realm-advertised) or an explicit reachable `host:8095`; enter credentials in the game window;
close normally; copy the run directory, `latest-diagnostics.md` and `latest-tickets.md`.

## Handoff to Claude and ClaudeJr

Every gameplay evidence item should carry these candidate identity fields, copied from
`launch.txt` and the run logs:

| Field | Source | Why |
|---|---|---|
| `mode`, `package_version`, `candidate_dir` | `launch.txt` | Names the exact candidate folder and package label |
| `binary_sha256` | `launch.txt` | Exact executable; a rebuild changes it |
| first `benilla build ...` line | `stdout.log` | Version, short commit, date, profile, extended flag |
| `package_manifest_sha256` | `launch.txt` | Exact file inventory; changes if any packaged file changes |
| `asset_profile`, `asset_manifest_sha256` | `launch.txt` | Archive precedence actually loaded |
| `data_effective`, `data_input` | `launch.txt` | Which data root was used (redact local paths when sharing) |
| `addon_mode`, `addons_path` | `launch.txt` | Explicit corpus versus client discovery |
| addon folder listing / TOC versions | the directory at `addons_path` | Exact loose addon set; UI/addon evidence is not comparable without it |
| `ui_mode`, `movement_hotpatch`, `multiclass` | `launch.txt` | Which UI/extension layer was active |
| `auth_server`, `world_server`, `world_override` | `launch.txt` | Logon endpoint and explicit/advertised world choice |
| advertised world address | server realmlist (or the client's connection error log) | The address actually used when `world_server=realm-advertised` |
| `run_dir` / PID / `started_utc` | `launch.txt` | The one session and its logs |
| flight-recorder header | `latest-diagnostics.md` / snapshot | `run_id`, `build_git_sha`, `protocol_mode` |
| realm and character name | player observation | Reproduces the roster/world context; never the password |
| Windows version, GPU and driver | tester observation | Rendering and launch findings are not comparable without them |

Do not include passwords, session keys, account credentials or unredacted local paths in a
public report. Pair each finding with the exact step/time and the relevant log line or
screenshot.

## Next step

Commit the changed source, docs and this note, merge `origin/main` once if it moved, push once,
and read the single CI run. This is a source-only handoff until a second machine runs the
procedure.
