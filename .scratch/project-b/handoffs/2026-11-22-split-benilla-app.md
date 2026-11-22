# Split benilla-app into workspace crates for faster rebuilds

Status: in progress
Owner: Claude Code / Opus 5.5
Branch: refactor/split-benilla-app-v2 (refactor/split-benilla-app is abandoned: its second commit captured staged renames without their contents)
Issue: none
Next: land on main once CI is green; then `benilla-dev` (plan step 3)

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

2. `benilla-souls` (new): the Souls systems (input, drive, targets, hud, retarget, the server
   link) moved verbatim from `benilla-app/src/souls*`. What the core reads stayed in a core
   `souls` module at its old paths: `souls/config.rs` unchanged, `ErControls` (+ methods),
   `SoulsState` (+ `DriveTrack`, `SwingTrack`, `Instance`, `ChargeHold`), `SwingMute`,
   `tap_addon` and its inbox. Their four `init_resource` calls moved from `SoulsPlugin` into a core
   `SoulsStatePlugin` that takes `SoulsPlugin`'s old slot in `GamePlugins`; the client inserts
   `SoulsPlugin` right after it (`add_after`), so the plugin and system order is unchanged and the
   core's headless test client still has the resources its systems read. In the core only
   visibility changed (items the souls crate names are `pub`, plus the types their signatures
   expose); one `#[allow(clippy::should_implement_trait)]` on `PlaySeq::next`, now public.
   `benilla-souls` joins player-clippy and player-tests (trigger and args) in CI and the scripts.

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

## After the split (benilla-client + benilla-souls)

Same build and edits; the souls edit is now in `crates/benilla-souls/src/server.rs`. Per-unit
compile+link work from `--timings`: an editor's rust-analyzer had this worktree open and held the
build lock between runs, so wall clock here includes lock waits and is not comparable.

| One-line edit | Before: work (units) | After: work (units) |
|---|---|---|
| souls, link.exe | 76 s (app 36.6 + link 39.6) | **22 s** (souls 2.9 + client 0.7 + link 18.1) |
| souls, rust-lld | 58–64 s (app 42–48 + link 15–16) | **16–18 s** (souls 2.5–3.9 + client 1.2 + link 11.8–13.9); wall 17–18 s |
| net (core), link.exe | 72 s (app 41.2 + link 30.9) | 64–70 s (app 26–31 + souls 3–7 + client 1–5 + link 30–33) |

A souls edit no longer compiles benilla-app at all; what is left is the small crate and the link.
A core edit costs what it did (the core is barely smaller yet; the crates above it are small).
Not measured: a dx live-code patch. It replays the edited crate and the crates above it, so a souls
patch should now compile benilla-souls + benilla-client instead of benilla-app, but that needs the
client running.

## Checks run

- `cargo check -p benilla -p benilla-client -p benilla-app --features hotpatch`: compiles (two
  warnings, both already on main: `player.rs` unused imports, `quest_trace` `hotpatch_attempts`).
- `cargo clippy -p benilla -p benilla-client -p benilla-app --no-default-features -- -D warnings`
  (player-clippy): clean.
- `cargo clippy -p benilla-client -p benilla --all-targets --features hotpatch`: no findings in
  the new crate.
- `cargo fmt --all -- --check`: clean.
- Souls commit: `cargo check -p benilla -p benilla-souls --features benilla/hotpatch` clean but
  the two main warnings; player-clippy over benilla, client, souls and app with `-D warnings`
  clean; `cargo test -p benilla-souls`: 29 passed, 1 ignored; fmt clean.
- Tests of benilla-app not run locally (its test build is ~13 min); CI runs the changed crates'.
- CI on commit 1 (run 45008817297): 6,917 benilla-app lib tests passed; `world_api_wall` failed
  (its source scan did not see the boot's engine calls in benilla-client), fixed in commit 2.
- CI on the souls commit 789c2909f (run 45013482959): windows passed (tests included); the Linux
  gates job lost its runner mid test-build (GitHub infrastructure), rerun superseded by the merge.
- Merge of main c4d139145: `cargo check` clean. CI on the merge tip: see the landing commit.
