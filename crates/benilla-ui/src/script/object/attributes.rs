//! Wrath frame attributes. Storage and change notification only; this does not make a frame
//! protected, execute secure snippets or grant an attribute the authority to send an action.

use mlua::{Lua, MultiValue, Table, Value};

use super::frame_handle_of;
use crate::script::{event, Model};

const REG_ATTRIBUTES: &str = "__benilla_frame_attributes";

pub(super) fn install(lua: &Lua, methods: &Table) -> mlua::Result<()> {
    lua.set_named_registry_value(REG_ATTRIBUTES, lua.create_table()?)?;
    // 12340 `Script_GetAttribute` (`CSimpleFrame_Scripts.cpp`, `0x0049F2D0`):
    // 1-arg `(name)` looks up `name`; 3-arg `(prefix, name, suffix)` checks
    // `"{prefix}{name}{suffix}"`, `"*{name}{suffix}"`, `"{prefix}{name}*"`, `"*{name}*"`, `"{name}"`.
    methods.set(
        "GetAttribute",
        lua.create_function(
            |lua, (this, args): (Table, MultiValue)| {
                let h = frame_handle_of(lua, &this)?;
                let (id, wrath) = {
                    let mut model = lua.app_data_mut::<Model>().expect("model");
                    (model.frame_id(h), model.is_wrath())
                };
                let attributes: Table = lua.named_registry_value(REG_ATTRIBUTES)?;
                let Some(per) = attributes.raw_get::<Option<Table>>(id)? else {
                    return Ok(Value::Nil);
                };
                if wrath && args.len() == 3 {
                    if let Some(name) = lua.coerce_string(args[1].clone())? {
                        let name = name.to_str()?.to_owned();
                        let prefix = lua.coerce_string(args[0].clone())?
                            .map(|s| s.to_string_lossy()).unwrap_or_default();
                        let suffix = lua.coerce_string(args[2].clone())?
                            .map(|s| s.to_string_lossy()).unwrap_or_default();
                        let candidates = [
                            format!("{prefix}{name}{suffix}"),
                            format!("*{name}{suffix}"),
                            format!("{prefix}{name}*"),
                            format!("*{name}*"),
                            name,
                        ];
                        for key in candidates {
                            let val = per.raw_get::<Value>(key.as_str())?;
                            if !matches!(val, Value::Nil) {
                                return Ok(val);
                            }
                        }
                        return Ok(Value::Nil);
                    }
                }
                let name = super::super::binding_abi::string_arg(
                    lua, args.front().cloned().unwrap_or(Value::Nil), "Usage: frame:GetAttribute(name)")?;
                per.raw_get::<Value>(name.as_str())
            },
        )?,
    )?;
    methods.set("SetAttribute", lua.create_function(|lua, (this, name, value): (Table, String, Value)| {
        let h = frame_handle_of(lua, &this)?;
        let id = lua.app_data_mut::<Model>().expect("model").frame_id(h);
        let attributes: Table = lua.named_registry_value(REG_ATTRIBUTES)?;
        let per = match attributes.raw_get::<Option<Table>>(id)? {
            Some(per) => per,
            None => {
                let per = lua.create_table()?;
                attributes.raw_set(id, per.clone())?;
                per
            }
        };
        // Every set notifies, a same-value one included: the stock dropdown re-sets
        // `createframes` to `true` on each button add to make
        // `UIDropDownMenuDelegate_OnAttributeChanged` grow the list
        // (`UIDropDownMenu.lua:223`-`225`), so a value-diff guard would stall its menus at the
        // eight template buttons.
        per.raw_set(name.as_str(), value.clone())?;
        let args = vec![Value::String(lua.create_string(name)?), value];
        if let Err(error) = event::fire_widget_handler(lua, id, "OnAttributeChanged", args) {
            lua.app_data_mut::<Model>().expect("model").record_lua_error(&error);
        }
        Ok(())
    })?)?;
    Ok(())
}
