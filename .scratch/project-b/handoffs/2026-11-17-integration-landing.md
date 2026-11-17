# Integration landing, 2026-11-17

Status: landed on main; not compiled or run locally (CI results below)
Owner: Claude Code (Opus 5.5), integrator
Branch: main
Issue: none (integrator brief 1, `docs/rustwrath-agent-briefs-2026-11-17.md` in `D:\Projects\wow`)
Next: the owner's batch build and launch, using the owner run list at the end of this note

No Cargo, client or test run was made locally. Compilation and tests come only from GitHub CI,
read as `docs/agents/sync.md` "Reading CI" says.

## 1. What landed on main

| step | branch | task note | main merge commit |
|---|---|---|---|
| 1 | `docs/wow-workspace-paths` | `2026-11-17-wow-workspace-paths.md` | `1b8c9f3d` |
| 2 | `fix/main-build-2026-11-16` | `2026-11-16-auth-rs-text.md` | `40453c88` |
| 2 | `fix/wrath-cast-outcomes` (B4) | `2026-11-15-wrath-cast-outcomes.md` | `b13c163d` |
| 2 | `fix/wrath-world-fidelity` (B3) | `2026-11-15-wrath-world-fidelity.md` | `3df56f87` |
| 5 | `fix/wrath-death-cycle` (A1) | `2026-11-15-wrath-death-cycle.md` | `f01a6e02` |
| 5 | `fix/wrath-melee-loop` (A2) | `2026-11-15-wrath-melee-loop.md` | `58e98b0d` |

