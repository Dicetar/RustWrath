# Server changes (live only)

The owner's rule (2026-11-20): **server changes are hotpatches only — no rebuild, no restart.**
A server fix is ALE Lua in `run\lua_scripts` (auto-reload, or `.reload ALE`), a module config
line plus `.reload config`, or a world-DB row a running server re-reads. A fix that can only be
done in C++ is written down here as **not live**, with what it would take, and is not scheduled;
the owner decides. Items 1 and 4 below predate this rule.

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

### 5. Random enchants roll any tier, level-60 enchants included (bug; live fix in ALE)

- **Symptom (owner, 2026-11-20):** Firna's crafted Brown Linen Shirt shows "+16 Stamina and +100
  Armor" in its tooltip, but her stats and armor are the same with and without it.
- **Why the stats are missing:** the roll gave enchant **2716** (Fortitude of the Scourge,
  `SpellItemEnchantment.RequiredLevel` 60). `Player::ApplyEnchantment` (`PlayerStorage.cpp:4418`)
  applies nothing below the enchant's level, which is stock behavior. The enchant will apply at 60.
- **Root cause:** `modules/mod-random-enchants/src/random_enchants.cpp`, `getRandEnchantment`.
  Its query reads ``WHERE `tier`={} AND `exclusiveSubClass`=NULL AND exclusiveSubClass='{}' OR
  `class`='{}' OR `class`='ANY'``. `=NULL` is never true, and `AND` binds before `OR`, so the
  query is effectively `class='ARMOR' OR class='ANY'`. **The tier is ignored**: a white item
  draws from the whole table. Nothing checks the enchant's level either.
- **Live fix (2026-11-20):** `integrations/ale/rustwrath_random_enchants.lua` takes over the
  roll. It keeps the module's events, chances, quality-to-tier roll, slots and chat line, runs
  the corrected query ``WHERE `tier`={} AND (`exclusiveSubClass` IS NULL OR
  `exclusiveSubClass`={}) AND (`class`='{}' OR `class`='ANY') ORDER BY RAND()``, and takes the
  first row whose `SpellItemEnchantment.RequiredLevel` (read once from the server's
  `<DataDir>\dbc\SpellItemEnchantment.dbc`) is at most `max(player level, item RequiredLevel)`.
  - **Switch-over (owner step):** set `RandomEnchants.Enable = 0` in
    `run\configs\modules\random_enchants.conf`, then `.reload config`. The module reads the
    switch on every roll. Until then the script stays idle, so nothing is rolled twice. Its
    load line in the console says which state it is in.
  - The module SQL's tiers: tiers 1-2 hold no level-gated enchants; tier 3 has four level-60,
    tier 4 two level-50 and two level-60 (2716 is tier 4 ARMOR), tier 5 one level-50. The level
    filter keeps every tier safe.
- **Not run:** no item has been rolled through the script yet.
- **Existing items** keep their rolls, so the shirt keeps 2716 and works from level 60. Re-rolling
  them is the owner's call.
- **Client, landed 2026-11-20:** the tooltip now prints an enchant above the player's level in
  requirement red, followed by "Enchantment Requires Level 60", as 12340 does (`Tooltip.cpp`
  `levelOk`). It used to show it green, which made the enchant look active.

### 6. Crafting while moving (feature; live in ALE and client live code)

Owner request 2026-11-20: tradeskill casts keep going while the player moves.

- **Server (live):** `integrations/ale/rustwrath_crafting.lua`. On load it walks Spell ids
  1-100000 and, for every spell with `SPELL_ATTR0_IS_TRADESKILL` (0x20, the recipe-list flag),
  clears `SPELL_INTERRUPT_FLAG_MOVEMENT` (0x1) with `SpellEntry:SetInterruptFlags`, which writes
  the `Spell.dbc` store and the live `SpellInfo`. Both server gates read that flag: the start
  check `Spell.cpp:3517` (`SPELL_FAILED_MOVING`) and the in-flight cancel `Spell.cpp:4375`. One
  rule covers every profession, enchanting included. `AuraInterruptFlags` are untouched.
  Removing the script does not restore the bits; a server restart does.
