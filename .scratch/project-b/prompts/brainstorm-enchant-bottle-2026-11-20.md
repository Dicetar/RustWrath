# Brainstorm handout: the Enchant Bottle (2026-11-20)

**Built 2026-11-21:** the approved v1 (Reroll, Gamble, Empower, Awaken, Imprint/Pour, plus Absorb
for white items) is `integrations/ale/rustwrath_enchant_bottle.lua`; its header is the spec.
Deferred: Lock, Overcharge buff, Corrupted gamble.

A brainstorm brief, not a build order. Goal: agree on one design the owner likes, that fits
what the running server can do **live**, then build it.

## The idea so far (owner, 2026-11-20)

- Disenchanting a random-enchanted item should not just waste a good roll.
- **Not** one item per enchant ("too busted, too much work").
- Instead: **one chargeable consumable, like the Bottle in DotA.** Disenchanting fills it with
  charges, more for higher enchant tiers. Gather enough charges and spend them to do something
  cool with your gear.
- **Decided:** rolls in the temporary slot count too (all three random-enchant slots feed charges).
- Open: everything else — what charges buy, how many, caps, binding, what makes it fun.

## The game and the hard rule

- World of Warcraft 3.3.5a (build 12340) on a private **AzerothCore** server (mod-playerbots
  fork) with **mod-random-enchants** and **ALE** (Eluna, Lua 5.2). Owner's character: Firna,
  low level, Northshire.
- Client: **RustWrath**, the owner's own Rust/Bevy client for 12340, live-patched while running
  (`-LiveCode`). It can show custom UI. Stock 12340 clients also connect and only see stock UI
  (tooltips, gossip menus, chat).
- **Owner rule: hotpatch only.** No worldserver rebuild, no restart, ever. Everything must reach
  the running server through:
  - ALE Lua in `D:\WOW\WM_BridgeLab\run\lua_scripts` (auto-reloads on save; `.reload ALE`);
  - module config lines + `.reload config`;
  - world/character DB rows the server reads at use time (owner approves DB writes);
  - possibly a native DLL loaded live by ALE `require` (proposed 2026-11-20, not approved yet),
    which can call server C++ functions directly.
- **Not live:** new `item_template` rows (items are loaded once at startup; no reload). So the
  bottle must **reuse an existing item entry**, or not be an item at all. New `Spell.dbc` /
  `spell_dbc` rows are not live either; reuse existing spells.
- **Dummy pools (seeded 2026-11-21):** 5,000 usable items 800000-804999, 5,000 tokens
  805000-809999, 7,500 spell shells 850000-857499 (15 shapes × 500), 5,000 creatures
  820000-824999 and 5,000 objects 830000-834999. They load at the next server start, then take
  behaviour from Lua live. See `integrations/sql/DUMMY_POOLS.md`.

## How random enchants work here

- Rolled when an item is looted, crafted, given as a quest reward or won in a group roll, on
  weapons and armor of quality white (1) to legendary (5).
- Up to three enchants, in item enchant slots PERM (0), TEMP (1), BONUS (5). Chain: 70 % for the
  first; if it lands, 65 % for the second; then 60 % for the third.
- **Tier** from item quality (module's roll on a 0-93 scale): white mostly tier 1 (10 % tier 2);
  green tier 2; blue tier 2-3; epic tier 4 (some 3/5); legendary tier 5.
- Pool: world table `item_enchantment_random_tiers` (`enchantID`, `class` ANY/ARMOR/WEAPON,
  `exclusiveSubClass`, `tier`), 700 rows:

  | Tier | ANY | ARMOR | WEAPON | Example |
  |---|---:|---:|---:|---|
  | 1 | 38 | 0 | 14 | +6..+12 Attack Power |
  | 2 | 112 | 28 | 20 | +14..+22 Attack Power |
  | 3 | 135 | 26 | 27 | +24..+30 Attack Power |
  | 4 | 108 | 58 | 24 | +32..+44 Attack Power |
  | 5 | 88 | 1 | 20 | +48..+56 Attack Power |

- Only **9 of 700** enchants carry a character-level requirement (4 at level 60 in tier 3, two
  level-50 and two level-60 in tier 4, one level-50 in tier 5). Below that level the server
  shows them but applies nothing.
- **Status:** the module's own roller ignores the tier (query bug), so any item can roll any
  tier. `integrations/ale/rustwrath_random_enchants.lua` (main, ce62be8f3) is a fixed Lua roller,
  deployed but idle until the owner switches the C++ roller off. The owner is still deciding how
  strict tiers should be (whites rolling big enchants was fun).
- Enchant data lives in the server's `SpellItemEnchantment.dbc` (id, effect types/amounts,
  name, required level); Lua already reads it.

