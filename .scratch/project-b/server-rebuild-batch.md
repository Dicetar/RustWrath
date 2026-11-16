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
- **Fix (in server source, not built yet):** the shared melee path, so SmartAI, CombatAI and
  scripted creatures all get it. `UnitAI::DoMeleeAttackIfReady` (`UnitAI.cpp`, +8 lines) calls
  `me->Attack(victim, true)` once in reach when the unit has no `UNIT_STATE_MELEE_ATTACKING`.
  `Unit::Attack` then adds the melee state and sends `SMSG_ATTACKSTART` once. The 2026-11-16
  rebuild compiled `game.lib` with it cleanly, but the worldserver was not linked or deployed
  because ALE (item 4) broke the modules build.
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

### 4. ALE (Eluna) server Lua, with auto-reload

Owner-approved 2026-11-16, for server-side Lua that can be hotpatched.

- Clone at `D:\WOW\WM_BridgeLab\src\modules\mod-ale` (azerothcore/mod-eluna `84b85cc5`). The
  folder **must** be named `mod-ale`: core `modules/CMakeLists.txt` wires up `lualib` only for a
  module matching `mod-ale` (named `mod-eluna`, it fails with "cannot open lua.h").
- Runtime is already prepared: `run\configs\modules\mod_ale.conf` (`ALE.AutoReload = true`) and
  `run\lua_scripts\project_b_hello.lua` (prints `[ALE] Project B lua_scripts loaded`).
- **Blocker:** `84b85cc5` is newer than this core and its playerbots fork. 33 errors, all in
  ALE: `PlayerScript::OnPlayerResurrect` signature, `WorldSession::IsHeadless`, and many
  `PlayerbotAI`/`PlayerbotFactory`/`PlayerbotAIConfig` members (`ALE_SC.cpp`,
  `methods/Playerbots/*`). Log: `state\ale-melee-build2.log`.
- **Next:** check out an ALE commit from the core's era (date of core `5e741c3` and the
  playerbots fork), or build with ALE's playerbots bindings off if it has a switch. Then
  re-create the junction `src\azerothcore\modules\mod-ale` → `src\modules\mod-ale` and run
  `cmake .` in `build` before the incremental build (its reconfigure check ignores new modules).
  The junction is currently removed so the next build does not fail.