- **Client (live code):** `spell::movement_interrupts` (`spell/inflight.rs`) applies the same
  rule at both client gates, the in-flight move cancel and the start refusal
  (`spell/validator.rs`). It sits in function bodies, so `-LiveCode` patches it in. The decode in
  `benilla-formats` would only change at load, so it stays as it is.
- Deploy the Lua before or with the client patch: a patched client against an unpatched server
  starts casts the server then refuses.
- **Check:** start a Linen Bandage or Brown Linen Shirt and run. The bar should keep filling and
  the item should arrive.

### 7. Disenchanting a random-enchanted item keeps its enchant (feature, ALE: no rebuild)

Owner request 2026-11-20: disenchanting a good roll should not waste it.

- **Design:** disenchanting an item that carries a random-enchant roll (slots PERM 0, TEMP 1, BONUS
  5, the slots `mod-random-enchants` writes) also gives one **Bottled Enchantment** per rolled
  enchant. The normal disenchant loot is unchanged. The bottle holds the enchant id in its own
  `PERM_ENCHANTMENT_SLOT`, so every tooltip shows the enchant's name in green with no client work.
  Using it on a weapon or armor piece moves that enchant onto the item. It goes into the target's
  PERM slot, or into BONUS when PERM is taken. The same requirements apply, so a level-60 enchant
  still needs level 60.
- **Pieces:**
  1. A world-DB `item_template` row: a custom entry in the custom range (e.g. 910001), class 0,
     subclass 8, quality 2, stack 1, an existing scroll display id, and an on-use spell with an
     item target. Reuse a stock item-targeting dummy, or add a `spell_dbc` row. Pick the spell
     when the row is written.
  2. ALE Lua `integrations/ale/rustwrath_bottled_enchants.lua`:
     - `PLAYER_EVENT_ON_SPELL_CAST` filtered to Disenchant (13262). Read the target item's enchant
       ids in slots 0/1/5. For each that is nonzero, `AddItem` a bottle and
       `SetEnchantment(id, 0)` on it.
     - `RegisterItemEvent(bottle, ITEM_EVENT_ON_USE)`: check that the target item is weapon or
       armor and belongs to the player, write the enchant, then consume the bottle.
     - Hot-reloads like item 2, with no rebuild. A new `item_template` row is **not live**: AC
       loads item templates once at startup and has no `.reload item_template`. To stay live,
       the bottle must reuse an existing item entry (an unused stock scroll/trinket with an
       item-targeting on-use spell), chosen when the owner answers the questions below.
- **Open questions for the owner:** whether a bottle binds (BoP or BoE), whether it can override
  an existing PERM enchant, and whether temporary-slot rolls count.
- **Superseded 2026-11-21:** the owner dropped bottle-per-enchant for one chargeable Enchant Bottle
  (`.scratch/project-b/prompts/brainstorm-enchant-bottle-2026-11-20.md`). The item entry can now come
  from the seeded dummy pools (`integrations/sql/DUMMY_POOLS.md`, usable items 800000-804999), which load at
  the next server start.
- **Check:** disenchant a random-enchanted green. The normal dust/essence should drop, plus one
  bottle per rolled enchant, and using a bottle on a fresh item should show the enchant there.

### 8. Souls hits count as the auto-attack (rage and Heroic Strike queue live; procs not live)

- **Symptom:** a Warrior gains no rage from Souls hits (owner 2026-11-20). More broadly, nothing
  that keys on the auto-attack fires on them: weapon procs (Crusader, Fiery, Windfury), seals,
  Flurry, Deep Wounds, Unbridled Wrath, a queued Heroic Strike or Cleave, weapon skill-ups.
- **Cause:** `rustwrath_souls.lua` lands each hit with ALE `Unit:DealDamage`
  (`mod-ale/.../UnitMethods.h:2498`), which calls `Unit::DealDamage` with no `CleanDamage` and
  no `ProcSkillsAndAuras`. AC grants melee rage only through the `CleanDamage` of
  `DealMeleeDamage` (`Unit.cpp:1316`), and procs only through `AttackerStateUpdate`'s tail.
