# Wrath trainers, chest and ore use

Status: in progress
Owner: Claude Code (Opus 5.5)
Branch: fix/wrath-trainer-gameobject-use
Issue: none (owner report 2026-11-17: four in-game failures)
Next: the owner's next batch build and launch checks the four in-game items below. The chest
and ore fixes are confirmed from source. The trainer causes are not confirmed: no logged run
shows a trainer visit.

Date 2026-11-17. Worktree `D:/Projects/wow/worktrees/fix-wrath-trainer-go-use`, cut from `main`
`78166894`. Reference `D:/Projects/wow/reference/wowdecomp-reference` at pin
`e67b6ffef23655dc7da061cb2d36b7963f6ef71a` (HEAD checked). No Cargo check, test or build was
run, and the client was not started.

Verification: CI: not run yet (filled in after the push).

## 3 and 4 · A chest cannot be opened; ore cannot be mined (cause confirmed)

**Cause.** `read_gameobject`
(`crates/benilla-protocol-wrath/src/gameplay.rs:3943` on `main`) read the 24 data dwords of
`SMSG_GAMEOBJECT_QUERY_RESPONSE` and threw them away. It returned `data: [0; 24]`.

- Every Wrath game-object template therefore reached `go_templates.rs` (`go_lock_slot`, line
  284) with lock id 0.
- `resolve_go_action` (`crates/benilla-app/src/target/click.rs:1687`) saw no lock. It answered
  `GoAction::Use`, so the client sent `CMSG_GAMEOBJ_USE`. It should have cast the lock's
  opening spell (Opening, Mining, Herb Gathering) at the object.
- The server refuses a bare use on a locked chest or a mineral vein, so nothing happened.

The comment on that code said the 3.3.5 block was indexed differently. The reference disproves
this. 12340 `CGameObjectDef::GetPropNum` (0x746190, prop tables at 0xADAD30..) keeps every 1.12
column at its 1.12 index:

- lock (prop 4): DOOR/BUTTON `data[1]`; CHEST, QUESTGIVER, TRAP, GOOBER, AREADAMAGE, CAMERA,
  FLAGSTAND and FLAGDROP `data[0]`; FISHINGHOLE `data[4]`;
- page (prop 15): TEXT `data[0]`, GOOBER `data[7]`;
- quest background (prop 17): QUESTGIVER/TEXT `data[2]`, GOOBER `data[9]`;
- meeting stone (props 51-53): `data[0..2]`.

Wrath only appends props after these.

**Fix.**

- `gameplay.rs:3940` keeps the 24 dwords. The negative-entry (unknown object) branch still
  answers zeros, as before.
- Wrath also adds a lock-slot type 3: a spell id that opens the lock itself when that spell
  has an `OPEN_LOCK` effect. This is 12340 `CGGameObject_C::IsLocked` 0x70F160. The fix adds:
  - `LOCK_KEY_SPELL` in `crates/benilla-formats/src/lock.rs`, exported from `lib.rs`;
  - a `LOCK_KEY_SPELL` arm in `resolve_lock`, `crates/benilla-app/src/target/lock.rs:177`,
    which answers `OpenBySpell(slot.index)`.

  Skill (type 2) and key (type 1) locks were already handled the Wrath way, by an Effect[0]
  opener list and `GetSpellRank/5` skill points.

**Check in game.**

- Right-click a locked chest (a Northshire/Elwynn footlocker or a quest chest). Expect the
  "Opening" cast bar, then the loot window.
- Right-click a Copper Vein with Mining learned. Expect the Mining cast bar, then ore in the
  loot window.
- Hover the vein. The tooltip should name the lock's skill ("Requires Mining" and the rank).
  A vein above your skill should show the refusal message instead of casting.
- Herb nodes use the same path; a Peacebloom is a good second check.

## 1 · A class trainer shows no abilities to train (cause not confirmed)

None of the nine launcher logs or 314 interaction-diagnostics files under
`D:/Projects/wow/client/benilla-config/Logs` records a trainer visit:

- `Blizzard_TrainerUI` was never loaded, only listed as "LoadOnDemand — not loaded".
- No `ui_trainer` line appears.

So the failing run was not captured.

These were checked from source and match 12340 and AzerothCore:

- the `SMSG_TRAINER_LIST` reader (`read_trainer_list`, gameplay.rs, matches
  `CGPlayer_C::OnTrainerList` 0x6D12C0 and AC `NPCPackets.cpp:25-46`);
- the `CMSG_TRAINER_LIST` and `CMSG_TRAINER_BUY_SPELL` encoders and opcodes (0x1B0/0x1B2);
- the state values (0 available, 1 unavailable, 2 used);
- the default type filter of 3, which is the 12340 `serviceTypeFilter` CVar default
  (GameUI_CVar.cpp:601);
