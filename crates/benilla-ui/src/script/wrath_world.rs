//! The 3.3.5a world and unit-popup queries whose facts already have owners here: the current
//! map's `Map.dbc` `InstanceType` ([`Model::instance_type`], pushed by the app's instance feed),
//! the frame-script CVar table, and the player's own loot-roll preference. A fact with no
//! producer answers the API's unavailable value (nil) instead of a fabricated default; the
//! missing producer is named where it stands.

use mlua::{Lua, Value};

use super::binding_abi::{flag, flag_or_default, string_arg};
use super::instance::instance_type_name;
use super::Model;

/// Register the queries. Only the Wrath target installs this module.
pub(super) fn install(lua: &Lua) -> mlua::Result<()> {
    let g = lua.globals();

    // `GetInstanceInfo()` (`UnitPopup.lua:245`, `Minimap.lua:486`): `(name, instanceType,
    // difficultyID, difficultyName, maxPlayers, playerDifficultyID, isDynamicDifficulty,
    // instanceID)`. The pushed `Map.dbc` row supplies the type, name and player cap (a party
    // instance's five); no difficulty producer exists, so those slots stay nil. Outside an
    // instance the name is nil, the open world's answer, even though its map row has a name.
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
            Ok(mlua::MultiValue::from_vec(vec![
                name,
                Value::String(lua.create_string(instance_type_name(instance_type))?),
                Value::Nil,
                Value::Nil,
                Value::Integer(i64::from(max_players)),
                Value::Nil,
                Value::Boolean(false),
                Value::Integer(0),
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

    // `GetDungeonDifficulty()` / `GetRaidDifficulty()` (`UnitPopup.lua:700`, `:703`): the setting
    // lives in the `dungeonDifficulty` / `playerDifficulty` CVar rows. A host that never
    // registered them answers nil, never a fabricated "normal" tier; the server's
    // `MSG_SET_DUNGEON_DIFFICULTY` echo is a producer that does not exist yet.
    g.set(
        "GetDungeonDifficulty",
        lua.create_function(|lua, ()| cvar_int(lua, "dungeondifficulty"))?,
    )?;
    g.set(
        "GetRaidDifficulty",
        lua.create_function(|lua, ()| cvar_int(lua, "playerdifficulty"))?,
    )?;
    // The setters are the client's own selection (the reference's popup calls them); the server
    // send that must follow is named above. An unregistered row is a silent no-op, like every
    // engine CVar write.
    g.set(
        "SetDungeonDifficulty",
        lua.create_function(|lua, value: Value| set_cvar_int(lua, "dungeondifficulty", value))?,
    )?;
    g.set(
        "SetRaidDifficulty",
        lua.create_function(|lua, value: Value| set_cvar_int(lua, "playerdifficulty", value))?,
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

/// A CVar row as the integer the API returns; nil when the host never registered it or the text
/// is not a number.
fn cvar_int(lua: &Lua, name: &str) -> mlua::Result<Value> {
    let model = lua.app_data_ref::<Model>().expect("model app_data");
    Ok(model
        .cvars
        .get(name)
        .and_then(|slot| slot.value.trim().parse::<i64>().ok())
        .map_or(Value::Nil, Value::Integer))
}

/// Write an integer into a CVar row through the one store every write uses (checks and the host
/// change queue included). A nil argument stores nothing.
fn set_cvar_int(lua: &Lua, name: &str, value: Value) -> mlua::Result<()> {
    if value.is_nil() {
        return Ok(());
    }
    let n = super::binding_abi::coerced_number(lua, Some(value)) as i64;
    super::cvars::set_from_engine(
        &mut lua.app_data_mut::<Model>().expect("model app_data"),
        name,
        n.to_string(),
    );
    Ok(())
}
