"""Emit the SQL that seeds RustWrath's dummy pools in the world DB.

Usage:  python world_dummy_pools.py <server DataDir>\\dbc\\Spell.dbc > dummy_pools.sql
then run the file with the mysql client against the world DB. Re-running replaces the pools.

Why pools: the running worldserver never reloads `item_template`, `gameobject_template`,
`creature_template_model` or the `*_dbc` override tables, and `.reload creature_template` only
refreshes entries it already holds. Rows seeded here load at the next server start; from then on
their behaviour is given live by ALE Lua (by entry or spell id), DB rows the server re-reads,
and `.reload` commands. What stays fixed per row after load is listed in DUMMY_POOLS.md.

Items, creatures and objects are cloned from stock rows in SQL. Spell shells are written out
in full from stock spell 18282 ("Dummy Spell": instant, self, no cost, no script), because the
`spell_dbc` table has no stock rows to clone from. Its 234 columns follow Spell.dbc's fields.
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

# (first id, count, name, effects [(effect, target, aura, period, misc)], duration, range, extra)
SPELL_SHELLS = [
    (900001, 10, "Self Dummy", [(DUMMY, SELF, 0, 0, 0)], 0, 1, {}),
    (900011, 10, "Enemy Dummy", [(DUMMY, ENEMY, 0, 0, 0)], 0, 5, {}),
    (900021, 10, "Ally Dummy", [(DUMMY, ALLY, 0, 0, 0)], 0, 5, {}),
    (900031, 5, "Self All Stats", [(APPLY_AURA, SELF, 29, 0, -1)], 21, 1, {}),
    (900036, 5, "Self Attack Power",
     [(APPLY_AURA, SELF, 99, 0, 0), (APPLY_AURA, SELF, 124, 0, 0)], 21, 1, {}),
    (900041, 5, "Self Damage Done Pct", [(APPLY_AURA, SELF, 79, 0, 127)], 21, 1, {}),
    (900046, 5, "Self Move Speed Pct", [(APPLY_AURA, SELF, 31, 0, 0)], 21, 1, {}),
    (900051, 5, "Self Max Health", [(APPLY_AURA, SELF, 34, 0, 0)], 21, 1, {}),
    (900056, 5, "Self Armor", [(APPLY_AURA, SELF, 22, 0, 1)], 21, 1, {}),
    (900061, 5, "Enemy Damage Over Time", [(APPLY_AURA, ENEMY, 3, 3000, 0)], 8, 5,
     {DMG_CLASS: 1, SCHOOL: 32}),
    (900066, 5, "Enemy Slow Pct", [(APPLY_AURA, ENEMY, 33, 0, 0)], 1, 5, {}),
    (900071, 5, "Enemy Stun", [(APPLY_AURA, ENEMY, 12, 0, 0)], 28, 5,
     {MECHANIC: 12, EFFECT_MECHANIC: 12}),
    (900076, 5, "Ally Heal Over Time", [(APPLY_AURA, ALLY, 8, 3000, 0)], 8, 5,
     {DMG_CLASS: 1, SCHOOL: 2}),
    (900081, 5, "Enemy Direct Damage", [(SCHOOL_DAMAGE, ENEMY, 0, 0, 0)], 0, 5,
     {DMG_CLASS: 1, SCHOOL: 4}),
    (900086, 5, "Ally Direct Heal", [(HEAL, ALLY, 0, 0, 0)], 0, 5, {DMG_CLASS: 1, SCHOOL: 2}),
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


def spell_rows(base):
    for first, count, name, effects, duration, range_id, extra in SPELL_SHELLS:
        for n in range(count):
            row = list(base)
            row[0] = first + n
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
            row[NAME] = f"Reserved {name} {n + 1:02d}"
            row[SUBTEXT] = "RustWrath shell"
            row[DESCRIPTION] = "Reserved spell shell; amounts come from CastCustomSpell."
            yield row


def clone(table, key, base, first, count, updates):
    """Temp-table clone of one stock row into ids first..first+count-1."""
    out = []
    for entry in range(first, first + count):
        sets = ", ".join(f"`{column}` = {value}" for column, value in updates(entry))
        out += [
            f"DROP TEMPORARY TABLE IF EXISTS rw_clone;",
            f"CREATE TEMPORARY TABLE rw_clone SELECT * FROM `{table}` WHERE `{key}` = {base};",
            f"UPDATE rw_clone SET `{key}` = {entry}, {sets};",
            f"INSERT INTO `{table}` SELECT * FROM rw_clone;",
        ]
    return out


def item_updates(entry, usable, bound):
    name = f"Reserved {'Usable' if usable else 'Token'} {entry}"
    return [
        ("name", sql_value(name)), ("description", "''"), ("ScriptName", "''"),
        ("class", 0 if usable else 7), ("subclass", 8 if usable else 11),
        ("displayid", 18077 if usable else 51756), ("Quality", 1), ("Flags", 0),
        ("FlagsExtra", 0), ("BuyPrice", 0), ("SellPrice", 0), ("InventoryType", 0),
        ("AllowableClass", -1), ("AllowableRace", -1), ("ItemLevel", 1), ("RequiredLevel", 0),
        ("maxcount", 0), ("stackable", 1 if usable else 1000), ("bonding", 1 if bound else 0),
        ("spellid_1", BASE_SPELL if usable else 0), ("spelltrigger_1", 0),
        ("spellcharges_1", 0), ("spellcooldown_1", -1), ("spellcategory_1", 0),
        ("spellcategorycooldown_1", -1), ("startquest", 0), ("duration", 0),
    ]


def creature_updates(entry):
    return [
        ("name", sql_value(f"Reserved Creature {entry}")), ("subname", "''"),
        ("minlevel", 1), ("maxlevel", 1), ("faction", 35), ("npcflag", 1), ("rank", 0),
        ("unit_flags", 0), ("AIName", "''"), ("ScriptName", "''"), ("lootid", 0),
        ("pickpocketloot", 0), ("skinloot", 0), ("gossip_menu_id", 0), ("mingold", 0),
        ("maxgold", 0),
    ]


def goober_updates(entry):
    return [
        ("name", sql_value(f"Reserved Object {entry}")), ("Data0", 0), ("Data1", 0),
        ("Data2", 0), ("Data10", 0), ("Data19", 0), ("AIName", "''"), ("ScriptName", "''"),
    ]


def chest_updates(entry):
    return [
        ("name", sql_value(f"Reserved Chest {entry}")), ("Data0", 0), ("Data1", entry),
        ("Data2", 0), ("Data3", 1), ("AIName", "''"), ("ScriptName", "''"),
    ]


def ledger(entity, first, last, shape):
    notes = sql_value('{"owner":"rustwrath_dummy_pool","shape":"%s","doc":"integrations/sql/DUMMY_POOLS.md"}' % shape)
    return [
        f"DELETE FROM wm_reserved_slot WHERE EntityType = '{entity}' AND ReservedID BETWEEN {first} AND {last};",
        f"INSERT INTO wm_reserved_slot (EntityType, ReservedID, SlotStatus, ArcKey, NotesJSON) "
        f"SELECT '{entity}', seq, 'reserved', 'rustwrath_dummy_pool', {notes} FROM ("
        + " UNION ALL ".join(f"SELECT {i} seq" for i in range(first, last + 1))
        + ") ids;",
    ]


def main():
    if len(sys.argv) != 2:
        sys.exit(__doc__)
    base = read_base_spell(sys.argv[1])
    out = ["-- Generated by integrations/sql/world_dummy_pools.py; see DUMMY_POOLS.md.", "START TRANSACTION;"]

    out.append("DELETE FROM item_template WHERE entry BETWEEN 911000 AND 911099;")
    out += clone("item_template", "entry", 3371, 911000, 50, lambda e: item_updates(e, True, True))
    out += clone("item_template", "entry", 3371, 911050, 25, lambda e: item_updates(e, False, True))
    out += clone("item_template", "entry", 3371, 911075, 25, lambda e: item_updates(e, False, False))
    out += ledger("item_template", 911000, 911049, "usable")
    out += ledger("item_template", 911050, 911099, "token")

    out.append("DELETE FROM creature_template_model WHERE CreatureID BETWEEN 920200 AND 920249;")
    out.append("DELETE FROM creature_template WHERE entry BETWEEN 920200 AND 920249;")
    out += clone("creature_template", "entry", 1642, 920200, 50, creature_updates)
    out += [
        f"INSERT INTO creature_template_model (CreatureID, Idx, CreatureDisplayID, DisplayScale, Probability) "
        f"VALUES ({e}, 0, 3167, 1, 1);" for e in range(920200, 920250)
    ]
    out += ledger("creature_template", 920200, 920249, "creature")

    out.append("DELETE FROM gameobject_template WHERE entry BETWEEN 913000 AND 913049;")
    out += clone("gameobject_template", "entry", 166872, 913000, 30, goober_updates)
    out += clone("gameobject_template", "entry", 2843, 913030, 20, chest_updates)
    out += ledger("gameobject_template", 913000, 913029, "goober")
    out += ledger("gameobject_template", 913030, 913049, "chest")

    out.append("DELETE FROM spell_dbc WHERE ID BETWEEN 900001 AND 900090;")
    for row in spell_rows(base):
        out.append("INSERT INTO spell_dbc VALUES (" + ", ".join(sql_value(v) for v in row) + ");")
    out += ledger("spell_dbc_or_spell_slots", 900001, 900090, "spell_shell")

    out += ["DROP TEMPORARY TABLE IF EXISTS rw_clone;", "COMMIT;"]
    print("\n".join(out))


if __name__ == "__main__":
    main()
