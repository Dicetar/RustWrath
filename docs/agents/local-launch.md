# Local server and client launch

This is the workstation-specific procedure for the owner's RustWrath client and
BridgeLab AzerothCore server. Starting the server, building, and launching the
client require authorization in the current task. Check live processes and
coordinate with the current owner before using a shared Cargo or client slot.
The owner enters credentials, chooses a character, and drives gameplay.

## Check existing services

In PowerShell, inspect both processes and listening ports before starting anything.

```powershell
Get-Process mysqld, authserver, worldserver, benilla, cargo, rustc -ErrorAction SilentlyContinue |
    Select-Object Id, ProcessName, Path
Get-NetTCPConnection -State Listen -LocalPort 33307, 3724, 8095 -ErrorAction SilentlyContinue |
    Format-Table LocalAddress, LocalPort, OwningProcess
```

BridgeLab MySQL uses loopback port 33307. The authentication and world servers
use ports 3724 and 8095. If a required port is occupied by an unexpected process,
identify its owner before acting. Do not terminate another session to free a slot.

## Start the server

The live deployment is `D:\WOW\WM_BridgeLab\run\bin`. The similarly named
`run\worldserver.exe` is a stale duplicate. Start only the missing services,
in database, authentication, then world order.

If port 33307 is not listening and no BridgeLab MySQL process is running, start
the existing helper with binlog maintenance disabled.

```powershell
& 'D:\WOW\wm-project\scripts\bridge_lab\Start-BridgeLabMySql.ps1' -SkipBinlogMaintenance
```

The helper waits for database readiness. If it fails, inspect its output and
`D:\WOW\WM_BridgeLab\logs\mysql-lab.err` before retrying.

After the database is ready, start each missing server using its existing config.
The working directory is the run directory, so the relative config paths resolve.

```powershell
$run = 'D:\WOW\WM_BridgeLab\run'
Start-Process -FilePath (Join-Path $run 'bin\authserver.exe') `
    -WorkingDirectory $run -ArgumentList '-c', 'configs\authserver.conf' `
    -WindowStyle Hidden -PassThru
Start-Process -FilePath (Join-Path $run 'bin\worldserver.exe') `
    -WorkingDirectory $run -ArgumentList '-c', 'configs\worldserver.conf' `
    -WindowStyle Hidden -PassThru
```

Run only the command for a service that is missing. World startup can take
longer than authentication. Wait for both ports and confirm the owning PIDs
match the launched processes. If a process exits, inspect
`D:\WOW\WM_BridgeLab\run\Server.log` and the server's configured logs.
Do not run SQL migrations, change configuration, restart a running server, or
delete database logs as part of a normal launch.

## Build and launch the client

The normal player launcher and data-profile options are documented in
[the client launch guide](../launching.md). From the primary checkout, after
confirming the shared Cargo slot is free, build when the user has requested a
build and the executable needs updating.

```powershell
Set-Location 'D:\Projects\project-b-client'
cargo build -p benilla --features hotpatch
```

The launcher uses `D:\Projects\project-b-client\target\debug\benilla.exe`.
For a build from a separate worktree, set `CARGO_TARGET_DIR` to
`D:\Projects\project-b-client\target` in that build shell to update that
executable. Inspect Git status and do not pull or switch the primary checkout
under its uncommitted work.

With no client, Cargo, or rustc process occupying the slot, launch the player
client using the workstation's normal launcher:

```powershell
& 'D:\Projects\project-b-client\Launch RustWrath.cmd'
```

The launcher uses the Wrath login UI, local authentication at
`127.0.0.1:3724`, world endpoint `127.0.0.1:8095`, and the `wm-hd`
profile. See the launch guide for data-path and addon options. It retains only
three launcher-log generations, so preserve evidence from an older run before
launching if it is needed. The client launcher does not start the server.

Confirm the client process stays alive and report the PID and launcher log
directory. A listening port or a live process does not establish successful
login or gameplay. Leave login and gameplay to the owner.
