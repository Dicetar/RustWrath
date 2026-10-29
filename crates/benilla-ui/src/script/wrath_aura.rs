//! Wrath aura queries over native slots, independent of the lossy 1.12 descriptor projection.
//! Tuple consumer: the target BuffFrame.lua / AuraButton_Update. Missing policy or expiration
//! observations are nil; a hidden-duration aura uses the client's numeric zero convention.
//! Timed entries use the native receipt's presentation anchor, never the query's time.

use mlua::{Lua, MultiValue, Table, Value, Variadic};
use super::{host, Model};

#[derive(Clone, Debug, PartialEq)]
pub struct WrathAuraView {
    pub slot: u8,
    pub spell_id: u32,
    pub name: String,
    pub rank: Option<String>,
    pub icon: Option<String>,
    pub count: u8,
    pub level: u8,
    pub dispel: Option<String>,
    pub helpful: Option<bool>,
    pub caster: Option<u64>,
    pub duration: f64,
    pub expiration: Option<f64>,
    pub cancelable: bool,
    pub stealable: Option<bool>,
    pub consolidate: Option<bool>,
}

#[derive(Default)]
struct Filter {
    helpful: bool,
    player: bool,
    cancelable: Option<bool>,
}

impl Filter {
    fn parse(lua: &Lua, value: Value, forced: Option<bool>) -> mlua::Result<Self> {
        let mut filter = Self { helpful: forced.unwrap_or(true), ..Default::default() };
        if matches!(value, Value::Nil) { return Ok(filter); }
        let text = super::binding_abi::string_arg(lua, value, "Usage: UnitAura(unit, index [, filter])")?;
        for word in text.split(|c: char| c == '|' || c.is_ascii_whitespace()).filter(|s| !s.is_empty()) {
            match word.to_ascii_uppercase().as_str() {
                "HELPFUL" if forced.is_none() => filter.helpful = true,
                "HARMFUL" if forced.is_none() => filter.helpful = false,
                "HELPFUL" | "HARMFUL" => {},
                "PLAYER" => filter.player = true,
                "CANCELABLE" => filter.cancelable = Some(true),
                "NOT_CANCELABLE" => filter.cancelable = Some(false),
                "RAID" => return Err(mlua::Error::runtime("Wrath RAID aura filtering requires the native raid policy")),
                _ => return Err(mlua::Error::runtime(format!("Invalid aura filter: {word}"))),
            }
        }
        Ok(filter)
    }
}

fn text(lua: &Lua, value: Option<&str>) -> mlua::Result<Value> {
    value.map_or(Ok(Value::Nil), |text| lua.create_string(text).map(Value::String))
}

fn caster_token(lua: &Lua, guid: Option<u64>) -> Option<String> {
    let guid = guid?;
    let model = lua.app_data_ref::<Model>().expect("model");
    model.unit_guids.tokens_of(guid).into_iter().next()
}

fn find(lua: &Lua, args: &MultiValue, forced: Option<bool>) -> mlua::Result<Option<(u64, WrathAuraView)>> {
    let token = super::binding_abi::string_arg(lua, args.front().cloned().unwrap_or(Value::Nil),
        "Usage: UnitAura(unit, index or name [, rank] [, filter])")?;
    super::unit::check_unit_token_for(&Some(token.clone()), true)?;
    let input = args.get(1).cloned().unwrap_or(Value::Nil);
    let number = lua.coerce_number(input.clone())?;
    let (index, name, rank, filter_arg) = if let Some(number) = number {
        (Some(number), None, None, args.get(2).cloned().unwrap_or(Value::Nil))
    } else {
        let name = super::binding_abi::string_arg(lua, input, "Aura index or name expected")?;
        let rank = match args.get(2) {
            None | Some(Value::Nil) => None,
            Some(value) => Some(super::binding_abi::string_arg(lua, value.clone(), "Aura rank must be a string")?),
        };
        (None, Some(name), rank, args.get(3).cloned().unwrap_or(Value::Nil))
    };
    let filter = Filter::parse(lua, filter_arg, forced)?;
    let (guid, player) = {
        let model = lua.app_data_ref::<Model>().expect("model");
        (model.unit_guids.guid_of(&token)?, model.unit_guids.player)
    };
    let Some(guid) = guid else { return Ok(None); };
    let Some(auras) = host::with_host(lua, |host| host.unit_auras(guid)).flatten() else { return Ok(None); };
    let mut listed = auras.into_iter().filter(|aura|
        aura.helpful == Some(filter.helpful) && (!filter.player || aura.caster == Some(player))
        && filter.cancelable.is_none_or(|cancelable| aura.cancelable == cancelable));
    let found = if let Some(index) = index {
        if !index.is_finite() || index < 1.0 || index > 255.0 { None }
        else { listed.nth(index as usize - 1) }
    } else {
        listed.find(|aura| Some(&aura.name) == name.as_ref()
            && rank.as_ref().is_none_or(|rank| aura.rank.as_deref().unwrap_or("") == rank.as_str()))
    };
    Ok(found.map(|aura| (guid, aura)))
}

