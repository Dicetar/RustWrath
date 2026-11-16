# Server rebuild batch

The owner's rule: **never rebuild the server for a single change.** Server fixes collect here
until the owner schedules one coordinated worldserver rebuild and restart. Add an item when a
fix needs the server; tick it off when the rebuild that carried it has run.

Server source: `D:\WOW\WM_BridgeLab\src\azerothcore` (mod-playerbots fork, branch `Playerbot`,
HEAD `5e741c3`). It carries many uncommitted local edits, including `Unit.cpp` (+217 lines).
Preserve them: edit on top, never reset, stash or switch branches there. The server source is
read-only unless the owner assigns the batch.

## Ready

### 1. Creatures that aggro from range never enter melee state

- **Symptom:** a bandit drops to the idle Stand animation between swings instead of the
  combat-ready stance (owner report and log 2026-11-16, `Logs/Launcher/20261116-232635-012`).
  The bandit swung 10 times and sent `SMSG_ATTACKSTOP` at death, but never sent
  `SMSG_ATTACKSTART`.
- **Cause:** `src/server/game/AI/SmartScripts/SmartAI.cpp:896`,
  `me->Attack(who, me->IsWithinMeleeRange(who))`. A creature that aggroes outside melee range
  starts with `meleeAttack = false`. `Unit::Attack` then sends the hostile AI reaction but no
  `SMSG_ATTACKSTART` (`Unit.cpp:7614` vs `:7631`). Nothing in this version switches melee on
  later; `DoMeleeAttackIfReady` swings without it.
- **Client contract:** 12340 shows the ready stance only while `m_combat.m_attacking` is set,
  and only `SMSG_ATTACKSTART` (or create data) sets it (`UnitCombat_C.cpp:753`,
  `Unit_C.cpp:2608`). The stock client would show the same idle stance. RustWrath mirrors that
  through `Engaged` (`creature_anim/net.rs` `attack_start`), so no client change is needed.
- **Fix:** take upstream AzerothCore's version. `SmartAI::AttackStart` passes
  `mCanAutoAttack && !_currentRangeMode`, and `SmartAI::UpdateMeleeStance()` (called from
  `UpdateAI` before `DoMeleeAttackIfReady`) re-calls `Attack(victim, canMelee)` when a ranged
  creature enters or leaves melee reach. It needs `SetCurrentRangeMode` to match upstream too.
- **Check after the rebuild:** fight a Northshire bandit pulled from range. The log should show
  `net: attack start 0xf130… → <player>` and the bandit should hold the ready stance between
  swings.

## Planned (not ready yet)

### 2. Souls combat server slice

Handoff `2026-11-14-souls-client-authority-direction.md` §6 H: a new module beside
`mod-multiclass` (swing/hit/verdict messages), the core edit in `UnitAI::DoMeleeAttackIfReady`
(strike cast while the mode is on), gating player auto-attack and the player-swing damage maths.
It changes the same melee path as item 1, so the two belong in one rebuild. The build order is
still an open owner decision; the client slices come first.

### 3. Spellmaking submit shim (live rules)

`docs/live-rules/B-001B-1-contract.md` §5 (a): an exported `SpellmakingV3::SubmitCommand(Player*,
std::string)` and a `mod-wm-bridge` verb wrapping it. Status not rechecked since the contract;
confirm it is still wanted before adding it to a batch.
