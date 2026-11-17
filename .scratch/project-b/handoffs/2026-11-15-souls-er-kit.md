# Souls kit schema and Elden Ring converter (work-plan step E)

Date 2026-11-15. Owner: Claude. Branch `feat/souls-er-kit`, worktree
`D:/Projects/rw-worktrees/feat-souls-er-kit`, based on `main` `4ff17526`. Merged to `main` by
fast-forward as `5f770c6b` and pushed to `origin` on 2026-11-15 at the owner's direction
(Dicetar/RustWrath pull request 1); the "Not merged" in the active-ownership entry predates that.
All changes are inside `crates/benilla-combat-core` plus one ownership note.

## What exists now

- `action.rs`: `InputWindows` (dodge listening offset; cancel offsets for light, heavy, dodge and
  guard; `None` is never) stored beside a spec with `ActionTable::insert_with_inputs`. An action
  inserted with plain `insert` behaves exactly as before.
- `engine.rs`: when the running action has `InputWindows`, a held request replaces it at the
  offset of its own kind, a dodge press is heard from its own offset, and the follow-up gates only
  requests of the follow-up's kind (a dodge can leave a light chain).
- `kit.rs`: `Kit` / `KitAction` / `KitHit` / `KitCancels`, thousandths fixed-point `Milli`, the
  `rustwrath-kit 1` text format (`Kit::parse`, `Kit::to_text`), `Kit::validate`,
  `KitAction::compile` and `Kit::to_table` (animation frames to engine ticks at any tick rate),
  `KitAction::motion_at`, `Kit::frame_at_tick`, `Kit::action_named`.
- `kit_import.rs`: `import_kit` reads the generated `extracted.rs` shape (plus the `data.rs`
  constants beside it) for one weapon, grip and load, and returns the kit and an `ImportReport`.
- `src/bin/import_kit.rs`: the command-line converter.

```
cargo run -p benilla-combat-core --bin import_kit -- \
  --extracted D:\Projects\elden-ring-combat-rewrite-reference\src\sim\extracted.rs \
  --weapon Longsword --grip one --load medium --out out\kits\longsword-1h-medium.kit
```

## Evidence

- Focused run: 19 combat-core cases, all passed (14 new, 5 existing engine cases as regression),
  and the binary builds without warnings. Log
  `.scratch/project-b/test-2026-11-15-claude-souls-er-kit.log`. The other 33 combat-core cases
  and every other crate were not run.
- Converter run on the owner's real generated tables: Longsword, one-handed, medium load gives
  5 dodges and 15 attacks (20 actions), all of which compile to a 100 Hz table; 18 attacks were
  left out (jump, off-hand, paired) and are listed by name with the reason; 15 empty windows were
  dropped. Output `out/kits/longsword-1h-medium.kit` in the worktree (ignored; never commit it).
- Tests use authored values only. No game value is in the repository.

## Not done, not proven

- Nothing in `benilla-app` loads a kit. `combat_playback.rs` still builds its two host actions by
  hand. No app build and no client run were made; nothing here is visible in game.
- The engine consumes: light/heavy/dodge/guard cancel offsets, the dodge listening offset, the
  first hit window as contact, and the summed hit stamina charged at the start. It does not yet
  consume: jump, move and left-hand cancels, the charge window, later hits and per-hit stamina
  timing, turn locks, root motion, hit-stop, blade samples. They are carried in the kit.
- A light or heavy press has to be resolved to an action id by the adapter (`next_light` /
  `next_heavy` of the running action, or the opener by `action_named`). That adapter is not
  written.
- Clip retargeting (Elden Ring clip to the WoW skeleton) is untouched; `source` in the kit names
  the donor clip for it.
- Guard itself (holding the shield) has no kit record; only `GuardCounter` is imported.

## Next slice

Load a kit in the app behind the existing combat-playback seam: read the file named by local
state, `to_table` it at the server tick rate, map the light/dodge inputs to kit ids, and drive
`motion_at` for root motion. That needs an app build and an owner-driven fight to accept.
