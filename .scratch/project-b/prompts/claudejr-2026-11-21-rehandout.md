You are ClaudeJr on RustWrath. Tonight's tickets are in
`.scratch/project-b/prompts/claudejr-2026-11-21.md`; its rules and ownership limits still apply.
This rehandout replaces `claudejr-2026-11-21-continue.md`.

## Why you keep stalling
You do the work, push, write "CI pending" in the note and end your turn. Nothing wakes you up
again, so the branches sit unlanded. **Pushing is not a stopping point. Landing is.**

After each push:
1. Get the run id:
   `gh run list --repo Dicetar/RustWrath --branch jr/<slug> --limit 1 --json databaseId --jq '.[0].databaseId'`.
2. Wait for it in the same session: `gh run watch <id> --repo Dicetar/RustWrath --exit-status`.
   Run it in the background, so you are notified when it ends, or with a 590000 ms timeout and
   repeat. A full run takes about 25-40 min. Work on another ticket while you wait.
3. Read the result, fix the cause, and push again. Repeat until it is green, then land.

End your turn only when every ticket is landed, or blocked by something you cannot fix, and you
can name that blocker from a log line or a reference line.

## State at the time of this rehandout (main f82c180f3)
**`player-clippy` is no longer old debt.** Main passed it at 0a4b93e73 after the OC clippy
sweep. Your branches were cut before the sweep, so 23 of their findings are already fixed on
main. Merge `origin/main` into each branch and they go away. From now on every `player-clippy`
and `clippy` finding counts.

| Ticket | Branch, head | CI | What is left |
|---|---|---|---|
| 1 combat-log ring | `jr/combat-log-history` 472b83717 | run 44836134253 **failed to compile** (windows `check`, gates `clippy`) | Fix E0596, write the note, land |
| 2 unified WMO | `jr/wmo-unified-modes-v2` be2c5510c | run 44836128860: windows green; gates failed only on `player-clippy`'s 23 old findings | Merge main, rerun, land |
| 3 interruptible | `jr/spellcast-interruptibility` 523b31ac3 | run 44838235825: windows green; `player-clippy` has 24 findings, **one of them yours** | Fix the finding, merge main, land |
| 4 spirit-healer | `jr/spirit-healer-followthrough` 80b995089 | no run (docs only) | Land now |

## Exact fixes

**Ticket 1, `jr/combat-log-history`**
- Fix `error[E0596]: cannot borrow script as mutable` in `crates/benilla-ui/src/script/combat_log.rs`:
  - line 943, `let script = wrath();`, which is followed by `script.fire_event(` at 947;
  - the `script` used by `script.fire_event(` at line 1000, declared at 989.
  - Make both `let mut script`.
- Search the file for any other `wrath()` binding that calls `fire_event` and fix it the same way.
  The compiler stops at the first errors.
- The branch has **no note**. Write `.scratch/project-b/handoffs/2026-11-21-jr-combat-log-history.md`
  covering:
  - the ring size and retention from the reference, with addresses;
  - every cursor and filter native and what it now returns;
  - how `COMBAT_LOG_EVENT` and `_UNFILTERED` are split;
  - that DeepSeek's `SPELL_HEAL` and `SPELL_ENERGIZE` events reach the ring.

**Ticket 2, `jr/wmo-unified-modes-v2`**
- No code fix is needed. Merge `origin/main`, push, and watch the run.
- Your note says mode 2 "continues to select the window midpoint", while the ticket said our mode 2
  reuses `lit_window` instead of the reference's path. In the note, cite the `CMapObj::UnifiedRender`
  WINDOW-branch lines and MapObjU constants that show the midpoint is what the reference draws. If
  they show something else, implement that before landing.

**Ticket 3, `jr/spellcast-interruptibility`**
- Fix clippy `unnecessary_literal_unwrap` in `crates/benilla-app/src/wrath.rs`. Line 10010 is
  `let value = Some(definition.prevention_type != 1 || interrupt != 0 || silence != 0);` and
  line 10023 is `not_interruptible: !value.expect(...)`.
- Keep the plain `bool` and drop the `Some(...)` and `.expect`. If the `Option` is there for a
  real `None` case, restructure so that case actually produces `None`.
- Merge `origin/main`. It moves fast (`wrath.rs` and `wrath_native.rs` are busy), so resolve
  conflicts by keeping both sides' intent.

**Ticket 4, `jr/spirit-healer-followthrough`**
- This is docs only; the source was already right. Set the note to `Status: landed` and add the
  line "owner should see as a ghost". Then merge main and `git push origin HEAD:main`, as you did
  for ticket 0.

## Landing (each ticket)
1. Get CI green on the branch head. A gate that also fails on main's latest run, at the same step
   with the same error, does not block: name it in the note.
2. Merge `origin/main` into the branch and push.
3. Run `git fetch origin`, then `git push origin HEAD:main`, that ref only. If it is rejected
   because main moved, merge again and retry. No force-push and no rebase.
4. Set the note to `Status: landed <commit>`, with the run id and its result, and push the branch.

No local Cargo, no client and no server. Run at most 20 focused tests per ticket.

## Final report, one line per ticket
landed (commit and run id), blocked (the exact log or reference line) or not started.
