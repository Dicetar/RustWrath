//! Wrath frame attributes. Storage and change notification only; this does not make a frame
//! protected, execute secure snippets or grant an attribute the authority to send an action.

use mlua::{Lua, Table, Value};

use super::frame_handle_of;
use crate::script::{event, Model};

const REG_ATTRIBUTES: &str = "__benilla_frame_attributes";

pub(super) fn install(lua: &Lua, methods: &Table) -> mlua::Result<()> {
    lua.set_named_registry_value(REG_ATTRIBUTES, lua.create_table()?)?;
    methods.set("GetAttribute", lua.create_function(|lua, (this, name): (Table, String)| {
        let h = frame_handle_of(lua, &this)?;
        let id = lua.app_data_mut::<Model>().expect("model").frame_id(h);
        let attributes: Table = lua.named_registry_value(REG_ATTRIBUTES)?;
        match attributes.raw_get::<Option<Table>>(id)? {
            Some(attributes) => attributes.raw_get::<Value>(name),
            None => Ok(Value::Nil),
        }
    })?)?;
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
        let previous: Value = per.raw_get(name.as_str())?;
        if previous == value { return Ok(()); }
        per.raw_set(name.as_str(), value.clone())?;
        let args = vec![Value::String(lua.create_string(name)?), value];
        if let Err(error) = event::fire_widget_handler(lua, id, "OnAttributeChanged", args) {
            lua.app_data_mut::<Model>().expect("model").record_lua_error(&error);
        }
        Ok(())
    })?)?;
    Ok(())
}