## Building blocks available live (ALE)

- **Events:** `RegisterPlayerEvent` (loot 32, create 52, quest reward 51, group roll 56, spell
  cast 5 — e.g. Disenchant 13262), `RegisterItemEvent` (on use 2, can cancel the cast),
  `RegisterItemGossipEvent` (an item can open a **gossip menu** on use — a menu UI that works on
  every client), `RegisterSpellEvent`, timed events.
- **Items:** read/write enchant slots (`GetEnchantmentId`, `SetEnchantment`, `ClearEnchantment`),
  quality, class/subclass, item level, required level, random property/suffix (`SetRandomProperty`,
  `SetRandomSuffix`), binding (`SetBinding`), count (`SetCount`), item link, save.
- **Player:** add/remove items, cast spells (triggered), auras, chat/broadcast/area messages,
  gossip menus with text input (`code`), money.
- **Storage for charges:** options to judge — the item's own update fields (e.g. spell charges,
  saved to `item_instance.charges`; tooltip display to verify), the stack count, or a small
  character-DB table keyed by player (needs owner OK to create).
- **RustWrath client:** custom UI is possible (e.g. a bottle charge meter, a gear-crafting
  panel), talking to the server Lua over the addon-whisper channel the Souls combat code already
  uses (`RWSOULS`).
- **Needs the native DLL (if approved):** anything the server C++ decides that Lua cannot reach,
  e.g. letting an enchant work below its required level, custom stat formulas, proc behavior.

## Design questions to brainstorm

1. **Charge economy:** charges per enchant by tier (1/2/4/8/16?), per slot, cap per bottle
   (DotA: 3), one bottle per character or many, can charges be traded or the bottle sold?
2. **How you get the bottle:** drop, quest, trainer/vendor (reuse an existing NPC), first
   disenchant grants it?
3. **What charges buy** (the fun part). Seeds, not decisions:
   - **Reroll** one enchant slot on a worn item (same tier, or gamble one tier up/down).
   - **Upgrade** an enchant to the same stat's next tier (+22 AP → +24 AP).
   - **Lock** a slot so later rerolls keep it; **open** an empty slot (add a 2nd/3rd roll).
   - **Imprint/transfer:** store the last disenchanted enchant in the bottle, pour it onto
     another item (one at a time, consumes charges — the old item-per-enchant idea, made scarce).
   - **Attune:** pick the stat family (AP, stamina, crit…) and roll only within it.
   - **Gamble:** big charge cost, chance of a tier-5 roll or a "corrupted" downside roll.
   - **Temporary overcharge:** spend charges for a short buff (DotA bottle's heal echo).
4. **Limits so it isn't busted:** costs scaling with tier, cooldown, one action per item,
   soulbinding upgraded items, charges decaying, only on items you equip.
5. **Feedback:** chat lines, gossip menu text, RustWrath UI panel, visual (item glow via enchant
   visuals already in the DBC).
6. **Playerbots:** do bots disenchant/use bottles, or player-only?

## Constraints for whoever builds it

- Repo: `Dicetar/RustWrath` (public — no secrets, no owner identity). Server scripts go in
  `integrations/ale/`, deployed by copying into the server's `run\lua_scripts`.
- Server source, DB and game install are read-only unless the owner says otherwise. DB writes
  and config changes are the owner's call.
- No Cargo builds or client launches unless the owner asks; the owner tests in game.