fn query(lua: &Lua, args: MultiValue, forced: Option<bool>) -> mlua::Result<MultiValue> {
    let Some((_, aura)) = find(lua, &args, forced)? else { return Ok(MultiValue::from_vec(vec![Value::Nil])); };
    let caster = caster_token(lua, aura.caster);
    Ok(MultiValue::from_vec(vec![
        text(lua, Some(&aura.name))?, text(lua, Some(aura.rank.as_deref().unwrap_or("")))?,
        text(lua, aura.icon.as_deref())?, Value::Integer(i64::from(aura.count)),
        text(lua, aura.dispel.as_deref())?, Value::Number(aura.duration),
        aura.expiration.map_or(Value::Nil, Value::Number), text(lua, caster.as_deref())?,
        aura.stealable.map_or(Value::Nil, Value::Boolean), aura.consolidate.map_or(Value::Nil, Value::Boolean),
        Value::Integer(i64::from(aura.spell_id)),
    ]))
}

pub(super) fn install(lua: &Lua) -> mlua::Result<()> {
    let globals = lua.globals();
    globals.set("UnitAura", lua.create_function(|lua, args: MultiValue| query(lua, args, None))?)?;
    globals.set("UnitBuff", lua.create_function(|lua, args: MultiValue| query(lua, args, Some(true)))?)?;
    globals.set("UnitDebuff", lua.create_function(|lua, args: MultiValue| query(lua, args, Some(false)))?)?;
    globals.set("CancelUnitBuff", lua.create_function(|lua, args: MultiValue| {
        if let Some((guid, aura)) = find(lua, &args, Some(true))? {
            let player = lua.app_data_ref::<Model>().expect("model").unit_guids.player;
            if guid == player && aura.cancelable { super::aura::cancel_aura(lua, aura.spell_id)?; }
        }
        Ok(())
    })?)?;
    Ok(())
}

pub(super) fn install_tooltip_methods(lua: &Lua, methods: &Table) -> mlua::Result<()> {
    for (name, forced) in [("SetUnitAura", None), ("SetUnitBuff", Some(true)), ("SetUnitDebuff", Some(false))] {
        methods.set(name, lua.create_function(move |lua, (this, args): (Table, Variadic<Value>)| {
            super::object::frame_handle_of(lua, &this)?;
            // Match the getter's filtered ordinal or name/rank, including slots beyond 47.
            let args = MultiValue::from_vec(args.into_iter().collect());
            let Some((_, aura)) = find(lua, &args, forced)? else { return Ok(()); };
            let key = super::AuraKey { spell_id: aura.spell_id, level: aura.level, stack: aura.count.max(1) };
            let remaining = aura.expiration.filter(|at| at.is_finite() && *at > 0.0)
                .map(|at| ((at - super::clock::now(lua)) * 1000.0)
                    .round().clamp(0.0, f64::from(u32::MAX)) as u32)
                .and_then(|ms| super::tooltip::duration_text(ms, "SPELL_TIME_REMAINING", true,
                    &|key| lua.globals().get::<String>(key).ok()));
            super::tooltip_aura::set_aura(lua, &this, key, Some(aura.name), remaining)
        })?)?;
    }
    Ok(())
}
