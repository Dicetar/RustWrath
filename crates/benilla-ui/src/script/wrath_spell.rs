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

fn spell_id(lua: &Lua, args: &MultiValue) -> mlua::Result<Option<u32>> {
    let input = args.front().cloned().unwrap_or(Value::Nil);
    if let Some(book) = args.get(1).filter(|value| !matches!(value, Value::Nil)) {
        let (slot, book) = spellbook::spell_slot_args(input, book.clone(), "GetSpellInfo")?;
        let model = lua.app_data_ref::<Model>().expect("model");
        return Ok(spellbook::book_slot(&model, slot, &book).map(|slot| slot.spell_id));
    }
    if let Some(number) = lua.coerce_number(input.clone())? {
        return Ok((number.is_finite() && number >= 1.0 && number <= f64::from(u32::MAX)).then_some(number as u32));
    }
    let Value::String(name) = input else { return Ok(None) };
    let name = name.to_str()?;
    let model = lua.app_data_ref::<Model>().expect("model");
    Ok(spellbook::resolve_spell_in_books(&model.spellbook, &model.pet_book, &name).map(|spell| spell.spell_id))
}

fn query(lua: &Lua, args: &MultiValue) -> mlua::Result<Option<(u32, SpellInfoView)>> {
    let Some(id) = spell_id(lua, args)? else { return Ok(None) };
    Ok(host::with_host(lua, |host| host.spell_info(id)).flatten().map(|view| (id, view)))
}

fn string(lua: &Lua, value: Option<&str>) -> mlua::Result<Value> {
    value.map_or(Ok(Value::Nil), |value| lua.create_string(value).map(Value::String))
}

pub(super) fn install(lua: &Lua) -> mlua::Result<()> {
    let globals = lua.globals();
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
