# Launching RustWrath on Windows

For the source-only versioned release assembly and its current limits, see
[Windows release assembly](architecture/windows-release-assembly.md). The normal checkout
launch behavior is unchanged; the description below matches the shipped shortcut.

Double-click **Launch RustWrath.cmd** in the repository root. The shortcut passes
`-BuildLatest`: it builds the current checkout with Cargo through sccache and then starts
`target/debug/benilla.exe`; a build failure stops the launch. The client opens its stock login
screen; enter your account and password there, choose the realm, then a character. The launcher
does not read `.probe-identity` or save a password. The local auth/world servers must already be
running. The launcher refuses to compete with an existing client or build of this checkout or
candidate.

From PowerShell, the same entry point is:

```powershell
& 'D:\Projects\wow\client\Launch RustWrath.cmd'
```

No commit, push or pull is required to test the local executable, but the shortcut does
rebuild it. To start the existing `target/debug/benilla.exe` without Cargo, run
`.\tools\launch-client.ps1` without `-BuildLatest`. A clean Git tree does not establish that
the executable is current. Do not pull or switch branches underneath uncommitted shared work.

Build after source changes, with the shared Cargo slot free and no client running:

```powershell
cd D:\Projects\wow\client
cargo build -p benilla --features hotpatch
```

Every Cargo build in this checkout uses sccache. The repository `.cargo/config.toml` sets
`build.rustc-wrapper = "sccache"`, so no shell setup is needed when `sccache` is on `PATH`:

```powershell
sccache --show-stats
cargo build -p benilla --features hotpatch
```

If a shell cannot find the wrapper, install it with the wrapper bypassed for that one command,
or set its full path before Cargo:

```powershell
$env:RUSTC_WRAPPER = ''
cargo install sccache --locked
$env:RUSTC_WRAPPER = 'C:\Users\Qraag\.cargo\bin\sccache.exe'
```

