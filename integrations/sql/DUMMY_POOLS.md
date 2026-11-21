# Dummy pools (world DB)

Pre-seeded rows for content we have not designed yet. Seeded 2026-11-21 into the BridgeLab world
DB by [`world_dummy_pools.py`](world_dummy_pools.py), and recorded in the WM ledger
`wm_reserved_slot` as `SlotStatus = 'reserved'`, `ArcKey = 'rustwrath_dummy_pool'`, so the WM
allocators leave them alone. Each ledger row's `NotesJSON` names its shape.

| Pool | Ids | Count |
|---|---|---:|
| Items | 800000-809999 | 10,000 |
| Creatures | 820000-824999 | 5,000 |
| Game objects | 830000-834999 | 5,000 |
| Spell shells | 850000-857499 | 7,500 |

The whole 800000-859999 band is RustWrath's. The gaps (810000-819999, 825000-829999,
835000-849999, 857500-859999) are spare room for more pools or more spell shapes.

## Why they exist

The running worldserver loads some tables once, at startup, and has no reload for them:
`item_template`, `gameobject_template`, `creature_template_model` and the `*_dbc` override tables
(`spell_dbc`, `spellitemenchantment_dbc`, ...). `.reload creature_template <entry>` exists, but only
for entries the server already holds. A new row in any of these needs a server start.

Under the hotpatch-only rule, that start is never forced. So the rows are seeded ahead of time,
in numbers meant to last a long run. They exist in memory from the next server start (a reboot, a
crash, whatever comes). After that, we give them behaviour live, with ALE Lua by entry or spell
id, DB rows the server re-reads, and `.reload` commands.

**Status:** the rows are in the DB. The running server does **not** have them yet. They load at
the next start. A native module loaded live (the ALE `require` DLL route) could load them without
a start, but nothing like that exists yet.

## The pools

Each pool says what stays fixed after load and what can be changed live.

### Items: `item_template` 800000-809999

| Entries | Count | Shape | Name now |
|---|---:|---|---|
| 800000-804999 | 5,000 | Usable consumable, soulbound, stack 1, on use casts spell 18282 (stock "Dummy Spell", unscripted, does nothing) | `Reserved Usable <entry>` |
| 805000-807499 | 2,500 | Token, trade goods "Other", soulbound, stack 1000, no spell | `Reserved Token <entry>` |
| 807500-809999 | 2,500 | Same token shape, not bound (tradeable) | `Reserved Token <entry>` |

- **Live:**
  - Behaviour via `RegisterItemEvent(entry, 2, fn)` (on use; return `false` to stop the cast)
    and `RegisterItemGossipEvent(entry, 1/2, fn)` (a menu on use, works on every client).
  - Per-instance state: enchant slots, count, charges, binding, through the `Item` methods.
- **Fixed after load:**
  - English name, icon, class, quality, binding type, stack size, stats and use spell.
  - `item_template_locale` reloads, but the core skips enUS.
- **Client:** items need no client patch, because the server sends the item data. Stock clients
  cache item names on disk, so a renamed entry shows its old name until the cache is cleared.
- **Auction house bot:** mod-ahbot never lists these. It only lists ids up to `ListedItemIDMax`
  (default 200000), and it also skips 900000-999999 and soulbound items.

### Creatures: `creature_template` 820000-824999

- **Shape:** a friendly (faction 35) level-1 humanoid with the gossip flag, no AI, no loot, and
  the Northshire Guard model 3167. Named `Reserved Creature <entry>`.
- **Live:**
  - Everything in the `creature_template` row: edit the row, then `.reload creature_template
    820000`. That covers name, subname, level, faction, npcflag, unit flags, AIName/SmartAI,
    gossip menu, loot ids, gold, scale and speeds.
  - `smart_scripts`, loot and gossip tables reload too. ALE `RegisterCreatureEvent` /
    `RegisterCreatureGossipEvent` by entry.
  - The model on spawn via Lua `creature:SetDisplayId`. Spawning via `PerformIngameSpawn` or
    `.npc add`.
- **Fixed after load:** the `creature_template_model` row (the default model).
- **Client:** stock clients cache creature names, as for items.

### Game objects: `gameobject_template` 830000-834999

| Entries | Count | Shape | Name now |
|---|---:|---|---|
| 830000-832499 | 2,500 | Goober (type 10), Supply Crate model 335, no lock, no spell, no gossip menu | `Reserved Object <entry>` |
| 832500-834999 | 2,500 | Chest (type 3), Battered Chest model 259, no lock, loot id = its own entry, consumable | `Reserved Chest <entry>` |

- **Live:**
  - Goobers: `RegisterGameObjectGossipEvent` / `RegisterGameObjectEvent` by entry.
  - Chests: `gameobject_loot_template` rows for the entry, then `.reload gameobject_loot_template`.