Each code branch first had `origin/main` merged into it in a worktree under
`D:\Projects\wow\worktrees\`. Its source was then read against its task note and merged with
`--no-ff`. No file was reformatted. Between steps 2 and 5 the owner pushed `0000279e`, which
runs CI on every branch, and each remaining branch merged it before landing (B3 `cfa5f997`,
A1 `8a73f4ea`, A2 `c5432f35`). A trial merge of B3, A1 and A2 together onto `0000279e` was
clean. I also checked for cross-branch compile breaks: A1 adds fields to `WrathEntity`, and
every `WrathEntity { … }` literal on the combined tree either sets them or uses `..base`. No
branch adds an exhaustive match over the new `SessionEvent` variants.

Conflict resolutions:

- **B4** (`fix/wrath-cast-outcomes`): one conflict in `crates/benilla-app/src/wrath.rs`. Main's
  side was kept, plus B4's test `an_interrupt_does_not_fail_the_same_cast_twice`. The details
  are in the B4 note's "Integration 2026-11-17" section.
- B3, A1 and A2 merged main without conflicts. B3's 9-byte `SMSG_WEATHER` parser is reached
  because 0x2F4 is in `wrath.rs::is_shared_12340_server_opcode`.

Review notes beyond the task notes:

- **A2**: the 12340→1.12 hit-info bit map and the school mask→index map match how AzerothCore
  writes them. One gap predates A2 and A2 doesn't widen it: `creature_anim/blood.rs:110` takes
  `hit_info & 0x2000` as "crushing" and uses it for the large blood spurt. After A2, a Wrath
  block maps to 1.12 0x800 and crushing to 0x8000, so neither picks the large spurt. The A2
  note names this as a donor reading.
- **A1**: the four new opcodes match AzerothCore `Opcodes.h` (0x378, 0x494, 0x2E4, 0x506; AC
  calls 0x506 `SMSG_CORPSE_NOT_IN_INSTANCE`). The corpse block shift (+2 from index 10) matches
  both layouts. A side effect that nothing reads: `CORPSE_FIELD_PARTY` (8/9) lands on the 1.12
  facing/pos_x slots, and no donor code reads a corpse's 8..=11.

## 2. CI

Main's baseline is run 44087313491 on `0000279e`. `gates` fails at `fmt` (2497 hunks in 407
files), so clippy and the Linux tests were skipped. On `windows`, the test build stops at
`crates/benilla-protocol/tests/items.rs:287` and `:461`
(`error[E0063]: missing field wrath_roll in initializer of ItemInfo`), so no test ran on main.
B3 contains the fix for that error.

Each run below was on the exact head that landed:

- **B3, CI 44097706049: compiles; tests 53 failures, none in a file B3 changes.** Its test
  build gets past main's `items.rs` error, so it is the first run on this tree where tests ran:
  9 targets had failures, 40 of them in `benilla-app --lib`. The failing tests are in auth
  (time-zone bias), the Lua strftime/date tests, `benilla-hotpatch` quarantine, `benilla-ui`
  (reputation slots, reference surface, quest POI), the quest verbs, the schedule and lane
  guard walls, the Wrath item/inventory rows, `launcher_shape`, `message_keys`,
  `ui_script_wall` and `vm_parity`. The known stale
  `a_second_esc_resends_the_strikes_cancel_and_the_echoes_fire_nothing` is among them. None is
  in the weather, liquid-loop, sheath, `world_point` or weather-parser code B3 touches. Main has
  no name-by-name baseline (its tests don't build); the 2026-11-15 baseline was about 95.
- **A1, CI 44098499926: compiles.** Its only compile error is main's `items.rs` `wrath_roll`
  one, which B3 fixes. Because of that error, no tests ran on the branch.
- **A2, CI 44098510458: compiles.** Same as A1: only main's `items.rs` error, no tests ran.
- **main after landing, CI 44105571020 on `58e98b0d`: not finished when this note was
  written.** It is the first run of A1's and A2's tests, and the next agent should compare its
  failures with B3's 53. The run on `3df56f87` (B3 alone) was cancelled by the next push to
  main, as the per-ref concurrency rule does.

`fmt` hunks added on top of main's 2497, all inside files each branch touches: B3 +10, A1 +9,
A2 +4. `cargo fmt` was not run (no local Cargo). The owner may want one formatting commit across
the tree once `fmt` is to go green; reformatting `wrath.rs`, `gameplay.rs` or `opcode.rs` was
outside this brief.

## 3. Branches without notes (step 3); none deleted

- **`argon/say-chat-log`** (`bc9b9dd8`, 1 ahead of main): **land**, after a main merge, a
  task note and CI. It fixes a real bug on main. `benilla-formats/src/languages.rs` reads
  `ChrRaces` BaseLanguage at field 8, but in 3.3.5 field 8 is CreatureType (AC
  `DBCStructure.h:714` puts the language at 7), so every race falls back to Common. It also
  fixes the chat tag: AC writes a bitmask (`Player.h:838-843`: AFK 0x01, DND 0x02, GM 0x04),
  while the donor reads an enum, so GM and combined tags show wrong. It adds Draenei (35), a
  self-player fallback in `seed_default_language` and a test. Its helpers all exist on main,
  and main hasn't touched its four files since its base.
- **`argon/field-mapping-audit`** (`04c16dbc`): **drop (superseded)**. Main's `01c99a42` (the
  live field table `wrath_field_map.toml`, with gameobject, dynamicobject and corpse sections)
  and `34ce8ed8` cover it. `34ce8ed8` took this branch's creature unit copies and rejected its
  GO rotation, artkit/animprogress and position-word changes. One piece is not on main yet:
  the player honor words (`PLAYER_FIELD_KILLS` 1225→1250, `TODAY_CONTRIBUTION` 1226→1254,
  `YESTERDAY_CONTRIBUTION` 1227→1257, `LIFETIME_HONORABLE_KILLS` 1228→1255, `BYTES2`
  1229→1260). If wanted, add them as `[[player]]` rules in the field table, not by merging this
  branch.
- **`fix/wrath-action-spell-slot-a`** (`eaf28de6`, 1 owner commit ahead, about 240 behind; the
  other owner commit `c08563f1` is already on main): **land**, after a main merge, a note and
  CI. `spellbook.rs` and `wrath_spell.rs` haven't changed on main since `c08563f1`, so it
  applies to the code it was written against. `action_bar_spell_reference` returns
  `Option<u32>` and refuses the pet book, and `book_cooldown` asks
  `host.spell_cooldown(spell_id, false)` without a `Spell.dbc` gate. That matches 12340
  `Script_GetSpellCooldown` (`SpellBookFrame_Scripts.cpp` ~504, `0x540E80`), which calls
  `Spell_C_GetSpellCooldown` with no DBC lookup. It adds 3 tests.

## 4. Servers (step 4)

- `D:\Projects\wow\server\WM_BridgeLab\run\configs\worldserver.conf`: one line added,
  `Logger.ALE=4,Console Server`, after `Logger.module` (line 714). The backup is
  `worldserver.conf.bak-2026-11-17-before-ale-logger` beside it. The server wasn't rebuilt.
- Started following `docs/agents/local-launch.md`'s steps, as the owner's .bat does: MySQL
  (lab helper, port 33307), authserver (3724), then worldserver (8095, `-c
  configs\worldserver.conf`, working directory `run`, started 14:59:44). All three were
  running at 15:03. SOAP, GM passwords and the `WM_SOAP_*` variables were not touched. No login
  and no client.
- ALE evidence: `state\ale-hello.txt` reads `ALE loaded project_b_hello.lua at 2026-11-17
  15:00:21`. Server.log has `[ALE]: Searching scripts from lua_scripts` and `[ALE]: Executed 4
  Lua scripts in 28 ms`, so the new logger line routes the ALE category into Server.log.
- **The literal `[ALE] Project B lua_scripts loaded` is not in Server.log, and it can't be.**
  `project_b_hello.lua` writes it with Lua's own `print()`, which goes to worldserver's console
  (stdout), not to the logger. This worldserver was started hidden, so the line went nowhere.
  To get it into Server.log, use `PrintInfo("[ALE] Project B lua_scripts loaded")` in the
  script (ALE binds `PrintInfo` to `ALE_LOG_INFO`, `GlobalMethods.h:2570`). Or start
  worldserver in a visible console. The server tree is the owner's; I didn't change the script.
- `configs/modules/mod_ale.conf` also sets `Logger.ALE=4,ALELog ALEConsole` (`ALE.log`). No
  `ALE.log` was created, so the `worldserver.conf` line is the one in effect.

## 5. Not done, on purpose

- No remote branch or worktree was deleted. `sync.md` gives that cleanup to the integrator,
  but it waits on the owner's word. The scratch worktrees `land-main`, `land-a1` and `land-a2`
  are still there. `trial-all` is pruned; its folder is empty and the app holds it.
- The shared checkout `D:\Projects\wow\client` was not pulled or switched, as the brief says.

## Owner run list (next batch build)

Build (from the docs move):

1. The path-literal edits compile (`benilla-app` `ui_script/addons.rs`; `benilla-protocol-wrath`
   `session.rs`, `gameplay/social.rs`, `gameplay/wrath_systems.rs`). Before building, read main's
   CI 44105571020 (section 2): it is the first build of all six landings together.
2. Note whether the new checkout path `D:/Projects/wow/client` makes Cargo/sccache do a full
   rebuild. Setting `CARGO_TARGET_DIR` to the old spelling
   `D:\Projects\project-b-client\target` may avoid it (unverified). Keep the `D:/WOW` spelling
   for server builds.

Cast outcomes (B4):

3. Cast a damage spell on a mob: one cast bar, one result, damage and power shown.
4. Cast out of range and while moving: the failure text, no duplicate line.
5. Get hit during a cast: "Interrupted" once.
6. A channelled spell: start, ticks, stop on cancel and on being interrupted (one terminal).
7. A cooldown spell twice: the cooldown start and the second press's refusal.
8. Auto Shot: start it, deselect/let the target die — no red "Interrupted" line; a genuine
   interrupt of the shot still shows it.
9. One real logout export shows one `confidence=exact` chain per cast. The stale test
   `a_second_esc_resends_the_strikes_cancel_and_the_echoes_fire_nothing` already fails on main.

World fidelity (B3):

10. Zone change with weather (Elwynn → a rainy zone): rain renders, ambience starts, clears.
11. A pool/river: the loop fades in within a second and stops when you dive.
12. Draw and sheathe melee, a shield, a bow and a gun: the model moves to hand/back/hip
    correctly.
13. Northshire Abbey interior: no missing ambience or lighting regression.

Death cycle (A1):

14. Die to a mob, then release, run back and reclaim.
15. Die again and use the spirit healer.
16. Die near a second character, then accept their resurrection.
17. Watch the camera across the release teleport (left to A3's controller work; A1 risk 6).

Melee loop (A2):

18. Target a Northshire wolf and auto-attack it to death.
19. Read the misses/dodges in the combat log.
20. Check the XP gain line and the XP bar.
21. Check combat ends (leaves combat, no stale target).
22. Check the bandits keep their combat stance between swings: **check the bandits no longer
    drop to idle between swings.**
23. Watch blocks and crushing blows: the large blood spurt is still keyed on the old bit
    (section 1).

Servers:

24. worldserver is already running with ALE logging. `[ALE] Project B lua_scripts loaded`
    shows only in a visible console, or in Server.log after the `PrintInfo` change (section 4).
