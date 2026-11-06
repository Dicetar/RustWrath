# Launching RustWrath on Windows

For the source-only versioned release assembly and its current limits, see
[Windows release assembly](architecture/windows-release-assembly.md). The normal checkout
launch described below is unchanged.

Double-click **Launch RustWrath.cmd** in the repository root. The client opens its stock
login screen; enter your account and password there, choose the realm, then a character.
The launcher does not read `.probe-identity` or save a password. The local auth/world servers
must already be running. The launcher refuses to compete with an existing client or build.

From PowerShell, the same entry point is:

```powershell
& 'D:\Projects\project-b-client\Launch RustWrath.cmd'
```

No commit, push or pull is required to test the existing local executable. The launcher
does not rebuild it. If source changed, coordinate a rebuild before expecting those fixes
in game. Do not pull or switch branches underneath uncommitted shared work.

Build after source changes, with the shared Cargo slot free and no client running:

```powershell
cd D:\Projects\project-b-client
cargo build -p benilla --features hotpatch
```

This workstation has an optional sccache wrapper. Set it in the build shell before Cargo:

```powershell
$env:RUSTC_WRAPPER = 'C:\Users\Qraag\.cargo\bin\sccache.exe'
```

Use that path only if installed. Selecting the wrapper does not prove a cache hit or speedup.
The launcher uses target/debug/benilla.exe; another build profile does not replace that
executable automatically. Git publication alone does not activate code in a running process.

Launch from PowerShell with another data root or auth server:

```powershell
.\tools\launch-client.ps1 -DataPath 'D:\WOW\world of warcraft 3.3.5a hd\data' -AuthServer '127.0.0.1:3724'
```

This workstation's default uses the existing linked shadow data root when available, to
exclude loose third-party addons without changing the installation. Pass `-WithAddons` to
use the supplied data root directly. The `wm-hd` manifest still defines archive precedence.
Logs are under `benilla-config/Logs/Launcher`; only three launch generations are retained.
The local world endpoint defaults to `127.0.0.1:8095`; pass `-WorldServer ''` to use the
address advertised by the selected realm instead.
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

## Testing and troubleshooting

- If the launcher reports an existing client/build, wait or coordinate with its owner;
  do not start another executable or kill the other process to bypass it.
- If the binary is missing, run the coordinated build above. A clean Git tree does not
  establish that the executable is current.
- If login cannot connect, confirm the local auth/world services are running and the
  endpoint overrides are correct. This launcher does not start or modify the server.
- Select Firna for Northshire and confirm her name/location before interacting. The owner
  drives combat; successful login is not a completed gameplay check.
- For a modal or black screen, record the exact error/action and newest Launcher run
  directory. launch.txt identifies the PID and executable hash; stdout.log/stderr.log
  contain diagnostics. Review/redact sensitive account/session details before sharing logs.
  Capture relevant evidence before launcher retention removes old runs.

Coordinate checks under [the project workflow](agents/workflow.md). Documentation updates
alone need no compilation, client run or server interaction.

Developer direct-entry runners remain separate: `WOW_WRATH=1` without
`WOW_WRATH_LOGIN_UI=1` retains their explicit environment identity and named-character path.
