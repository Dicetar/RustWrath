"""Emit the SQL that seeds RustWrath's dummy pools in the world DB.

Usage:  python world_dummy_pools.py <server DataDir>\\dbc\\Spell.dbc > dummy_pools.sql
then run the file with the mysql client against the world DB. Re-running replaces the pools.

Why pools: the running worldserver never reloads `item_template`, `gameobject_template`,
`creature_template_model` or the `*_dbc` override tables, and `.reload creature_template` only
refreshes entries it already holds. Rows seeded here load at the next server start; from then on
their behaviour is given live by ALE Lua (by entry or spell id), DB rows the server re-reads,
and `.reload` commands. What stays fixed per row after load is listed in DUMMY_POOLS.md.

Every pool is cloned set-based in SQL from one source row. Items, creatures and objects clone
stock rows. Spell shells clone one row per shape built here from stock spell 18282 ("Dummy
Spell": instant, self, no cost, no script), because `spell_dbc` has no stock rows to clone from.
Its 234 columns follow Spell.dbc's fields.
"""

import struct
import sys

BASE_SPELL = 18282
FLOAT_FIELDS = {47, 77, 78, 79, 101, 102, 103, 119, 120, 121, 216, 217, 218, 229, 230, 231}
STRING_FIELDS = set(range(136, 152)) | set(range(153, 169)) | set(range(170, 186)) | set(range(187, 203))

# Spell.dbc / spell_dbc field indices used below.
ATTRIBUTES = range(4, 12)
MECHANIC, CASTING_TIME, DURATION, RANGE = 3, 28, 40, 46
EFFECT, DIE_SIDES, BASE_POINTS, EFFECT_MECHANIC = 71, 74, 80, 83
TARGET_A, TARGET_B, AURA, AURA_PERIOD, MISC_VALUE = 86, 89, 95, 98, 110
NAME, SUBTEXT, DESCRIPTION, DMG_CLASS, SCHOOL = 136, 153, 170, 213, 225

# SpellDuration ids: 21 until cancelled, 28 5 s, 1 10 s, 8 15 s. SpellRange ids: 1 self, 5 40 yd.
SELF, ENEMY, ALLY = 1, 6, 21
DUMMY, SCHOOL_DAMAGE, APPLY_AURA, HEAL = 3, 2, 6, 10

# Spell shells: shape k holds ids SPELL_FIRST + k * SPELL_BLOCK .. + SPELL_BLOCK - 1. Append new
# shapes at the end so existing ids keep their shape; the band has room for 20 shapes (859999).
SPELL_FIRST, SPELL_BLOCK = 850000, 500
# (name, effects [(effect, target, aura, period, misc)], duration, range, extra)
SPELL_SHAPES = [
    ("Self Dummy", [(DUMMY, SELF, 0, 0, 0)], 0, 1, {}),
    ("Enemy Dummy", [(DUMMY, ENEMY, 0, 0, 0)], 0, 5, {}),
    ("Ally Dummy", [(DUMMY, ALLY, 0, 0, 0)], 0, 5, {}),
    ("Self All Stats", [(APPLY_AURA, SELF, 29, 0, -1)], 21, 1, {}),
    ("Self Attack Power", [(APPLY_AURA, SELF, 99, 0, 0), (APPLY_AURA, SELF, 124, 0, 0)], 21, 1, {}),
    ("Self Damage Done Pct", [(APPLY_AURA, SELF, 79, 0, 127)], 21, 1, {}),
    ("Self Move Speed Pct", [(APPLY_AURA, SELF, 31, 0, 0)], 21, 1, {}),
    ("Self Max Health", [(APPLY_AURA, SELF, 34, 0, 0)], 21, 1, {}),
    ("Self Armor", [(APPLY_AURA, SELF, 22, 0, 1)], 21, 1, {}),
    ("Enemy Damage Over Time", [(APPLY_AURA, ENEMY, 3, 3000, 0)], 8, 5, {DMG_CLASS: 1, SCHOOL: 32}),
    ("Enemy Slow Pct", [(APPLY_AURA, ENEMY, 33, 0, 0)], 1, 5, {}),
    ("Enemy Stun", [(APPLY_AURA, ENEMY, 12, 0, 0)], 28, 5, {MECHANIC: 12, EFFECT_MECHANIC: 12}),
    ("Ally Heal Over Time", [(APPLY_AURA, ALLY, 8, 3000, 0)], 8, 5, {DMG_CLASS: 1, SCHOOL: 2}),
    ("Enemy Direct Damage", [(SCHOOL_DAMAGE, ENEMY, 0, 0, 0)], 0, 5, {DMG_CLASS: 1, SCHOOL: 4}),
    ("Ally Direct Heal", [(HEAL, ALLY, 0, 0, 0)], 0, 5, {DMG_CLASS: 1, SCHOOL: 2}),
]

