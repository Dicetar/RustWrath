//! Wrath spell queries use the lent app host, never a separate catalog or simulated spell state.

use mlua::{Lua, MultiValue, Value};

use super::{host, spellbook, Model};

#[derive(Clone, Debug, PartialEq)]
pub struct SpellInfoView {
    pub name: String,
    pub rank: Option<String>,
    pub icon: Option<String>,
    pub cost: Option<i32>,
    pub channeled: bool,
    pub power_type: i32,
    pub cast_time_ms: u32,
    pub min_range: Option<f32>,
    pub max_range: Option<f32>,
}

/// A presentation interval from the native cast owner, in GetTime milliseconds. The engine
/// owns the identity and timeline; Lua bindings only shape the stock return tuple.
#[derive(Clone, Debug, PartialEq)]
pub struct UnitCastView {
    pub name: String,
    pub rank: Option<String>,
    pub icon: Option<String>,
    pub start_ms: f64,
    pub end_ms: f64,
    pub trade_skill: bool,
    pub cast_id: Option<u64>,
    pub not_interruptible: Option<bool>,
}

fn spell_reference(lua: &Lua, args: &MultiValue) -> mlua::Result<Option<(u32, bool)>> {
    let input = args.front().cloned().unwrap_or(Value::Nil);
    if let Some(book) = args.get(1).filter(|value| !matches!(value, Value::Nil)) {
        let (slot, book) = spellbook::spell_slot_args(input, book.clone(), "GetSpellInfo")?;
        let model = lua.app_data_ref::<Model>().expect("model");
        return Ok(spellbook::book_slot(&model, slot, &book)
            .map(|slot| (slot.spell_id, spellbook::is_pet_book(&book))));
    }
    if let Some(number) = lua.coerce_number(input.clone())? {
        return Ok((number.is_finite() && number >= 1.0 && number <= f64::from(u32::MAX))
            .then_some((number as u32, false)));
    }
    let Value::String(name) = input else { return Ok(None) };
    let name = name.to_str()?;
    let model = lua.app_data_ref::<Model>().expect("model");
    let player_slots = if model.spell_list.is_empty() {
        &model.spellbook.slots
    } else {
        &model.spell_list
    };
    Ok(spellbook::resolve_spell_in_list(player_slots, &model.pet_book, &name)
        .map(|spell| (spell.spell_id, spell.pet)))
}

fn query(lua: &Lua, args: &MultiValue) -> mlua::Result<Option<(u32, SpellInfoView)>> {
    let Some((id, _)) = spell_reference(lua, args)? else { return Ok(None) };
    Ok(host::with_host(lua, |host| host.spell_info(id)).flatten().map(|view| (id, view)))
}

fn string(lua: &Lua, value: Option<&str>) -> mlua::Result<Value> {
    value.map_or(Ok(Value::Nil), |value| lua.create_string(value).map(Value::String))
}

fn unit_cast(lua: &Lua, token: Value, channel: bool) -> mlua::Result<MultiValue> {
    let usage = if channel { "Usage: UnitChannelInfo(unit)" } else { "Usage: UnitCastingInfo(unit)" };
    let token = super::binding_abi::string_arg(lua, token, usage)?;
    super::unit::check_unit_token_for(&Some(token.clone()), true)?;
    let guid = {
        let model = lua.app_data_ref::<Model>().expect("model");
        model.unit(&token).map(|unit| unit.guid)
    };
    let view = guid.and_then(|guid| host::with_host(lua, |host| host.unit_cast(guid, channel)).flatten());
    let Some(view) = view else { return Ok(MultiValue::from_vec(vec![Value::Nil])); };
    let name = string(lua, Some(&view.name))?;
    let mut values = vec![
        name.clone(), string(lua, Some(view.rank.as_deref().unwrap_or("")))?, name,
        string(lua, view.icon.as_deref())?, Value::Number(view.start_ms), Value::Number(view.end_ms),
        Value::Boolean(view.trade_skill),
    ];
    // The target CastingBarFrame uses slot eight for cast identity, but channel tuples omit it.
    if !channel {
        values.push(view.cast_id.map_or(Value::Nil, |id| Value::Number(id as f64)));
    }
    values.push(view.not_interruptible.map_or(Value::Nil, Value::Boolean));
    Ok(MultiValue::from_vec(values))
}

pub(super) fn install(lua: &Lua) -> mlua::Result<()> {
    let globals = lua.globals();
    globals.set("GetSpellCooldown", lua.create_function(|lua, args: MultiValue| {
        let value = if let Some(book) = args.get(1).filter(|book| !matches!(book, Value::Nil)) {
            // Preserve the existing pushed-book and pet-book forms, including their errors.
            spellbook::book_cooldown(lua, (args.front().cloned().unwrap_or(Value::Nil), book.clone()))?
        } else {
            let Some((id, pet)) = spell_reference(lua, &args)? else {
                return Ok(MultiValue::from_vec(vec![Value::Nil]));
            };
            let Some(cooldown) = host::with_host(lua, |host| host.spell_cooldown(id, pet)).flatten() else {
                return Ok(MultiValue::from_vec(vec![Value::Nil]));
            };
            spellbook::cooldown_tuple(Some(cooldown), super::clock::now(lua))
        };
        Ok(MultiValue::from_vec(vec![
            Value::Number(value.0), Value::Number(value.1), Value::Integer(i64::from(value.2)),
        ]))
    })?)?;
    globals.set("UnitCastingInfo", lua.create_function(|lua, token: Value| unit_cast(lua, token, false))?)?;
    globals.set("UnitChannelInfo", lua.create_function(|lua, token: Value| unit_cast(lua, token, true))?)?;
    globals.set("IsSpellKnown", lua.create_function(|lua, (id, pet): (Value, Option<bool>)| {
        let id = lua.coerce_number(id)?.filter(|id| id.is_finite() && *id >= 1.0 && *id <= f64::from(u32::MAX))
            .ok_or_else(|| mlua::Error::runtime("Usage: IsSpellKnown(spellID [, isPet])"))? as u32;
        Ok(host::with_host(lua, |host| host.spell_known(id, pet.unwrap_or(false))).flatten())
    })?)?;
    globals.set("GetSpellInfo", lua.create_function(|lua, args: MultiValue| {
        let Some((_, view)) = query(lua, &args)? else { return Ok(MultiValue::from_vec(vec![Value::Nil])) };
        Ok(MultiValue::from_vec(vec![
            string(lua, Some(&view.name))?,
            string(lua, Some(view.rank.as_deref().unwrap_or("")))?,
            string(lua, view.icon.as_deref())?,
            view.cost.map_or(Value::Nil, |cost| Value::Integer(i64::from(cost))),
            Value::Boolean(view.channeled),
            Value::Integer(i64::from(view.power_type)),
            Value::Integer(i64::from(view.cast_time_ms)),
            view.min_range.map_or(Value::Nil, |range| Value::Number(f64::from(range))),
            view.max_range.map_or(Value::Nil, |range| Value::Number(f64::from(range))),
        ]))
    })?)?;
    globals.set("GetSpellTexture", lua.create_function(|lua, args: MultiValue| {
        string(lua, query(lua, &args)?.as_ref().and_then(|(_, view)| view.icon.as_deref()))
    })?)?;
    globals.set("GetSpellLink", lua.create_function(|lua, args: MultiValue| {
        let Some((id, view)) = query(lua, &args)? else { return Ok(Value::Nil) };
        let link = format!("|cff71d5ff|Hspell:{id}|h[{}]|h|r", view.name);
        string(lua, Some(&link))
    })?)?;
    Ok(())
}
