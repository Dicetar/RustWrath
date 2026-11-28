# Integration baseline before the P0 alpha work

Status: landed on main by this branch's single push
Owner: Claude Code (Opus 5.5)
Branch: integrate/baseline-2026-11-28
Issue: none
Next: P0 sessions merge the new origin/main; the deferred branches below wait on their owners

Started from `origin/main` `aa8f4a9c7`. Every local and remote branch, registered worktree and
loose clone found on the workstation on 2026-11-28 was checked by ancestry, `git cherry` /
patch-id and the resulting source. Nothing here was compiled or run locally (owner rule); the
single push's CI run is the first compile.

## Landed

| Change | How | Notes |
|---|---|---|
| `fix/wrath-debugstack-utf8` `382be4943` | merge | `debugstack` uses the VM's byte-based `short_src` (`benilla-mlua` builds it with `chunkid_n(source, 60)`) |
| `fix/wrath-screenshot-collision` `dbfaadf6e` | merge | plus `b7de019b5`: its new test needs `tempfile`, which `benilla-app` enables only under `dev`, and CI's `player-tests` job runs `--no-default-features`; the test is now `#[cfg(feature = "dev")]` like every other `tempfile` use in the crate |
| failed-dispel diagnostics `dd9be2b3c` from `refactor/benilla-tooltip` | cherry-pick `-x` | independent of the branch's other commits; `benilla-app` only |

## Not landed

| Candidate | Disposition | Reason |
|---|---|---|
| `refactor/benilla-tooltip` (harness move `35830acd2`, perf split `a35d87b9c`) | deferred | Neither compiles as written. `35830acd2` drops `local_state` from `benilla-dev`'s root `use benilla_app::dev_seam::{…}` while `hover_log.rs`, `debug_panel/mod.rs` and `capture/probe_model_camera.rs` still use `crate::local_state::…`. `a35d87b9c` adds `mod perf;` beside the `perf` already in that `use` list (E0255). The owner of the branch fixes both and validates per its notes |
| `refactor/benilla-addon-harness` `35830acd2` | superseded | contained in `refactor/benilla-tooltip`; same `local_state` defect |
| `fix/wrath-addon-drop-in` (separate clone at `worktrees/wrath-addon-drop-in`, tip `b169ceb80`) | deferred | `Status: in progress` (Codex), never pushed, no remote; three commits not on main (launcher `-AddOnsPath`/`-AddonCompatibleBags`/`-ProfileAddonUi`, `GetClickFrame`, Bagnon/Bartender repairs) and it edits the `benilla-app/src/addon_harness/` files the harness move relocates |
| `origin/astra/sex-stance`, `origin/astra/all-rigs` | deferred | Souls retarget work on `benilla-app/src/souls/retarget.rs`, which main moved to `benilla-souls` in `789c2909f`; needs a port, notes give no status, combined Linux tests unconfirmed. `all-rigs` is contained in `sex-stance` |
| `origin/claude/m2-probe` `6d5109402` | deferred | developer example its author held back as never compiled |
| `origin/oc/clippy-proto-header` | already present | the `#[cfg(test)]` fix is on main; the branch's only unique commit shortens its note |
| `origin/astra/night-2026-11-20` | already present | main holds newer versions of both work packages |
| `fix/wrath-mirror-info`, `wrath/night-2026-11-27`, `integrate/night-2026-11-27`, `wrath/achievements-2026-11-28`, `fix/release-recovery-guardrails`, `worktree-abilities-casting-actionbars-modules` | already present | ancestors of main |
| `fix/release-recovery-guardrails` worktree | no code | only an untracked `Status: in progress` note from 2026-11-15; main has the release-set recovery from `c91514217` |
| `.claude/worktrees/abilities-casting-actionbars-modules` | superseded | uncommitted 144-slot recount in `ui_action/recount.rs`; main has its own 144-slot `action_slots` from `a6a5877c2` |
| `fix/rustwrath-p0-session-entry-reconnect` | reviewed, no merge needed | see below |
| `fix/windows-alpha-launch` | active | created 2026-11-28 from `aa8f4a9c7`; left alone |

## P0 session-entry review carried forward

ClaudeJr's source review of `fix/rustwrath-p0-session-entry-reconnect` ran at `aa8f4a9c7` and
reported no code changes. Confirmed: the branch tip is `aa8f4a9c7`, with no commits beyond it or
`origin/main`, no remote branch, no tracked, untracked or ignored changes in its worktree, and a
reflog holding only its creation. No merge is needed.

`aa8f4a9c7..e67bd8d6a` changes only `benilla-app/src/screenshot.rs`, the failed-dispel counters
in `benilla-app/src/ui_script/wrath_combat_log_feed.rs` (debug-only, `Phase::InWorld` only, no
reset path), `benilla-ui`'s `debugstack` label and their notes. None touch login, realm or roster
selection, session-worker admission, logout, terminal-event ordering or world-state reset, so the
source review carries forward to `e67bd8d6a` unchanged. Runtime acceptance of the session-entry
and reconnect flow is still pending (owner-driven launch).

The worktree `D:/Projects/wow/worktrees/fix-rustwrath-p0-session-entry-reconnect`
(`refs/heads/fix/rustwrath-p0-session-entry-reconnect` @ `aa8f4a9c7`, clean) is a cleanup
candidate pending the owner's approval.