# Pool bands (first, last). No WM range, ledger row or server Spell.dbc row uses 800000-859999.
ITEM_USABLE = (800000, 804999)
ITEM_TOKEN_BOUND = (805000, 807499)
ITEM_TOKEN_FREE = (807500, 809999)
CREATURES = (820000, 824999)
GOOBERS = (830000, 832499)
CHESTS = (832500, 834999)
SPELLS = (SPELL_FIRST, SPELL_FIRST + len(SPELL_SHAPES) * SPELL_BLOCK - 1)

# The first seeding (2026-11-21) used slots from WM's own free ledger pools (items 911000-911099,
# spells 900001-900090) and two small bands. Re-running clears them and gives WM its slots back.
# (table, key, ledger entity or None, first, last, ledger action)
RETIRED_POOLS = [
    ("item_template", "entry", "item_template", 911000, 911099, "free"),
    ("spell_dbc", "ID", "spell_dbc_or_spell_slots", 900001, 900090, "free"),
    ("creature_template_model", "CreatureID", None, 920200, 920249, None),
    ("creature_template", "entry", "creature_template", 920200, 920249, "delete"),
    ("gameobject_template", "entry", "gameobject_template", 913000, 913049, "delete"),
]


def read_base_spell(path):
    data = open(path, "rb").read()
    if data[:4] != b"WDBC":
        sys.exit(f"{path} is not a WDBC file")
    records, fields, size, _ = struct.unpack_from("<4I", data, 4)
    if fields != 234:
        sys.exit(f"{path}: {fields} fields, expected 234 (3.3.5a Spell.dbc)")
    for i in range(records):
        at = 20 + i * size
        if struct.unpack_from("<I", data, at)[0] == BASE_SPELL:
            raw = struct.unpack_from("<234I", data, at)
            values = []
            for index, value in enumerate(raw):
                if index in FLOAT_FIELDS:
                    values.append(struct.unpack("<f", struct.pack("<I", value))[0])
                elif index in STRING_FIELDS:
                    values.append("")
                else:
                    values.append(struct.unpack("<i", struct.pack("<I", value))[0])
            return values
    sys.exit(f"{path}: spell {BASE_SPELL} not found")


def sql_value(value):
    if isinstance(value, str):
        return "'" + value.replace("\\", "\\\\").replace("'", "''") + "'"
    if isinstance(value, float):
        return repr(value)
    return str(value)


def seq(first, last):
    """A derived table `s` of the integers first..last (cte_max_recursion_depth is raised)."""
    return (f"(WITH RECURSIVE c (n) AS (SELECT {first} UNION ALL SELECT n + 1 FROM c WHERE n < {last})"
            f" SELECT n FROM c) s")


def shape_rows(base):
    """One spell_dbc row per shape, at the first id of its block."""
    for k, (name, effects, duration, range_id, extra) in enumerate(SPELL_SHAPES):
        row = list(base)
        row[0] = SPELL_FIRST + k * SPELL_BLOCK
        for index in ATTRIBUTES:
            row[index] = 0
        row[CASTING_TIME], row[DURATION], row[RANGE] = 1, duration, range_id
        for slot, (effect, target, aura, period, misc) in enumerate(effects):
            row[EFFECT + slot] = effect
            row[DIE_SIDES + slot] = 0
            row[BASE_POINTS + slot] = 0
            row[TARGET_A + slot] = target
            row[TARGET_B + slot] = 0
            row[AURA + slot] = aura
            row[AURA_PERIOD + slot] = period
            row[MISC_VALUE + slot] = misc
        for index, value in extra.items():
            row[index] = value
        row[NAME] = name
        row[SUBTEXT] = "RustWrath shell"
        row[DESCRIPTION] = "Reserved spell shell; amounts come from CastCustomSpell."
        yield row


