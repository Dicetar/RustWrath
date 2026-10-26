//! Wrath resource getters over the current unit projection and the existing raw-field owner.

use mlua::{Lua, Value};

use super::{host, unit, Model};

fn power(lua: &Lua, token: Value, kind: Value, raw: Option<bool>, maximum: bool) -> mlua::Result<u32> {
    let token = super::binding_abi::string_arg(lua, token, "Usage: UnitPower(unit [, powerType [, unmodified]])")?;
    unit::check_unit_token(&Some(token.clone()))?;
    let (guid, active) = {
        let model = lua.app_data_ref::<Model>().expect("model");
        let Some(unit) = model.unit(&token) else { return Ok(0) };
        (unit.guid, unit.power_type)
    };
    let kind = if matches!(kind, Value::Nil) { active } else {
        let kind = lua.coerce_number(kind)?
            .filter(|kind| kind.is_finite() && *kind >= 0.0 && *kind <= 6.0)
            .ok_or_else(|| mlua::Error::runtime("Invalid Wrath power type"))?;
        kind as u8
    };
    if let Some(value) = host::with_host(lua, |host| host.unit_power(guid, kind, maximum)).flatten() {
        let scale = match kind { 1 | 6 => 10, 4 => 1000, _ => 1 };
        return Ok(if raw.unwrap_or(false) { value } else { value / scale });
    }
    // Do not fall back to a retained UI amount after the owner refused a scope/field read.
    // Nor can a rounded display amount reconstruct the unmodified protocol value.
    Ok(0)
}

pub(super) fn install(lua: &Lua) -> mlua::Result<()> {
    let globals = lua.globals();
    globals.set("UnitPower", lua.create_function(|lua, (token, kind, raw): (Value, Value, Option<bool>)| {
        power(lua, token, kind, raw, false)
    })?)?;
    globals.set("UnitPowerMax", lua.create_function(|lua, (token, kind, raw): (Value, Value, Option<bool>)| {
        power(lua, token, kind, raw, true)
    })?)?;
    globals.set("UnitPowerType", lua.create_function(|lua, token: Value| {
        let token = super::binding_abi::string_arg(lua, token, "Usage: UnitPowerType(unit)")?;
        unit::check_unit_token(&Some(token.clone()))?;
        let kind = lua.app_data_ref::<Model>().expect("model").unit(&token).map_or(0, |unit| unit.power_type);
        let name = match kind {
            0 => "MANA", 1 => "RAGE", 2 => "FOCUS", 3 => "ENERGY", 4 => "HAPPINESS",
            5 => "RUNES", 6 => "RUNIC_POWER", _ => "MANA",
        };
        Ok((u32::from(kind), name))
    })?)?;
    Ok(())
}
