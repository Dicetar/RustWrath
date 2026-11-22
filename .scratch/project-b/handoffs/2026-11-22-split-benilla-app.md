# Split benilla-app into workspace crates for faster rebuilds

Status: in progress
Owner: Claude Code / Opus 5.5
Branch: refactor/split-benilla-app
Issue: none
Next: extract `souls` into `benilla-souls` on top of the core (plan below), then time the edit loop

## Why

A one-line edit anywhere in `benilla-app` (293k lines of non-test code, 312k of tests) recompiles the
whole crate and relinks a 164 MB exe. The owner waited ~13 min for a cold dx build and minutes for
each live-code patch.

## What exists now

1. `benilla-client` (new): the assembler. `run`, `run_with` and `launch()` moved out of
   `benilla-app/src/lib.rs` verbatim; `config_dir` stays in the core (the core calls it) and is
   re-exported. The `benilla` launcher depends on `benilla-client`. `examples/extended_launcher.rs`
   and `tests/launcher_shape.rs` moved with it. In the core only visibility changed: the modules
   and items the boot names are `pub` (`client_state`, `crash`, `dev`, `game_plugins`,
   `local_state`, `no_install`, `perf`, `run_mode`, `shutdown`, `video`, `hotpatch`), plus a
   `WrathSessionConfig` re-export. `capture::probe_env`'s source scan also reads
   `benilla-client/src`, where `WOW_PROBE=list` is dispatched now. CI, `scripts/check.sh`,
   `gates.sh` (player-clippy, live-code-check) and `genmap.sh` name the new crate.
   `Cargo.lock` on main was missing the `bevy/hotpatching` packages (subsecond, dioxus-devtools…);
   every build added them locally, and this commit carries them.

## Plan (the measured graph and timings are in Phase 1 below)

Layering: `benilla` → `benilla-client` (assembler) → feature crates → `benilla-app` (core) → engine
crates. The core keeps its name (renaming it would change embedded shader paths, `benilla_app::`
type-name strings, log targets and the CI triggers). Plugin order is preserved exactly: an upper
crate's plugin goes in with `GamePlugins.build().add_after::<Prev>(Plugin)` at today's position.
Moved files keep their `crate::x::` paths by importing the core modules at the new crate's root.

Order:
1. **benilla-client** — done (this branch).
2. **benilla-souls** — souls systems (input, drive, targets, hud, retarget, server link). Five
   core reads point into souls; their types move down into a core `souls` module at the same paths:
   `ErControls` (+ methods), `SoulsConfig` (all of `souls/config.rs`), `SoulsState` (+ its field
   types `DriveTrack`, `SwingTrack`, `Instance`, `ChargeHold`; fields become `pub`), `SwingMute`
   (path `souls::server::SwingMute` kept for `lanes/guard_tests.rs`), `tap_addon` + inbox. The
   upper crate uses 19 core modules (89 refs): those items go `pub`. The client inserts
   `SoulsPlugin` after `CombatPlaybackPlugin`. Add `benilla-souls` to player-clippy and the
   player-tests trigger/args.
3. **benilla-dev** — capture, debug_panel, devtools, hover_log, asset_churn, preflight,
   probe_shield, dev.rs: 27.9k lines, 449 refs out, 2 in. Cut `run_mode` → capture
   (`capture_ui_opted_in`, `start_state` read the scenario table at boot) with a boot hook the
   client installs first, or by moving the scenario names down. Four core tests build the client
   with every instrument (`vm_pass` ×3, `script_calls` ×1) and `dev.rs`/`capture::scenarios` tests
   use core fixtures (`headless_client_with`, `local_state::test_env`): expose them behind a
   `test-support` feature and move the four tests up.
4. **benilla-panes** — 49 UI panes, 35k lines, 205 core refs into them, mostly a hub reading a
   pane's state resource (ui_unit → guild 11, portrait → ui_models 8, target → dialog verbs 6). Move
   those resources down pane by pane.
5. Small downward leaves when convenient: ui_text (4.3k + 4.1k tests, 3 refs out), text_filter,
   smart_rect, client_state.
6. The core stays one strongly connected component of ~140 modules around net / ui_script /
   script_calls / ui_action. Shrinking it further is a design change (invert the packet-handler and
   call-host registries), not a move.

## Phase 1 measurements (main ff0205077, this workstation)

`cargo build -p benilla --features hotpatch`, dev profile, own target dir, sccache.

| Build | Total | benilla-app | link |
|---|---|---|---|
| Cold, link.exe | 20m31s | 513s | 45.7s |
| souls edit, link.exe | 78s | 36.6s | 39.6s |
| net edit, link.exe | 74s | 41.2s | 30.9s |
| souls edit, rust-lld | 61–65s | 42–48s | 15–16s |
| net edit, rust-lld | 59–67s | 47–56s | 10.6–11.2s |
| souls edit, rust-lld + bevy_dynamic_linking | 45–49s | 37–40s | 7.5–8.9s |

- The link is half of an edit today. `rust-lld.exe` (in the toolchain) halves it with no code
  change; not committed (a workstation-wide build setting, one full rebuild, dx's link unverified).
- `bevy_dynamic_linking` builds and links in 7.5–9 s. It was not tried under
  `dx serve --hot-patch` (that starts the client); ADR-0002 says live code must not combine with it,
  and subsecond resolves patch symbols against the exe, where dynamic Bevy's symbols are not.

## Checks run

- `cargo check -p benilla -p benilla-client -p benilla-app --features hotpatch`: compiles (two
  warnings, both already on main: `player.rs` unused imports, `quest_trace` `hotpatch_attempts`).
- `cargo clippy -p benilla -p benilla-client -p benilla-app --no-default-features -- -D warnings`
  (player-clippy): clean.
- `cargo clippy -p benilla-client -p benilla --all-targets --features hotpatch`: no findings in
  the new crate.
- `cargo fmt --all -- --check`: clean.
- Tests: not run locally (the app's test build is ~13 min); CI runs the changed crates' tests.
- CI: not run yet.
