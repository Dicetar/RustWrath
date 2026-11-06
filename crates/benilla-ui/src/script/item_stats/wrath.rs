//! Wrath item queries over the existing template cache. The installed spellmaking UI reads
//! icon at result 10; Bagnon reads item level at 4. Neither uses the donor nine-value tuple.

use mlua::{Lua, MultiValue, Value};

use super::{equip_loc_token, suffixed_name, ItemTemplateView, Model, QUALITY_COLORS};

/// The 12340 item link payload, as AzerothCore's HyperlinkTags.cpp item::StoreTo consumes it.
/// Keep the signed property and suffix factor distinct; negative-suffix scaling is not yet
/// represented by the shared RandomPropertyView and must not become a positive property.
#[derive(Clone, Default)]
pub(in crate::script) struct ItemLink {
    pub id: u32,
    pub enchant: i32,
    pub gems: [u32; 3],
    reserved: u32,
    pub random_property: i32,
    suffix_factor: u32,
    render_level: u32,
}

impl ItemLink {
    pub(super) fn template(id: u32) -> Self {
        Self { id, ..Default::default() }
    }

    pub fn parse(text: &str) -> Option<Self> {
        let payload = text.split_once("|H").map_or(text, |(_, data)| data);
        if !payload.get(..5)?.eq_ignore_ascii_case("item:") {
            return None;
        }
        let mut fields = payload[5..].split('|').next()?.split(':');
        let id = fields.next()?.trim().parse().ok().filter(|&id| id != 0)?;
        let mut next = || fields.next().unwrap_or("").trim().parse::<i64>().unwrap_or(0);
        Some(Self {
            id,
            enchant: i32::try_from(next()).unwrap_or(0),
            gems: std::array::from_fn(|_| u32::try_from(next()).unwrap_or(0)),
            reserved: u32::try_from(next()).unwrap_or(0),
            random_property: i32::try_from(next()).unwrap_or(0),
            suffix_factor: u32::try_from(next()).unwrap_or(0),
            render_level: u32::try_from(next()).unwrap_or(0),
        })
    }

    pub(super) fn hyperlink(&self, name: &str, quality: u32) -> String {
        let quality = if quality == 7 { 6 } else { quality };
        let color = QUALITY_COLORS.get(quality as usize).unwrap_or(&QUALITY_COLORS[1]).3;
        format!(
            "{color}|Hitem:{}:{}:{}:{}:{}:{}:{}:{}:{}|h[{name}]|h|r",
            self.id, self.enchant, self.gems[0], self.gems[1], self.gems[2],
            self.reserved, self.random_property, self.suffix_factor, self.render_level,
        )
    }
}

fn cached_name_id(lua: &Lua, name: &str) -> Option<u32> {
    let model = lua.app_data_ref::<Model>().expect("model app_data");
    let mut matches = model.item_templates.iter()
        .filter(|(_, item)| item.name == name).map(|(&id, _)| id);
    let id = matches.next()?;
    matches.next().is_none().then_some(id)
}

fn argument(lua: &Lua, value: Value) -> mlua::Result<Option<ItemLink>> {
    if let Some(number) = lua.coerce_number(value.clone())? {
        return Ok((number.is_finite() && number >= 1.0 && number <= f64::from(u32::MAX))
            .then_some(ItemLink { id: number as u32, ..Default::default() }));
    }
    let Value::String(text) = value else {
        return Err(mlua::Error::runtime("Usage: GetItemInfo(itemID|\"itemlink\")"));
    };
    let text = text.to_str()?;
    Ok(ItemLink::parse(&text).or_else(|| cached_name_id(lua, &text).map(ItemLink::template)))
}

fn query(lua: &Lua, value: Value) -> mlua::Result<Option<(ItemLink, ItemTemplateView)>> {
    let Some(link) = argument(lua, value)? else { return Ok(None) };
    let mut model = lua.app_data_mut::<Model>().expect("model app_data");
    let view = model.item_templates.get(&link.id).cloned();
    if view.is_none() {
        // Same late-cache read owner as the donor binding; no query is executed here.
        model.item_stat_reads.insert(link.id);
    }
    Ok(view.map(|view| (link, view)))
}

pub(in crate::script) fn item_name(lua: &Lua, base: &str, property: i32) -> Option<String> {
    if property >= 0 {
        return Some(suffixed_name(lua, base, property));
    }
    let suffix = super::super::host::with_host(lua, |host| host.item_suffix_name(property)).flatten()?;
    let template = super::super::glue_core::global_text(lua, "ITEM_SUFFIX_TEMPLATE");
    Some(crate::strings::fill(
        &template,
        &[crate::strings::Arg::S(base), crate::strings::Arg::S(&suffix)],
    ))
}

