# OpenCode swarm: 2026-11-21

Owner: the OpenCode DeepSeek overseer, running AutoSwarm. This package replaces the task list
of [the 2026-11-20 package](2026-11-20-opencode-swarm.md); that package's swarm rules still
apply. The workers used free models first and the main DeepSeek model only where a free ticket
failed review twice or every attempted free model returned a quota limit.

## Swarm result 2026-11-21

1. **`SWING_MISSED` in the Wrath combat log: landed.** `oc/swing-missed-victim-state`
   (637142df): miss, dodge, parry, block, evade, immune and deflect from
   `SMSG_ATTACKERSTATEUPDATE` hit info/victim state, in 12340's check order, with the reference's
   own miss strings; a zero-damage block carries its blocked total.
   `oc/swing-missed-absorb-resist` (de9e0e4c): absorb (hit info 0x20) before resist (0x80), each
   with its summed amount, zero included. Both CI runs: Windows check and changed-crate tests
   green; gates red only on the pre-existing `player-clippy` debt. Not proven in game: that a
   live swing reaches the stock combat-log frame or that a real server sends the values the
   reader sums. The 12340 facts are in the two notes.
2. **Pre-existing `player-clippy` debt: partly cleared, gates still red.** The gates test
   failure was a separate new problem and is fixed: `GameTooltip:AddTexture` now has its
   `ALLOWED_BEYOND` row (40649fd7), so
   `our_widget_methods_stay_inside_the_1_12_surface` passes. The whole
   `benilla-protocol-wrath` layer is now clean: `multiclass.rs` (a989dd57), `session.rs`
   (75ce5eee), `reducer.rs` (0a7c75b0), `world.rs` (85cd0d14), `header.rs` (847ca80c) and
   `gameplay.rs` (a53bcc47). `benilla-assets/src/profile_source.rs` is clean (b6e4fcb2).
   In `benilla-ui`: `secure.rs` (5a76a5d4), `channel.rs` (73d85490), `glue_select.rs`
   (8cf7deea) and `button.rs` (7692ca1b). `benilla-app/src/wrath.rs` is clean
   (f7bf06c7): the 14 findings named by the owner, including the dead store and the two
   never-used control methods. The gates job is still red on the remaining
   `player-clippy` files: about 55 findings across roughly 30 files, mostly `benilla-app` (`quest_trace.rs`
   and `ui_items/mod.rs` carry the most) and `benilla-ui` (`wrath_natives.rs` 4,
   `macros/bind.rs`, `unit/mod.rs`, `addon_message.rs`, `keybind/clicks.rs`,
   `spellbook.rs`, `talent.rs`, `unit/bindings.rs`). A green gates job needs one file per
   ticket for those; this session landed 12 of them and did not finish the list; main after the batch is 7692ca1b.
3. **Gap lists for issues 19-24 and 28-32: landed.** Eleven issue notes, in number order, each
   with the acceptance table, a real file/symbol per row, small ticket descriptions and explicit
   `Blocked: needs owner run` rows: 19 (429a2c50), 20 (475473d5; overseer removed a duplicate
   section), 21 (c1b84a6b), 22 (63a172d9), 23 (c740f04b; duplicate removed), 24 (6c388b8d),
   28 (7a986116), 29 (6cbbf9bc; the client-only Spellwright stub ticket was rejected), 30
   (495b2a76; three `wm-project` tickets marked outside this repository), 31 (12f1a32b) and
   32 (2bf21854). Every live-only acceptance line is marked `Blocked: needs owner run`; the
   tables do not claim visible behavior. Issues 25-27 keep last night's lists.
4. **`SPELL_*` combat-log events: not started.** The Wrath spell packets and the 1.12 spell
   lines exist, but the `COMBAT_LOG_EVENT_UNFILTERED` feed still has no
   `SpellDamage`/`SpellMissed`/`SpellHeal`/aura event queue. The next slice is a
   `WrathFeedEvent` family per event with the 12340 `PushEvent` argument block; no ticket was
   written or attempted tonight.

**Workers and models.** 24 tickets ran on free models: OpenRouter free (Flint, Ling 3.2 Flash,
Nemotron 3.5 Lightning, Nemotron 3 Ultra, Baidu Ernie 5.5 Flash, Particle-1, Gemma 4 31B) and
OpenCode free (Iceman, Longcat 2.5, Mimo v2.6 Flash, Beam, Hy4 Flash, DeepSeek V4.1 Flash Free).
Two tickets fell back to the overseer's paid DeepSeek: `wrath.rs` after the free models failed
review twice (a broken alias commit, then an incomplete rewrite), and `button.rs` after the
remaining free models returned `free-limit` before editing. Two free-model tickets were finished
by the overseer after the worker hit a rate limit with the work already committed or uncommitted
(`swing-missed-victim-state`, `swing-missed-absorb-resist`); no behaviour was authored by
DeepSeek in those two.

**CI.** Every landed code ticket's branch had Windows `check` and its changed-crate tests green
before landing; the gates job was red only on the pre-existing `player-clippy` debt, except
four runs where the gates `test-build` hit a missing `wayland-sys` system library on the
runner (the Windows job passed; the branch was landed on that and on source review). Main's own
run is re-built after this batch. No local Cargo, client or server was run; no `target` was
created. `origin` received no credentials, game data or extracted interface files.

**What is not proven.** Any in-game swing, block, absorb or resist row in the stock combat-log
frame; the gap-list acceptance lines that need a live client or server; and a green gates job.
The owner's next batch should read the remaining `player-clippy` files (the files named above) and
run the solo Northshire route, which is the only check for the blocked rows.