def clone(table, key, source, base, first, last, updates):
    """Copy row `base` of `source` into `table` as ids first..last.

    `updates` is a list of (column, SQL expression); `rw_n` in an expression is the new id. The
    insert names `table`'s columns, so the helper column `rw_n` stays behind in the temp table.
    """
    sets = ", ".join(f"`{column}` = {value}" for column, value in updates)
    return [
        "DROP TEMPORARY TABLE IF EXISTS rw_clone;",
        f"CREATE TEMPORARY TABLE rw_clone SELECT s.n AS rw_n, t.* FROM `{source}` t"
        f" JOIN {seq(first, last)} WHERE t.`{key}` = {base};",
        f"UPDATE rw_clone SET `{key}` = rw_n, {sets};",
        "SELECT GROUP_CONCAT(CONCAT('`', COLUMN_NAME, '`') ORDER BY ORDINAL_POSITION) INTO @rw_cols"
        f" FROM information_schema.COLUMNS WHERE TABLE_SCHEMA = DATABASE() AND TABLE_NAME = '{table}';",
        f"SET @rw_sql = CONCAT('INSERT INTO `{table}` (', @rw_cols, ') SELECT ', @rw_cols, ' FROM rw_clone');",
        "PREPARE rw_stmt FROM @rw_sql;",
        "EXECUTE rw_stmt;",
        "DEALLOCATE PREPARE rw_stmt;",
    ]


def item_updates(usable, bound):
    label = "Usable" if usable else "Token"
    return [
        ("name", f"CONCAT('Reserved {label} ', rw_n)"), ("description", "''"), ("ScriptName", "''"),
        ("class", 0 if usable else 7), ("subclass", 8 if usable else 11),
        ("displayid", 18077 if usable else 51756), ("Quality", 1), ("Flags", 0),
        ("FlagsExtra", 0), ("BuyPrice", 0), ("SellPrice", 0), ("InventoryType", 0),
        ("AllowableClass", -1), ("AllowableRace", -1), ("ItemLevel", 1), ("RequiredLevel", 0),
        ("maxcount", 0), ("stackable", 1 if usable else 1000), ("bonding", 1 if bound else 0),
        ("spellid_1", BASE_SPELL if usable else 0), ("spelltrigger_1", 0),
        ("spellcharges_1", 0), ("spellcooldown_1", -1), ("spellcategory_1", 0),
        ("spellcategorycooldown_1", -1), ("startquest", 0), ("duration", 0),
    ]


CREATURE_UPDATES = [
    ("name", "CONCAT('Reserved Creature ', rw_n)"), ("subname", "''"),
    ("minlevel", 1), ("maxlevel", 1), ("faction", 35), ("npcflag", 1), ("rank", 0),
    ("unit_flags", 0), ("AIName", "''"), ("ScriptName", "''"), ("lootid", 0),
    ("pickpocketloot", 0), ("skinloot", 0), ("gossip_menu_id", 0), ("mingold", 0),
    ("maxgold", 0),
]

GOOBER_UPDATES = [
    ("name", "CONCAT('Reserved Object ', rw_n)"), ("Data0", 0), ("Data1", 0),
    ("Data2", 0), ("Data10", 0), ("Data19", 0), ("AIName", "''"), ("ScriptName", "''"),
]

CHEST_UPDATES = [
    ("name", "CONCAT('Reserved Chest ', rw_n)"), ("Data0", 0), ("Data1", "rw_n"),
    ("Data2", 0), ("Data3", 1), ("AIName", "''"), ("ScriptName", "''"),
]


def ledger(entity, first, last, shape):
    notes = sql_value('{"owner":"rustwrath_dummy_pool","shape":"%s","doc":"integrations/sql/DUMMY_POOLS.md"}' % shape)
    return [
        f"DELETE FROM wm_reserved_slot WHERE EntityType = '{entity}' AND ReservedID BETWEEN {first} AND {last};",
        f"INSERT INTO wm_reserved_slot (EntityType, ReservedID, SlotStatus, ArcKey, NotesJSON) "
        f"SELECT '{entity}', s.n, 'reserved', 'rustwrath_dummy_pool', {notes} FROM {seq(first, last)};",
    ]