pub(super) fn install(lua: &Lua) -> mlua::Result<()> {
    lua.globals().set("GetItemFamily", lua.create_function(|lua, arg: Value| {
        let Some((link, _)) = query(lua, arg)? else { return Ok(None) };
        Ok(super::super::host::with_host(lua, |host| host.item_family(link.id)).flatten())
    })?)?;
    lua.globals().set("GetItemSpell", lua.create_function(|lua, arg: Value| {
        let Some((link, _)) = query(lua, arg)? else { return Ok((None, None)) };
        let spell = super::super::host::with_host(lua, |host| host.item_spell(link.id)).flatten();
        Ok(match spell {
            Some((name, id)) => (Some(name), Some(id)),
            None => (None, None),
        })
    })?)?;
    lua.globals().set("IsEquippableItem", lua.create_function(|lua, arg: Value| {
        Ok(query(lua, arg)?.map(|(_, item)| item.inventory_type != 0))
    })?)?;
    lua.globals().set("GetItemCooldown", lua.create_function(|lua, id: Value| {
        let id = lua.coerce_number(id)?
            .filter(|id| id.is_finite() && *id >= 1.0 && *id <= f64::from(u32::MAX))
            .ok_or_else(|| mlua::Error::runtime("Usage: GetItemCooldown(itemID)"))? as u32;
        let Some((start, duration, enabled)) =
            super::super::host::with_host(lua, |host| host.item_cooldown(id)).flatten()
        else {
            return Ok(MultiValue::from_vec(vec![Value::Nil]));
        };
        Ok(MultiValue::from_vec(vec![
            Value::Number(start as f64 / 1000.0),
            Value::Number(f64::from(duration) / 1000.0),
            Value::Integer(i64::from(enabled)),
        ]))
    })?)?;
    lua.globals().set("GetItemCount", lua.create_function(|lua, args: MultiValue| {
        let input = args.front().cloned().unwrap_or(Value::Nil);
        let id = if let Some(number) = lua.coerce_number(input.clone())? {
            (number.is_finite() && number >= 1.0 && number <= f64::from(u32::MAX))
                .then_some(number as u32)
        } else if let Value::String(text) = input {
            let text = text.to_str()?;
            ItemLink::parse(&text).map(|link| link.id).or_else(|| cached_name_id(lua, &text))
        } else {
            return Err(mlua::Error::runtime("Usage: GetItemCount(item [, includeBank [, includeUses]])"));
        };
        let Some(id) = id else { return Ok(0) };
        let bank = super::super::binding_abi::bool_or_default(args.get(1), false);
        let uses = super::super::binding_abi::bool_or_default(args.get(2), false);
        Ok(super::super::host::with_host(lua, |host| host.item_count(id, bank, uses))
            .flatten()
            .unwrap_or(0))
    })?)?;
    lua.globals().set("GetItemInfo", lua.create_function(|lua, arg: Value| {
        let Some((link, view)) = query(lua, arg)? else { return Ok(MultiValue::new()) };
        let Some(name) = item_name(lua, &view.name, link.random_property) else {
            return Ok(MultiValue::new());
        };
        Ok(MultiValue::from_vec(vec![
            Value::String(lua.create_string(&name)?),
            Value::String(lua.create_string(link.hyperlink(&name, view.quality))?),
            Value::Integer(i64::from(view.quality)),
            Value::Integer(i64::from(view.item_level)),
            Value::Integer(i64::from(view.required_level)),
            Value::String(lua.create_string(view.item_type.as_deref().unwrap_or(""))?),
            Value::String(lua.create_string(view.item_sub_type.as_deref().unwrap_or(""))?),
            Value::Integer(i64::from(view.stackable)),
            Value::String(lua.create_string(equip_loc_token(view.inventory_type))?),
            view.icon.as_deref().map_or(Ok(Value::Nil), |icon| {
                lua.create_string(icon).map(Value::String)
            })?,
            Value::Integer(i64::from(view.sell_price)),
        ]))
    })?)?;
    // WM_SpellmakingV3.lua:250 queries a reagent icon directly from the same template view.
    lua.globals().set("GetItemIcon", lua.create_function(|lua, arg: Value| {
        match query(lua, arg)?.and_then(|(_, view)| view.icon) {
            Some(icon) => lua.create_string(icon).map(Value::String),
            None => Ok(Value::Nil),
        }
    })?)?;
    Ok(())
}