- **Fixed after load:** type, model, size, name and the `DataN` fields.

### Spell shells: `spell_dbc` 850000-857499

All are instant, cost nothing, have no cooldown and no class family, and are cloned from stock
spell 18282. Every amount is 0: pass the real values with
`caster:CastCustomSpell(target, id, true, bp0, bp1, bp2)`.

Each shape owns one block of 500 ids: shape *k* (from 0) is `850000 + 500k` to `850499 + 500k`.
Names are `Reserved <shape> 001` to `Reserved <shape> 500`. The band has room for five more
shapes (857500-859999).

| Ids | Shape | Duration / range |
|---|---|---|
| 850000-850499 | Self Dummy: dummy effect on self | instant, self |
| 850500-850999 | Enemy Dummy: dummy effect on an enemy target | 40 yd |
| 851000-851499 | Ally Dummy: dummy effect on a friendly target | 40 yd |
| 851500-851999 | Self All Stats (`MOD_STAT`, misc -1) | until removed |
| 852000-852499 | Self Attack Power: melee (bp0) and ranged (bp1) | until removed |
| 852500-852999 | Self Damage Done Pct (all schools) | until removed |
| 853000-853499 | Self Move Speed Pct | until removed |
| 853500-853999 | Self Max Health | until removed |
| 854000-854499 | Self Armor | until removed |
| 854500-854999 | Enemy Damage Over Time, tick every 3 s, shadow | 15 s, 40 yd |
| 855000-855499 | Enemy Slow Pct | 10 s, 40 yd |
| 855500-855999 | Enemy Stun (stun mechanic) | 5 s, 40 yd |
| 856000-856499 | Ally Heal Over Time, tick every 3 s, holy | 15 s, 40 yd |
| 856500-856999 | Enemy Direct Damage, fire | 40 yd |
| 857000-857499 | Ally Direct Heal, holy | 40 yd |

- **Live:**
  - What a dummy does: `RegisterSpellEvent(id, 2, fn)` (on cast) or player event 5.
  - Amounts, via `CastCustomSpell`.
  - Through the `SpellEntry` setters (from `GetSpellInfo(id)`): school (`SetSchoolMask`), damage
    class, cooldowns, costs and power type, proc flags/chance/charges, attributes, interrupt
    flags, levels and max targets.
  - Removing an "until removed" aura: `unit:RemoveAura(id)`.
- **Fixed after load:** effect types, aura types, targets, duration, range, radius and tick
  period. ALE has no setters for these.
- **Client:** these ids are not in the client's Spell.dbc, so neither stock clients nor RustWrath
  know their name, icon or cast bar. Server effects still apply. To make one visible, add a client
  patch row, the way wm-project's `client_patches/wm_spell_shell_bank` builds `patch-z.mpq`.

## Not seeded

- **Enchants** (`spellitemenchantment_dbc`). An enchant's stats are fixed when the server loads it,
  and no Lua method changes them, so empty enchant rows are no use. Seed real enchants when they
  are designed, for example the tier-6+ enchants for epic items in `item_enchantment_random_tiers`.
  - WM's range is 100500-100599. 100520, 100525 and 100530 are taken.
  - The client needs a matching SpellItemEnchantment.dbc patch row to show the tooltip text.
- **Quests, gossip menus, npc text, loot, conditions and SmartAI.** These reload whole
  (`.reload quest_template`, `.reload gossip_menu`, ...), so new rows go live without pools.

## Other custom ids already in use

- **800000-859999:** these pools (above).
- The server Spell.dbc already holds about 26,000 custom spells, 940000-997999, but none are
  free:
  - Spellforge and Spellwright (mod-multiclass) own 947xxx and 948000-997999.
  - WM shells own 940000-946999.
  - WM item and spell slots own the rest.
- WM's own free ledger pools are items 911000-911999, spells 900000-900999, gossip menus
  912000-912499 and npc text 912500-912999. Leave them to WM's allocators. The first seeding
  (2026-11-21) briefly took 911000-911099 and 900001-900090; the current script returns those
  slots to `free` and deletes the old 920200-920249 creatures and 913000-913049 objects.
- WM's reserved ranges are in wm-project's `data/specs/reserved_id_ranges.json` and
  `custom_id_registry.json`.

## Re-seeding

```powershell
python integrations\sql\world_dummy_pools.py "<DataDir>\dbc\Spell.dbc" > dummy_pools.sql
```

Run the output with the mysql client against the world DB (about 27,500 rows; it needs MySQL 8
for the recursive id sequence). It deletes and re-inserts only these ranges and their ledger
rows, inside one transaction. Before re-running, check that no pool row has been taken into use.
Once a row is used, give it its own ledger status (`active`).