A missing wrapper fails the build; do not bypass it. Selecting the wrapper does not prove a
cache hit or speedup: `sccache --show-stats` shows requests and hits. sccache does not cache
incremental compilations, so set `$env:CARGO_INCREMENTAL = '0'` when a clean build should
populate the cache. The first build only fills it; later clean builds benefit. CI installs
sccache with `mozilla-actions/sccache-action` and uses the GitHub Actions cache backend.
`cargo clippy` may bypass the wrapper (rust-lang/rust-clippy#3840), so clippy runs are not
sccache-cached; build and test runs are.
The launcher uses target/debug/benilla.exe; another build profile does not replace that
executable automatically. Git publication alone does not activate code in a running process.

Launch from PowerShell with another data root or auth server:

```powershell
.\tools\launch-client.ps1 -DataPath 'D:\Projects\wow\server\world of warcraft 3.3.5a hd\data' -AuthServer '127.0.0.1:3724'
```

`-AssetProfile` selects the archive chain: `wm-hd` (the default; the owner's HD patches) or
`base` (stock 3.3.5a enUS archives). The selected `docs/profiles/<profile>.mpqlist` defines
archive precedence, and the launcher checks that every archive it names exists in the effective
data root before starting the client. The check is presence only: a corrupt or unreadable archive
still fails when the client opens it.

`-WithAddons` defaults to `$true` and makes `<game root>\Interface\AddOns` the explicit
addon corpus (`WOW_ADDONS_PATH`). That corpus is authoritative in the client: an explicit folder
suppresses its own install/state discovery. If that folder is missing, the launcher warns and
leaves `WOW_ADDONS_PATH` unset, so the client uses discovery instead; the launch record says
`addon_mode=discovery-install-addons-missing`.

With `-WithAddons $false`, the launcher uses this workstation's linked shadow data root when it
exists (`addon_mode=shadow-data`); otherwise it only omits the explicit corpus
(`addon_mode=discovery-no-explicit-path`). In both cases the client still uses its own discovery,
so this is not a guaranteed no-addons mode. The four recorded modes are `installed-explicit`,
`discovery-install-addons-missing`, `shadow-data` and `discovery-no-explicit-path`. The shadow
is a workstation-only convenience that a second machine does not have.

Logs are under `benilla-config/Logs/Launcher` in a checkout; an assembled release uses
`%LOCALAPPDATA%\RustWrath\benilla-config\Logs\Launcher`. Only three launch generations are
retained. The local world endpoint defaults to `127.0.0.1:8095`; pass `-WorldServer ''` to use the
address advertised by the selected realm instead.
Default launches load benilla's layer: the Wrath-only server windows (the Enchant Bottle) and
the glue realmlist control. `-StockUi` boots the stock interface alone, as a diagnostic.
No unattended probe, automatic character pick, combat action or timeout is enabled.

The intended stock workflow is login, optional realm choice, roster selection and world entry,
with character creation/cancel and deletion confirmation on their respective screens.
These paths remain under active repair; the presence of a button is not acceptance.
Persistent creation/deletion must be driven by the owner. Settings use local CVar persistence;
backend wiring must be verified for each option.

The source now includes an opt-in WM multiclass host. After building the branch,
`.\tools\launch-client.ps1 -WithMulticlass` enables it with the installed
`WM_Multiclass` addon and the local WM Glue script. `-MulticlassGluePath` selects
another script path. This source change has not been built or runtime-accepted;
see the [U4 handoff and acceptance cases](architecture/multiclass-host-2026-11-05.md).
Default launches leave this extension disabled.

## Private alpha on a second Windows machine

The assembled candidate is the directory produced by
[Windows release assembly](architecture/windows-release-assembly.md). It contains no game data,
addons, credentials or player settings. A tester needs:

- the candidate directory, copied intact; do not edit `release-manifest.json` or the profile
  files. The launcher refuses a changed listed file, but an assembled package has no separate
  expected manifest hash, so an edit to only the manifest's version label is not detected;
- a 3.3.5a `Data` folder. The intended `wm-hd` data is the owner's HD install, including every
  `wm` archive named by `docs/profiles/wm-hd.mpqlist`. A plain stock enUS install works with
  `-AssetProfile base`;
- the intended addons in `<game root>\Interface\AddOns` (the default mode), and
- network access to the auth and world ports. The expected candidate is an unsigned MSVC build;
  a clean machine also needs the Microsoft Visual C++ Redistributable 2015-2022. The assembly
  does not bundle it.

The candidate is not signed; if Windows warns, confirm its binary hash against the owner's
record before allowing it. The shortcut has no endpoint fields and uses the local defaults, so do
not use it for a remote realm. Open Windows PowerShell in the candidate directory and run this
exact command, replacing the placeholders. The empty world value asks the client to use the
realm's advertised address:

```powershell
Set-Location 'C:\RustWrath\candidate-1'
& .\tools\launch-client.ps1 `
    -DataPath 'D:\WoW-3.3.5a-HD\Data' `
    -AssetProfile wm-hd `
    -WithAddons $true `
    -AuthServer 'wow.example.net:3724' `
    -WorldServer ''
```

`-AuthServer` is the logon address. `-WorldServer ''` leaves `WOW_WRATH_WORLD` unset, so the
client dials the `address` field of the realm the auth server returns. The launch record marks
that case as `world_server=realm-advertised`; include the server's realmlist address in the
report. If the advertised address is loopback or otherwise unreachable from the tester, pass the
reachable world address instead (still with the port, and without a URL scheme):

```powershell
    -WorldServer 'wow.example.net:8095'
```

If Windows blocks the script by execution policy, start
`powershell.exe -NoProfile -ExecutionPolicy Bypass` and run the same command in that window.
If the realm list advertises `127.0.0.1:8095`, the tester must use that override, or the server's
realmlist must advertise an address the tester can reach. Enter credentials only in the game
window; do not put them on the command line.

The launcher records the candidate identity in `launch.txt` in the newest run directory under
`%LOCALAPPDATA%\RustWrath\benilla-config\Logs\Launcher`. A normal close waits for
`latest-diagnostics.md` and `latest-tickets.md` in that state root. Copy the run directory and
both reports before launching again: only three launch generations are retained. If the reports
are missing, run the collector against the same state root:

```powershell
$env:BENILLA_HOME = "$env:LOCALAPPDATA\RustWrath\benilla-config"
& 'C:\RustWrath\candidate-1\tools\collect-diagnostics.ps1'
```

To identify the candidate, include the candidate directory name, `release-manifest.json` (or
its SHA-256 from `launch.txt`), `launch.txt`, the first `benilla build ...` line in
`stdout.log`, `latest-diagnostics.md` and `latest-tickets.md`. For UI or addon evidence,
include a directory listing of the AddOns folder and the relevant TOC versions. Review and
redact account names and endpoints before posting a report publicly.

The launcher refuses a changed release file, a missing `release-manifest.json`, and an
effective data root that does not contain every archive named by the selected profile. It names
the missing or `.mpq.disabled` archives. Do not rename `.disabled` files; obtain the matching data or select
the profile that matches the data. If it refuses before the game opens, copy the console text; it
names the profile, the Data folder and the archives it could not find.

## Testing and troubleshooting

- If the launcher reports an existing client/build, wait or coordinate with its owner;
  do not start another executable or kill the other process to bypass it.
- If the binary is missing in a checkout, run the coordinated build above. A clean Git tree
  does not establish that the executable is current. In an assembled candidate, a missing binary
  is a changed release and the launcher refuses it; re-copy the candidate.
- If login cannot connect, confirm the auth/world services are running and the endpoint
  overrides are correct. For a remote realm, check that both ports are reachable from the
  tester and that the advertised world address is not loopback. This launcher does not start or
  modify the server.
- Select Firna for Northshire and confirm her name/location before interacting. A human tester
  drives the session; successful login alone is not a completed gameplay check.
- For a modal or black screen, record the exact error/action and the newest Launcher run
  directory. `launch.txt` records the mode, candidate directory, executable hash, package
  version and manifest hash, asset profile and manifest hash, data roots, checked manifest
  archive count, addon mode and path, UI mode, endpoints and state root; `stdout.log` and
  `stderr.log` contain the build banner and diagnostics. Review and redact account/session
  details and endpoints before sharing logs. Capture relevant evidence before launcher
  retention removes old runs.

Coordinate checks under [the project workflow](agents/workflow.md). Documentation updates
alone need no compilation, client run or server interaction.

The client is the **3.3.5a session** by default: the boot selects the Wrath network and UI
target, and the stock 3.3.5a interface, without an environment switch. `WOW_DONOR_112=1` is the
only way back to the donor 1.12 path (fixtures and reference tests). Developer direct-entry
runners remain separate: `WOW_WRATH=1` without `WOW_WRATH_LOGIN_UI=1` retains their explicit
environment identity and named-character path.