- **Live now (ALE, deployed 2026-11-20, no rebuild):** `reward_rage` in the script grants
  AC's `RewardRage` formula once per swing (backstab and riposte as criticals), with
  `Rate.Rage.Income` 2 and Endless Rage. That fixes rage only.
- **Live 2026-11-20 (ALE):** `queued_strike_replaces_swing` in `on_hit`. If the player has a
  queued melee spell (`GetCurrentSpell(CURRENT_MELEE_SPELL)`: Heroic Strike, Cleave, Maul,
  Raptor Strike), the hit casts it (`Spell:Cast(false)`, the same CheckCast and failure path as
  `AttackerStateUpdate`) and the plain hit is skipped, so the strike replaces the swing. Short of
  power, it falls back to the plain hit, as `SPELL_FAILED_NO_POWER` does in core. Backstab and
  riposte are untouched.
- **Still not live:** weapon and talent procs (Crusader, Fiery, Windfury, seals, Flurry, Deep
  Wounds, Unbridled Wrath) and weapon skill-ups. They come only from `ProcSkillsAndAuras` with a
  melee `DamageInfo`, and no ALE method reaches it. The C++ method below is the only route found;
  it is written down, not scheduled, under the live-only rule.
- **C++ route (not live):** a new ALE method `Unit:SoulsMeleeHit(target, damage[, critical])`, added to
  `UnitMethods.h` and registered beside `DealDamage` in `LuaFunctions.cpp`. It is one
  main-hand auto-attack swing whose outcome and damage the caller has already decided:
  - **Queued melee spell:** if `GetCurrentSpell(CURRENT_MELEE_SPELL)` passes
    `CheckCast(false)`, call `cast(true)` and return; the queued strike replaces the swing.
    `SPELL_FAILED_NO_POWER` falls through to a plain swing. This is `AttackerStateUpdate`,
    `Unit.cpp:2975-2999`.
  - **Swing info:** fill a `CalcDamageInfo` the way `CalculateMeleeDamage` does
    (`Unit.cpp:1858`): attacker and target; `damages[0]` = `damage`, school
    `GetMeleeDamageSchoolMask(BASE_ATTACK, 0)`; the rest 0; `attackType` BASE_ATTACK;
    `procAttacker` `DONE_MELEE_AUTO_ATTACK | DONE_MAINHAND_ATTACK`; `procVictim`
    `TAKEN_MELEE_AUTO_ATTACK | TAKEN_DAMAGE`; `hitOutCome` NORMAL or CRIT; `TargetState`
    VICTIMSTATE_HIT; `HitInfo` AFFECTS_VICTIM, plus CRITICALHIT. Skip the stock hit table,
    armour and crit doubling: the client decided the hit, and the script already applied the
    Elden Ring defense curve.
  - **Absorb:** take absorb and resist via `DamageInfo(info, 0)` + `Unit::CalcAbsorbResist`,
    as `Unit.cpp:2150-2168` does.
  - **The swing:** then run `AttackerStateUpdate`'s tail (`Unit.cpp:3001-3024`), in order:
    `AtTargetAttacked(victim, true)`, `RemoveAurasWithInterruptFlags(MELEE_ATTACK)`,
    `DealDamageMods`, `SendAttackStateUpdate(&info)`, `DealMeleeDamage(&info, true)` (the rage,
    durability and block/parry rage) and `ProcSkillsAndAuras(... &dmgInfo)` (the procs and
    weapon skill).
  If it were ever built, the Lua would swap `DealDamage` for it on the nearest unit and drop
  `reward_rage` and `queued_strike_replaces_swing`, or rage and the strike would happen twice.
- **Check now (live):** a Warrior at 0 rage gains rage per Souls hit. A queued Heroic Strike
  fires on the next Souls hit and spends its rage; with too little rage the hit lands plainly.

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
