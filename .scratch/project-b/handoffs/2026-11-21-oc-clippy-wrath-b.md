# Player-clippy debt: wrath.rs

Status: ready to merge
Owner: OpenCode / DeepSeek V4.1 Flash (fallback after two free-model review failures)
Branch: oc/clippy-wrath-b
Issue: none; CI gates
Next: the other player-clippy files; the superseded oc/clippy-wrath branch is not to be merged.

## Findings fixed (all body changes are mechanical; no behavior change)

- Removed the unused `WrathControl::running` and `WrathControl::stop` methods (no caller in
  the workspace; `stop_with_delivery` stays).
- Collapsed the nested `if self.ended` restart guard into one condition.
- `SessionEnded` destructuring: `session_epoch: _`, `world_generation: _`.
- `Some(mut items)` -> `Some(items)` at the setup callback; the other site still needs its
  `mut`.
- Removed the dead `world_authed = true` store; the loop is now `loop { ... break }` because
  the flag was only read by its own always-true condition.
- `drop(control)`: the guard is now scoped in a block, so the borrow ends at the same point
  without the no-op drop.
- `drained`: `take_events().into_iter().enumerate()`; the limit test and the processed count
  are unchanged, and the counter is not read after the loop.
- Six `&mut *view` -> `&mut view`.
- `for slot in 0..20` -> `slots.iter().take(20).enumerate()` (the array has `QUEST_LOG_SLOTS`
  entries, so the same 20 slots are visited).
- `(idx - base) % 2 == 0` -> `(idx - base).is_multiple_of(2)`.
- `Some(SourceHealth::new(...).ok()?)` -> `SourceHealth::new(...).ok()`.
- Three `type_complexity` aliases: `AuraAnchorRows`, `PartyAuraRows`, `FieldDiff`; the
  underlying types are unchanged.
- The old unused `Err(e)` finding at line 12192 does not exist at this HEAD: that arm now uses
  `e` in its `format!("{e:#}")`, so it is left as is.

No `#[allow]` was added. No new test: the change is behavior-neutral. The existing
`interactive_restart_never_uses_the_boot_pick_during_logout_transition` covers the restart
guard, and CI re-runs the `benilla-app` suite. `rustfmt --check` on the file is clean.

CI: not run yet. No Cargo, test, client or server run.
