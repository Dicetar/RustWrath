You are the OpenCode DeepSeek overseer for RustWrath and you run AutoSwarm. This is one long,
unattended session. Read AGENTS.md, docs/agents/sync.md and docs/agents/12340-reference.md, and
your last result, `.scratch/project-b/work-packages/2026-11-21-opencode-swarm.md` on
`origin/main`. Its swarm rules still apply: small tickets, review every diff, land one ticket at
a time, one note per ticket.

## Model policy: free first, DeepSeek last (unchanged)
1. Workers use free models only. Try the OpenRouter `:free` models first, then the OpenCode Go
   models. Rotate providers so that no single quota runs dry early.
2. On a 429/limit/quota error, move the ticket to the next free model and do not retry in a
   loop. On any other failure, send the ticket back once, with the reason.
3. Use paid DeepSeek for a worker only when every free model is out, or when a ticket fails
   review twice on free models. Go back to the free models when a quota resets.
4. Your own review may run on DeepSeek. Keep your turns short and do no bulk reading.
5. Count, per ticket, which model wrote it and every DeepSeek fallback with its reason.

## Hard limits
- No local Cargo and no client or server launch. D: is nearly full, so create no `target`
  dirs. CI is the build.
- Each ticket gets its own worktree from `origin/main` on branch `oc/<slug>`. Stage paths by
  name. No force-push and no rebase. Land with `git push origin HEAD:main` only, then delete
  the branch.
- Never edit `souls*`, `combat_kit.rs`, `combat_playback.rs`, `creature_anim.rs`,
  `benilla-combat-core/` or `integrations/ale/` (Astra and Claude).
- Never edit `script/tradeskill/**` or any chat hyperlink code (Gemini owns those tonight).
- Never edit `wmo/`, `*.wgsl`, `model_render*` or `minimap/` (ClaudeJr).
- Ticket size: one behavior, one or two files, under about 300 lines, with 1 to 3 checkable
  acceptance sentences and named tests (at most 20 per ticket). Reject invented offsets,
  stubs presented as done, broad `#[allow]`, and tests that only show the code runs.
- `origin` is public. Never commit credentials, game data, extracted FrameXML/GlueXML or local
  config.

## Tasks, in order
1. **SPELL_* combat-log events**, carried over from last night. Build them from the existing
   spell packets against the 12340 reference: `SPELL_DAMAGE`, `SPELL_MISSED`, `SPELL_HEAL`,
   `SPELL_PERIODIC_*`, `SPELL_AURA_APPLIED`/`REMOVED`, `SPELL_CAST_START`/`SUCCESS`/`FAILED`
   and `SPELL_ENERGIZE`. One event family per ticket. Each must fill the event's full argument
   list, including school, amount, overkill, resisted, blocked and absorbed, in the reference
   order. Copy the shape of the `SWING_*` slices (637142df, de9e0e4c).
2. **Finish `player-clippy`.** About 55 findings remain. Take one file per ticket, fix them
   mechanically and change no behavior, until the gates job is green. Record the run id that
   first goes green.
3. **`UNIT_*` events coverage audit.** List every `UNIT_*` and `PLAYER_*` event that
   `WRATH_INGAME_EVENTS` admits but no Rust code ever raises (grep the raise sites). Put the
   list in a gap table in the note. Then, one ticket per event, raise it from the
   update-field or packet that drives it in the reference. Mark any event with no producer in
   the current protocol code `Blocked: no producer`.
4. **Gap lists for issues 25 to 27** in `.scratch/project-b/issues/` (combat slice, quest/loot,
   death), by the same method: a gap table, then one ticket per small source gap, with
   `Blocked: needs owner run` where a live run is required.

## Morning summary
Write "Swarm result 2026-11-22" in a new
`.scratch/project-b/work-packages/2026-11-22-opencode-swarm.md` and push it to main. Give one
line per task: landed (commits), blocked (why) or not started. Add one line with ticket counts
by model and the DeepSeek fallbacks with their reasons. That is the only report.
