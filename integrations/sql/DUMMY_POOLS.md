# Dummy pools (world DB)

Pre-seeded rows for content we have not designed yet. Seeded 2026-11-21 into the BridgeLab world
DB by [`world_dummy_pools.py`](world_dummy_pools.py), and recorded in the WM ledger
`wm_reserved_slot` as `SlotStatus = 'reserved'`, `ArcKey = 'rustwrath_dummy_pool'`, so the WM
allocators leave them alone.

## Why they exist

The running worldserver loads some tables once, at startup, and has no reload for them:
`item_template`, `gameobject_template`, `creature_template_model` and the `*_dbc` override tables
(`spell_dbc`, `spellitemenchantment_dbc`, ...). `.reload creature_template <entry>` exists, but only
for entries the server already holds. A new row in any of these needs a server start.

Under the hotpatch-only rule, that start is never forced. So the rows are seeded ahead of time.
They exist in memory from the next server start (a reboot, a crash, whatever comes). After that,
we give them behaviour live, with ALE Lua by entry or spell id, DB rows the server re-reads, and
`.reload` commands.

**Status:** the rows are in the DB. The running server does **not** have them yet. They load at
the next start. A native module loaded live (the ALE `require` DLL route) could load them without
a start, but nothing like that exists yet.

## The pools

Each pool says what stays fixed after load and what can be changed live.

### Items: `item_template` 911000-911099

| Entries | Shape | Name now |
|---|---|---|
| 911000-911049 | Usable consumable, soulbound, stack 1, on use casts spell 18282 (stock "Dummy Spell", unscripted, does nothing) | `Reserved Usable <entry>` |
| 911050-911074 | Token, trade goods "Other", soulbound, stack 1000, no spell | `Reserved Token <entry>` |
| 911075-911099 | Same token shape, not bound (tradeable) | `Reserved Token <entry>` |

- **Live:**
  - Behaviour via `RegisterItemEvent(entry, 2, fn)` (on use; return `false` to stop the cast)
    and `RegisterItemGossipEvent(entry, 1/2, fn)` (a menu on use, works on every client).
  - Per-instance state: enchant slots, count, charges, binding, through the `Item` methods.
- **Fixed after load:**
  - English name, icon, class, quality, binding type, stack size, stats and use spell.
  - `item_template_locale` reloads, but the core skips enUS.
- **Client:** items need no client patch, because the server sends the item data. Stock clients
  cache item names on disk, so a renamed entry shows its old name until the cache is cleared.

### Creatures: `creature_template` 920200-920249

- **Shape:** a friendly (faction 35) level-1 humanoid with the gossip flag, no AI, no loot, and
  the Northshire Guard model 3167.
- **Live:**
  - Everything in the `creature_template` row: edit the row, then `.reload creature_template
    920200`. That covers name, subname, level, faction, npcflag, unit flags, AIName/SmartAI,
    gossip menu, loot ids, gold, scale and speeds.
  - `smart_scripts`, loot and gossip tables reload too. ALE `RegisterCreatureEvent` /
    `RegisterCreatureGossipEvent` by entry.
  - The model on spawn via Lua `creature:SetDisplayId`. Spawning via `PerformIngameSpawn` or
    `.npc add`.
- **Fixed after load:** the `creature_template_model` row (the default model).
- **Client:** stock clients cache creature names, as for items.

### Game objects: `gameobject_template` 913000-913049

| Entries | Shape |
|---|---|
| 913000-913029 | Goober (type 10), Supply Crate model 335, no lock, no spell, no gossip menu |
| 913030-913049 | Chest (type 3), Battered Chest model 259, no lock, loot id = its own entry, consumable |

- **Live:**
  - Goobers: `RegisterGameObjectGossipEvent` / `RegisterGameObjectEvent` by entry.
  - Chests: `gameobject_loot_template` rows for the entry, then `.reload gameobject_loot_template`.
- **Fixed after load:** type, model, size, name and the `DataN` fields.

### Spell shells: `spell_dbc` 900001-900090

All are instant, cost nothing, have no cooldown and no class family, and are cloned from stock
spell 18282. Every amount is 0: pass the real values with
`caster:CastCustomSpell(target, id, true, bp0, bp1, bp2)`.

| Ids | Shape | Duration / range |
|---|---|---|
| 900001-900010 | Dummy effect on self | instant, self |
| 900011-900020 | Dummy effect on an enemy target | 40 yd |
| 900021-900030 | Dummy effect on a friendly target | 40 yd |
| 900031-900035 | Self aura: all stats (`MOD_STAT`, misc -1) | until removed |
| 900036-900040 | Self aura: melee attack power (bp0) and ranged attack power (bp1) | until removed |
| 900041-900045 | Self aura: damage done % (all schools) | until removed |
| 900046-900050 | Self aura: movement speed % | until removed |
| 900051-900055 | Self aura: max health | until removed |
| 900056-900060 | Self aura: armor | until removed |
| 900061-900065 | Enemy damage over time, tick every 3 s, shadow | 15 s, 40 yd |
| 900066-900070 | Enemy slow % | 10 s, 40 yd |
| 900071-900075 | Enemy stun (stun mechanic) | 5 s, 40 yd |
| 900076-900080 | Friendly heal over time, tick every 3 s, holy | 15 s, 40 yd |
| 900081-900085 | Enemy direct damage, fire | 40 yd |
| 900086-900090 | Friendly direct heal, holy | 40 yd |

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

- The server Spell.dbc already holds about 26,000 custom spells, 940000-997999, but none are
  free:
  - Spellforge and Spellwright (mod-multiclass) own 947xxx and 948000-997999.
  - WM shells own 940000-946999.
  - WM item and spell slots own the rest.
- WM's reserved ranges are in wm-project's `data/specs/reserved_id_ranges.json` and
  `custom_id_registry.json`.

## Re-seeding

```powershell
python integrations\sql\world_dummy_pools.py "<DataDir>\dbc\Spell.dbc" > dummy_pools.sql
```

Run the output with the mysql client against the world DB. It deletes and re-inserts only these
ranges and their ledger rows, inside one transaction. Before re-running, check that no pool row
has been taken into use. Once a row is used, give it its own ledger status (`active`).
