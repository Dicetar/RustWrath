# RustWrath

A Rust and Bevy World of Warcraft client targeting **3.3.5a, build 12340**, derived from
[benilla](https://github.com/samwhosung/benilla). The native client connects to the owner's
existing AzerothCore server and reads assets from the player's own installation.

The current milestone is a private Windows alpha: login, realm and character navigation,
then a playable Northshire quest/combat/loot loop. These flows are under active repair.
Successful builds and focused tests do not establish complete gameplay or addon compatibility.

## Launch the native client

In the local checkout, double-click **Launch RustWrath.cmd**, or run in PowerShell:

```powershell
& 'D:\Projects\project-b-client\Launch RustWrath.cmd'
```

The executable must already be built and the local auth/world servers running. Enter
credentials in the game window. The launcher captures logs and refuses to compete with
an existing client or Cargo build. See [Windows launch instructions](docs/launching.md)
for building, paths, endpoint overrides and troubleshooting.

Committing, pushing or pulling is not required to test an existing local executable.
Source changes require a rebuild or a specifically supported and verified reload path.

## Repository layout

| Location | Purpose |
|---|---|
| crates/, Cargo.toml | Native Rust/Bevy client workspace |
| src/, public/, package.json, vite.config.ts | Separate React/Vite web preview |
| docs/profiles/ | Asset patch-chain manifests |
| benilla-config/ | Ignored local settings, logs and runtime state |
| .scratch/project-b/ | Local tickets, handoffs and verification evidence |
| reference/, third_party/ | Donor reference tables and vendored dependencies |

The web preview's npm commands build that applet. They do not build or verify the native
client. The generated [native module map](docs/MAP.md) helps navigate the Rust workspace.

## Development

The canonical repository is [Dicetar/RustWrath](https://github.com/Dicetar/RustWrath),
with active development on wotlk. Start with [AGENTS.md](AGENTS.md),
[project context](CONTEXT.md), and the [working workflow](docs/agents/workflow.md).
Current assignments belong in [active ownership](docs/agents/active-ownership.md).

Wrath behavior is investigated using the preserved local build-12340 reference and
selected asset profile. See [the reference workflow](docs/agents/12340-reference.md).
Donor documentation supports 1.12 compatibility; it does not describe this fork's
current feature coverage.

## Attribution and content

RustWrath builds on benilla's Rust/Bevy client foundation. Early file-format guidance
in the donor came from [wowemulation-dev](https://github.com/wowemulation-dev), including
[warcraft-rs](https://github.com/wowemulation-dev/warcraft-rs).

This independent fan project is not affiliated with Blizzard Entertainment. No Blizzard
game assets or original client implementation are bundled. Provide your own legally
obtained installation. Own code is licensed under [MIT](LICENSE-MIT) or
[Apache 2.0](LICENSE-APACHE); vendored code retains its accompanying notices.
