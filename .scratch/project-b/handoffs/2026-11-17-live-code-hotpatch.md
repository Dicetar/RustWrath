# Live code: patch any client code into the running process

Status: pushed on the task branch, not landed; not compiled locally, not run
Owner: Claude Code (Opus 5.5)
Branch: feat/live-code-hotpatch
Issue: none (owner request 2026-11-17: "be able to hotpatch anything")
Next: the integrator lands this on `main`. The owner installs the matching `dx`, runs
`tools/launch-client.ps1 -LiveCode`, commits the `Cargo.lock` change the first build makes, and
runs the acceptance in ADR-0002 "Live code". First real use: patch in the trainer/game-object
fix from `fix/wrath-trainer-gameobject-use` without a restart.

Date 2026-11-17. Worktree `D:/Projects/wow/worktrees/live-code-hotpatch`, cut from `origin/main`
`78166894`. No Cargo was run locally and the client was not started; CI is the only compile.

## Why

The existing `hotpatch` path reloads pose/movement module DLLs, shaders and Lua. A fix in a
decoder, a system or the trainer/GO-use path still needed a client restart and relog. The owner
wants any code fix applied to the running client.

## What changed

- `live_code` feature (`benilla` → `benilla-app` → `bevy/hotpatching`): Bevy's subsecond-based
  `HotPatchPlugin` joins `DefaultPlugins` and connects to the `dx serve --hot-patch` that started
  the client. Every system and observer takes the patched code on its next run.
- `crates/benilla-app/src/live_code.rs`: `call` (subsecond's jump-table call, a direct call
  without the feature), the `LiveCodePatched` message, the `Rederive` set in `First` gated on it,
  `rederive(app, systems)`, and `LiveCodePlugin` (in `game_plugins.rs`), which turns Bevy's
  `HotPatched` into `LiveCodePatched`. One focused test.
- `net/io.rs`: the reader thread's poll and decode, and the writer thread's per-command encode,
  go through `live_code::call`, so packet code patches too. The writer match moved into a closure
  (rustfmt re-indented it; whitespace-ignoring diff is small); the command is taken from an
  `Option` so a patch's retry cannot use a moved value.
- `wdb.rs`: each saved store re-decodes its held records from their kept answer bytes on a patch
  (`redecode`). One focused test in `wdb/tests.rs`.
- `ui_action/mod.rs`: `load_spells` re-runs on a patch (the trainer's learn-spell hop).
- `tools/launch-client.ps1 -LiveCode`: checks `dx` against the `subsecond` version in
  `Cargo.lock`, prints the install command, runs `dx serve --hot-patch --package benilla --bin
  benilla --features hotpatch,live_code` with the launcher's environment.
- CI: `live-code-clippy` step on the Linux job.
- Docs: ADR-0002 "Live code" section and reload-class rows; `docs/agents/local-launch.md`.

## Unverified

- `dx` flag spellings (`--package`, `--bin`, `--features`) for dioxus-cli 0.7.x.
- `dx` builds through Cargo with the repo's sccache `rustc-wrapper`; dx sets its own
  `RUSTC_WORKSPACE_WRAPPER`. Whether the two cooperate is unknown.
- `Cargo.lock` has no `subsecond` yet; the launcher warns and the first build adds it.
- Whether dx passes the launcher's environment (data path, profile) to the client process.
- No patch has been applied to a running client.
