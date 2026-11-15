# Today's obvious defects: the gate probes, the plate-gate lint, the ButtonText label

Status: ready to merge
Owner: OpenCode / DeepSeek V4.1 Flash (overseer), two AutoSwarm `opencode` children, one per area
Branch: fix/overseer-obvious-2026-11-15
Issue: none
Next: land on source review; the owner's next launch is the first look at the clock button, the chat stall and the plates

Based on `origin/main` `9dc36e48`. The batch that landed on 2026-11-15 (`919edd7a`, `b0e906f4`,
`408df3ad`, and the review notes in `2026-11-14-wrath-glue-runtime.md`) was source-reviewed only; no
build followed it. This branch is the build-and-repair pass over that batch.

## What exists now

| File | Change | Why |
|---|---|---|
| `crates/benilla-app/src/ui_script/manifest.rs` | `each_wrath_core_file_loads_in_bounded_time` is `#[ignore]`d, doc keeps the stall, the log path and the explicit run command | It loads the whole installed core and asserts wall-clock time; it is a probe for one 153 s stall, and in the default run it is slow and flaky. The originating note asked for exactly this once the stall was explained |
| `crates/benilla-app/src/ui_text/engine/mod.rs` | `the_chat_face_shapes_a_line_in_bounded_time` is `#[ignore]`d the same way | Same: a 5 s wall-clock assert on one raster, flaky on a loaded workstation |
| `crates/benilla-app/src/vplates.rs` | `wrath_plate_gate`'s last rule is the reference's own shape, `if unit.is_player && !friendly && !unit.attackable { return false; } true` | `!(...)` drew `clippy::nonminimal_bool`, which `scripts/gates.sh` turns into a failure with `-D warnings`; semantics unchanged, order unchanged (`Unit_C.cpp:20408`-`:20520`) |
| `crates/benilla-ui/src/loader/widgets.rs` | The named `<ButtonText/>` label no longer re-sets its Lua global after `CreateFontString`; comments now state the region is always built, as `LoadXML_String` does (`CSimpleButton.cpp:939`), and `SetText("")` alone would not (`:650`) | The extra `globals().set` overwrote `_G`'s first holder, against `0x701c8a`, and disagreed with the named state-texture path two functions above, which publishes through the name lookup alone |
| `crates/benilla-ui/src/loader/regions.rs`, `widgets.rs` | `.last()` → `.next_back()` on the `parentKey` lookup | `clippy::double_ended_iterator_last`; first match and last match are the same element |
| `crates/benilla-ui/src/loader/tests.rs` | Two new cases: `unnamed_button_text_is_available_during_onload_on_1_12`, `a_same_named_instance_button_text_keeps_its_name` | The label build is not target-gated, so the 1.12 VM needed its own case; the second pins the `_G` first-holder rule the removed `globals().set` violated |
| `crates/benilla-app/src/target/gate_tests.rs` | `greeted` and `steps` also read `QuestgiverOpenTracked { gossip: true }` as a greeting | A traced Wrath world sends the gossip open as the tracked command (`click.rs`), so five talk-leg cases were red against helpers that only knew `GossipHello`. Test-only; no production behaviour changed |

## Checks actually run

All on `D:/Projects/rw-worktrees/overseer-2026-11-15`, Cargo 1.99.1, `CARGO_INCREMENTAL=0`, the
`.cargo/config.toml` sccache wrapper active (`cargo check -v` shows `Running \`sccache rustc ...\``).
Logs are under `F:/AI/tmp/opencode/overseer/` on the workstation and are not committed.

| Check | Result |
|---|---|
| `cargo check -p benilla --features hotpatch` on `9dc36e48`, before any edit | Finished in 6m 04s, no errors. `main` compiles; the batch was build-safe |
| same, on this branch's tree | Finished in 1m 54s, no errors |
| `cargo test -p benilla-ui --lib unnamed_button_text` | 2 passed: the Wrath case and the new 1.12 case |
| `cargo test -p benilla-ui --lib a_same_named_instance_button_text_keeps_its_name` | 1 passed |
| `cargo test -p benilla-app --lib wrath_gate` | 8 passed |
| `cargo test -p benilla-app --lib vplates::gate_tests` | 3 passed (the 1.12 gate is unchanged) |
| `cargo test -p benilla-app --lib target::gate_tests` | 12 passed, after the `greeted`/`steps` change; 5 of these were red before it |
| `cargo test -p benilla-app gate_tests` (the broad filter) | 41 passed, 5 failed, all five in `ui_script::protected_gate_tests` — see below |
| `cargo clippy --no-deps -p benilla-app --all-targets` | Exit 0, no diagnostics in `vplates.rs` or `manifest.rs`, no `nonminimal_bool` |

`cargo test -p benilla-app gate_tests` also selects `ui_script::protected_gate_tests`. Its five
cases load the installed 3.3.5a FrameXML into a 1.12 VM and die on 3.3.5 scripts that index `self`
in OnLoad, plus `MoneyFrame.lua:233`. That is the environment condition the nameplate work already
recorded; on a 1.12 install, or under a target-aware harness, it is a different result. Nothing in
this branch reaches it.

## Not done, not proven

- **No client run.** Nothing here is seen in game: the clock button's label, the 153 s chat stall,
  and the plates at 41 yd are still open visible checks.
- **The two `#[ignore]`d probes were not run** in this task. They need the install; the commands
  are in their doc comments (`--ignored --nocapture`).
- **The gate chain cannot run on this workstation.** `rust-toolchain.toml` pins `1.98.1` with
  clippy and rustfmt, but `rustup` is not installed and the local toolchain is 1.99.1, so
  `cargo fmt` and `cargo clippy ... -D warnings` are not the gate the repository defines: local
  rustfmt prints repo-wide style diffs, and local clippy reports pre-existing debt in workspace
  crates (278 warnings from `third_party/parry3d` alone) that the pinned clippy may not. The
  narrow clippy form above is what was available, not the gate.
- `sccache --show-stats` reads 0 compile requests from this shell even while `cargo -v` shows the
  wrapper and the cache directory grows (716 MB). No cache hit is claimed.
- **1.12 gap recorded, not fixed:** the donor plate path reads only `UNIT_VIS_FLAGS_GHOST`
  (`0x60f62e` records `byte3 & 3`), so a stealthed unit keeps its plate on the 1.12 VM. The same
  read feeds `nameplates.rs:521`. This target is 12340, where only an enemy's creep bit hides a
  plate; the 1.12 half needs the 1.12 pin, which this checkout does not have.