- `TRAINER_SHOW` → UIParent `ClassTrainerFrame_LoadUI` → `Blizzard_TrainerUI`;
- the `BuyTrainerService` gate (state 0 only, as `CGPlayer_C::TrainerBuySpell` 0x594DA0);
- `UnitCharacterPoints` cp2, which `wrath.rs` maps from `PLAYER_CHARACTER_POINTS2` 1021 →
  1103;
- the skill-line lookup through `SkillLineAbility`/`SkillRaceClassInfo`.

The class trainer did differ from 12340 in two places, and both are fixed here.

**State re-derivation.** The donor's 1.12 pass `re_derive`
(`crates/benilla-app/src/ui_trainer/reeval.rs:66`) treats a row as known only through its
`LEARN_SPELL` effects. An AzerothCore 3.3.5 class trainer lists the ability itself, which has
none. As a result, after the first money, skill or level change while the window was open:

- every ability the player already knows came back as "available";
- a Train click on such a row is refused by the server;
- at a type-1 trainer the pass could also set the hidden state 3, which 12340 does not have.

The fix ports 12340 `CGClassTrainer::RefreshList` (0x596B00, ClassTrainerFrame.cpp:390-596) as
`re_derive_wrath` (reeval.rs:178). The rules are:

- A non-learn slot stands for the wire spell itself, and a `SKILL_STEP` slot counts once the
  step is reached. A row whose slots are all known is used.
- Otherwise, a short skill, any missing required ability, or a level above 1 the player lacks
  makes the row unavailable.

The feed selects it for the Wrath VM (`ui_trainer/mod.rs:656`). The 1.12 pass and its tests
are unchanged.

**Learn-spell hop.** 12340 `GetSkillLineFromService` (0x594AE0) skips a `LEARN_SPELL` slot aimed
at the pet (`ImplicitTargetA == 5`). The donor took the first `LEARN_SPELL` slot, and could
group such a row under the pet spell's line or drop it. Fixed for the Wrath layout in
`crates/benilla-formats/src/spells/mod.rs:566`; `learned_spell` has no other caller.

Neither difference empties the list when it first opens. If the window is still empty,
these are the candidates left:

- Every row dropped by the group key, logged as "has no skill line — dropped from the tree"
  (`ui_trainer/mod.rs:324`, debug level).
- `feed_trainer` returning before `TRAINER_SHOW` because `Spells` or `SkillLines` is missing
  (`ui_trainer/mod.rs:548-553`). In that case the window does not open at all.

**Check in game.**

- Talk to Llane Beshere (Northshire warrior trainer) and choose the training option.
  - Expect his list with green rows for what you can learn now and red rows for higher levels.
  - Known abilities are hidden by the default filter. Ticking "Used" in the filter shows them
    grey.
- Train one ability. It should leave the green rows, and the money should drop.
- If the window is empty or does not open, run once with trainer debug logging and send the log
  lines that mention `ui_trainer` or `TrainerUI`:

```powershell
$env:RUST_LOG = "info,benilla_app::ui_trainer=debug"
```

## 2 · Nothing can be learned from a profession trainer (cause not confirmed)

The same evidence gap applies: no logged trainer visit. Checked as matching 12340:

- the wire;
- the first-rank confirmation, cpCost2 = `PointCost[1]` with `UnitCharacterPoints` cp2;
- the buy gate.

The fixes above also apply here:

- After the first trigger, a profession rank the player already has (for example Apprentice
  Mining) now becomes used instead of available.
- **Tradeskill grouping.** 12340 `AddServices` (0x596450, ClassTrainerFrame.cpp:774-790) puts a
  service whose spell has `Attributes & 0x40` in a third group, "Passive"
  (`TRADESKILL_SERVICE_PASSIVE`). The donor only had "Development Skills" and "Recipes". Fixed
  in `crates/benilla-app/src/ui_trainer/law.rs:226-229`.

**Check in game.**

- At a profession trainer the player has no rank in (the Stormwind or Goldshire mining or
  herbalism trainer), choose training.
  - Expect an "Apprentice" row under Development Skills. Train is enabled when you have a free
    profession slot.
  - Train asks to confirm the new profession. Accepting teaches it, and the row turns used.
- With the profession learned, the recipe or ability rows (Find Minerals, Smelt Copper) should
  be learnable.
- If nothing is learnable, note whether the Train button is disabled or clicks without effect.
  Run with the logging line above and send the `ui_trainer` lines (a refusal logs
  "ui_trainer: … (code N)").

## Not done

- Not compiled, not run, no tests. CI is the first compile.
- `spell_opens_lock` (`target/lock.rs`, the cursor predicate when targeting a spell at an
  object) still knows only skill and key slots, not the Wrath type-3 slot. This affects the
  cursor only, not right-click opening.
- No new test for the kept GO data block or `re_derive_wrath`.
