# Server rebuild batch

The owner's rule: **never rebuild the server for a single change.** Server fixes collect here
until the owner schedules one coordinated worldserver rebuild and restart. Add an item when a
fix needs the server; tick it off when the rebuild that carried it has run.

Server source: `D:\WOW\WM_BridgeLab\src\azerothcore` (mod-playerbots fork, branch `Playerbot`,
HEAD `5e741c3`). It carries many uncommitted local edits, including `Unit.cpp` (+217 lines).
Preserve them: edit on top, never reset, stash or switch branches there. The server source is
read-only unless the owner assigns the batch.

## Deployed 2026-11-17

Rebuild of 2026-11-17 01:47 (`state\ale-melee-build3.log`, 32 min, no errors), staged and
started 01:48. Previous worldserver kept as `run\bin\worldserver.20261007.exe.bak`.

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
- **Fix (deployed, not yet seen in game):** the shared melee path, so SmartAI, CombatAI and
  scripted creatures all get it. `UnitAI::DoMeleeAttackIfReady` (`UnitAI.cpp`, +8 lines) calls
  `me->Attack(victim, true)` once in reach when the unit has no `UNIT_STATE_MELEE_ATTACKING`.
  `Unit::Attack` then adds the melee state and sends `SMSG_ATTACKSTART` once.
- **Owner check still open:** fight a Northshire bandit pulled from range. The log should show
  `net: attack start 0xf130… → <player>` and the bandit should hold the ready stance between
  swings.

## Planned (not ready yet)

### 2. Souls combat server slice — no rebuild needed (ALE Lua)

Handoff `2026-11-14-souls-client-authority-direction.md` §6 H planned a C++ module and a core
edit in `UnitAI::DoMeleeAttackIfReady`. **Superseded 2026-11-18:** the whole slice is ALE Lua
(item 4), `integrations/ale/rustwrath_souls.lua` in the RustWrath repo. No core edit, no
rebuild, no restart.

- **Install:** copy `integrations/ale/rustwrath_souls.lua` into `run\lua_scripts`. ALE reloads
  it within a second. Deleting the file turns the whole slice off; `.reload ALE` reloads by hand.
- **What it does** (only for players whose client sent `MODE 1`, i.e. F10 on):
  - swallows the client's `RWSOULS` addon self-whispers and answers the same way;
  - blocks the player's `CMSG_ATTACKSWING` (their swings are the kit's);
  - deals the client's reported hits as stock physical damage (weapon roll × motion value,
    cleave falloff and blob share per owner decisions 6-10, riposte ×3 after a parry);
  - raises the run speed ×1.4 while the client sprints and lowers it ×0.5 while it crouches (with
    the stealth CREEP stand flag as the pose), restored on stop/F10 off/logout/reload;
  - counts each managed creature's stance down with the player's hits; at zero the creature is
    stunned 3 s (`EMOTE_STATE_STUN`, strike and casts cancelled) and a ×3 riposte opens;
  - takes over creatures fighting a Souls player: sets `UNIT_FLAG_PACIFIED` so stock melee stops,
    and drives telegraphed strikes (windup 40 % of the attack time, 0.6-1.5 s, creature stops and
    faces the victim), resolved by the client's verdict or the server's own reach/cone check.
- **Known side effect:** `UNIT_FLAG_PACIFIED` also blocks the creature's pacify-prevented
  (physical) spells while it is managed. Casters still cast.
- **Hand-back:** F10 off, logout, the creature leaving combat, or a script reload/shutdown clears
  the flag and resumes `MoveChase`.
- Item 1 (deployed) stays: it fixed stock melee, which every non-Souls player still uses.

### 3. Spellmaking submit shim (live rules)

`docs/live-rules/B-001B-1-contract.md` §5 (a): an exported `SpellmakingV3::SubmitCommand(Player*,
std::string)` and a `mod-wm-bridge` verb wrapping it. Status not rechecked since the contract;
confirm it is still wanted before adding it to a batch.

## Also deployed 2026-11-17

### 4. ALE (Eluna) server Lua, with auto-reload

Owner-approved 2026-11-16, for server-side Lua that can be hotpatched.

**Deployed 2026-11-17 and verified running** (see the Deployed section for the build).

- Clone at `D:\WOW\WM_BridgeLab\src\modules\mod-ale`, junctioned into
  `src\azerothcore\modules\mod-ale`. The folder **must** be named `mod-ale`: core
  `modules/CMakeLists.txt` wires up `lualib` only for a module matching `mod-ale` (named
  `mod-eluna`, it fails with "cannot open lua.h").
- **Pinned to `1ab53ba` (2026-03-24, detached HEAD).** It matches core `5e741c3` (2026-03-30)
  and playerbots `fdd228c` (2026-04-03). Newer ALE does not build here: `3eca176` (2026-04-17)
  changes `OnPlayerResurrect` for a newer core, and the later playerbots bindings need
  `PlayerbotAI` members this fork lacks (`state\ale-melee-build2.log`, 33 errors). Do not
  update ALE without updating the core.
- Adding a module: run `cmake .` in `build` first. The incremental build's reconfigure check
  only fingerprints the manifest's local modules, so it does not notice a new one.
- Runtime: `run\configs\modules\mod_ale.conf` (`ALE.AutoReload = true`, interval 1 s), scripts
  in `run\lua_scripts` (ALE `extensions` copied there). Edit and save a `.lua` file and it
  reloads by itself; `.reload ALE` does the same by GM command.
- **Verified:** editing `project_b_hello.lua` while the server ran made it rewrite
  `state\ale-hello.txt` within seconds (auto-reload works).
- **Known gap:** ALE's own log lines (`Logger.ALE` in `mod_ale.conf`) do not appear and no
  `ALE.log` is written. Module-config loggers seem not to be applied. To see them, add
  `Logger.ALE=4,Console Server` to `worldserver.conf` at the next restart.
