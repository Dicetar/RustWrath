# Launching RustWrath on Windows

Double-click **Launch RustWrath.cmd** in the repository root. The client opens its stock
login screen; enter your account and password there, choose the realm, then a character.
The launcher does not read `.probe-identity` or save a password. The local auth/world servers
must already be running. The launcher refuses to compete with an existing client or build.

Build the executable after source changes:

```powershell
cd D:\Projects\project-b-client
cargo build -p benilla --features hotpatch
```

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

The stock character screen owns selection, creation, appearance choices and deletion
confirmation. Character changes must receive server replies before the roster refreshes.
Settings use the existing stock options and local CVar persistence. Multiclassing is deferred.

Developer direct-entry runners remain separate: `WOW_WRATH=1` without
`WOW_WRATH_LOGIN_UI=1` retains their explicit environment identity and named-character path.