def pool(table, key, span, source, base, updates, entity, shape):
    first, last = span
    return clone(table, key, source, base, first, last, updates) + ledger(entity, first, last, shape)


def main():
    if len(sys.argv) != 2:
        sys.exit(__doc__)
    base = read_base_spell(sys.argv[1])
    out = [
        "-- Generated by integrations/sql/world_dummy_pools.py; see DUMMY_POOLS.md.",
        "SET SESSION cte_max_recursion_depth = 100000;",
        "SET SESSION group_concat_max_len = 1000000;",
        # The shape rows go in a temporary table before the transaction, so no DDL runs inside it.
        "DROP TEMPORARY TABLE IF EXISTS rw_shape;",
        "CREATE TEMPORARY TABLE rw_shape LIKE spell_dbc;",
    ]
    for row in shape_rows(base):
        out.append("INSERT INTO rw_shape VALUES (" + ", ".join(sql_value(v) for v in row) + ");")
    out.append("START TRANSACTION;")

    for table, key, entity, first, last, action in RETIRED_POOLS:
        out.append(f"DELETE FROM `{table}` WHERE `{key}` BETWEEN {first} AND {last};")
        where = (f"EntityType = '{entity}' AND ReservedID BETWEEN {first} AND {last}"
                 " AND ArcKey = 'rustwrath_dummy_pool'")
        if action == "free":
            out.append("UPDATE wm_reserved_slot SET SlotStatus = 'free', ArcKey = NULL, NotesJSON = NULL"
                       f" WHERE {where};")
        elif action == "delete":
            out.append(f"DELETE FROM wm_reserved_slot WHERE {where};")

    out.append(f"DELETE FROM item_template WHERE entry BETWEEN {ITEM_USABLE[0]} AND {ITEM_TOKEN_FREE[1]};")
    out += pool("item_template", "entry", ITEM_USABLE, "item_template", 3371,
                item_updates(True, True), "item_template", "usable")
    out += pool("item_template", "entry", ITEM_TOKEN_BOUND, "item_template", 3371,
                item_updates(False, True), "item_template", "token")
    out += pool("item_template", "entry", ITEM_TOKEN_FREE, "item_template", 3371,
                item_updates(False, False), "item_template", "token_tradeable")

    first, last = CREATURES
    out.append(f"DELETE FROM creature_template_model WHERE CreatureID BETWEEN {first} AND {last};")
    out.append(f"DELETE FROM creature_template WHERE entry BETWEEN {first} AND {last};")
    out += pool("creature_template", "entry", CREATURES, "creature_template", 1642,
                CREATURE_UPDATES, "creature_template", "creature")
    out.append("INSERT INTO creature_template_model (CreatureID, Idx, CreatureDisplayID, DisplayScale,"
               f" Probability) SELECT s.n, 0, 3167, 1, 1 FROM {seq(first, last)};")

    out.append(f"DELETE FROM gameobject_template WHERE entry BETWEEN {GOOBERS[0]} AND {CHESTS[1]};")
    out += pool("gameobject_template", "entry", GOOBERS, "gameobject_template", 166872,
                GOOBER_UPDATES, "gameobject_template", "goober")
    out += pool("gameobject_template", "entry", CHESTS, "gameobject_template", 2843,
                CHEST_UPDATES, "gameobject_template", "chest")

    out.append(f"DELETE FROM spell_dbc WHERE ID BETWEEN {SPELLS[0]} AND {SPELLS[1]};")
    for k, (name, *_) in enumerate(SPELL_SHAPES):
        first = SPELL_FIRST + k * SPELL_BLOCK
        updates = [("Name_Lang_enUS", f"CONCAT('Reserved {name} ', LPAD(rw_n - {first} + 1, 3, '0'))")]
        out += pool("spell_dbc", "ID", (first, first + SPELL_BLOCK - 1), "rw_shape", first,
                    updates, "spell_dbc_or_spell_slots", "spell_shell")

    out += ["DROP TEMPORARY TABLE IF EXISTS rw_clone;", "DROP TEMPORARY TABLE IF EXISTS rw_shape;", "COMMIT;"]
    print("\n".join(out))


if __name__ == "__main__":
    main()
