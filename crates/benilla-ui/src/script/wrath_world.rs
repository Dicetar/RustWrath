//! The 3.3.5a world and unit-popup queries whose facts already have owners here: the current
//! map's `Map.dbc` `InstanceType` ([`Model::instance_type`], pushed by the app's instance feed),
//! the difficulty cells ([`super::instance::Difficulty`], fed by `SMSG_INSTANCE_DIFFICULTY`, the
//! `MSG_SET_*_DIFFICULTY` echoes and `SMSG_GROUP_LIST`), and the player's own loot-roll
//! preference. A fact with no producer answers the API's unavailable value (nil) instead of a
//! fabricated default; the missing producer is named where it stands.

use mlua::{Lua, Value};

use super::binding_abi::{flag, flag_or_default, string_arg};
use super::instance::instance_type_name;
use super::Model;

/// Register the queries. Only the Wrath target installs this module.
pub(super) fn install(lua: &Lua) -> mlua::Result<()> {
    let g = lua.globals();

    // `GetInstanceInfo()` (`0x51A8C0`; `UnitPopup.lua:245`, `Minimap.lua:486`): `(name,
    // instanceType, difficulty, difficultyName, maxPlayers, dynamicDifficulty, isDynamic)`. The
    // pushed `Map.dbc` row supplies the type, name and player cap (a party instance's five);
    // `SMSG_INSTANCE_DIFFICULTY` the difficulty (plus one) and the dynamic difficulty. The
    // difficulty name is the `MapDifficulty.dbc` row's, which no catalog here loads, so it is the
    // reference's no-row answer `""`; `isDynamic` is `Map.dbc` flag `0x100`, also unloaded, so
    // false. Outside an instance the name is nil, the open world's answer, even though its map row
    // has a name.
    g.set(
        "GetInstanceInfo",
        lua.create_function(|lua, ()| {
            let model = lua.app_data_ref::<Model>().expect("model app_data");
            let instance_type = model.instance_type.unwrap_or(0);
            let info = model.instance_info.as_ref();
            let name = match info {
                Some(i) if instance_type != 0 => match i.name.as_deref() {
                    Some(n) => Value::String(lua.create_string(n)?),
                    None => Value::Nil,
                },
                _ => Value::Nil,
            };
            let max_players = info
                .filter(|_| instance_type != 0)
                .map_or(0, |i| i.max_players);
            let difficulty = model.difficulty;
            Ok(mlua::MultiValue::from_vec(vec![
                name,
                Value::String(lua.create_string(instance_type_name(instance_type))?),
                Value::Integer(i64::from(difficulty.instance) + 1),
                Value::String(lua.create_string("")?),
                Value::Integer(i64::from(max_players)),
                Value::Integer(i64::from(difficulty.dynamic)),
                Value::Boolean(false),
            ]))
        })?,
    )?;

    // `UnitInBattleground(unit)` (`UnitPopup.lua:479`): every unit this client can see shares its
    // own instance, so a unit that exists is in the battleground when the current map's type is
    // the battleground's (3).
    g.set(
        "UnitInBattleground",
        lua.create_function(|lua, token: Value| {
            let token = string_arg(lua, token, r#"Usage: UnitInBattleground("unit")"#)?;
            let model = lua.app_data_ref::<Model>().expect("model app_data");
            let in_battleground = model.instance_type == Some(3) && model.unit(&token).is_some();
            Ok(flag(in_battleground))
        })?,
    )?;

    // `GetInstanceDifficulty()` (`0x515750`): `m_instanceDifficulty + 1`, 1 until the first
    // `SMSG_INSTANCE_DIFFICULTY`.
    g.set(
        "GetInstanceDifficulty",
        lua.create_function(|lua, ()| {
            let model = lua.app_data_ref::<Model>().expect("model app_data");
            Ok(i64::from(model.difficulty.instance) + 1)
        })?,
    )?;
    // `GetDungeonDifficulty()` (`0x515790`; `UnitPopup.lua:700`): the effective difficulty (the
    // group's in a party, else the player's) and the player's own, each plus one.
    g.set(
        "GetDungeonDifficulty",
        lua.create_function(|lua, ()| {
            let model = lua.app_data_ref::<Model>().expect("model app_data");
            let d = model.difficulty;
            Ok((
                i64::from(d.current_dungeon(&model.party)) + 1,
                i64::from(d.dungeon) + 1,
            ))
        })?,
    )?;
    // `GetRaidDifficulty()` (`0x515810`; `UnitPopup.lua:703`): the group's in a raid, else the
    // player's, then the player's own, each plus one.
    g.set(
        "GetRaidDifficulty",
        lua.create_function(|lua, ()| {
            let model = lua.app_data_ref::<Model>().expect("model app_data");
            let d = model.difficulty;
            Ok((
                i64::from(d.current_raid(&model.party)) + 1,
                i64::from(d.raid) + 1,
            ))
        })?,
    )?;
    // The setters take the 1-based difficulty the menus pass, write it locally at once as the
    // reference does, and send it 0-based (`MSG_SET_DUNGEON_DIFFICULTY` /
    // `MSG_SET_RAID_DIFFICULTY`); the server's echo of the same opcode then confirms or corrects.
    g.set(
        "SetDungeonDifficulty",
        lua.create_function(|lua, value: Value| {
            super::instance::set_dungeon_difficulty_verb(lua, value)
        })?,
    )?;
    g.set(
        "SetRaidDifficulty",
        lua.create_function(|lua, value: Value| {
            super::instance::set_raid_difficulty_verb(lua, value)
        })?,
    )?;

    // `GetOptOutOfLoot()` / `SetOptOutOfLoot(value)` (`UnitPopup.lua:238`, `:1305`, `:1308`):
    // the player's own preference, false on a fresh client. A set also submits the send
    // (`CMSG_OPT_OUT_OF_LOOT`, 0x409), which the app's loot steps put on the wrath wire.
    g.set(
        "GetOptOutOfLoot",
        lua.create_function(|lua, ()| {
            let model = lua.app_data_ref::<Model>().expect("model app_data");
            Ok(flag(model.opt_out_of_loot))
        })?,
    )?;
    g.set(
        "SetOptOutOfLoot",
        lua.create_function(|lua, value: Option<Value>| {
            let pass = flag_or_default(value.as_ref(), 0) != 0;
            lua.app_data_mut::<Model>()
                .expect("model app_data")
                .opt_out_of_loot = pass;
            super::calls::submit(
                lua,
                super::ScriptCall::Loot(super::LootCall::OptOut { pass }),
            )
        })?,
    )?;

    // `UnitThreatSituation(unit)` (`UnitFrame.lua:434`): 1..3 by the server's threat state, nil
    // with none. No `SMSG_THREAT_UPDATE`-family producer reaches the unit view yet, so every
    // unit answers nil instead of a fabricated "no threat".
    g.set(
        "UnitThreatSituation",
        lua.create_function(|_, _: mlua::MultiValue| Ok(Value::Nil))?,
    )?;

    Ok(())
}
