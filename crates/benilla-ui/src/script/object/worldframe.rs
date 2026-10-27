//! The TBC/3.3.5 `WorldFrame:IgnoreDepth`: the world frame's opt-out of the world render's depth
//! test, which the stock `WorldFrame.xml` onLoad demands, and the 1.13-era method tables lack.
//! The flag is stored under a name the attributes API does not reach, so `GetAttribute` can never
//! show it. The consumer that would render with it is the renderer seam, later.

use mlua::{Lua, Table};

use super::frame_handle_of;
use crate::script::Model;
use crate::widget::FrameKind;

const REG_IGNORE_DEPTH: &str = "__benilla_frame_ignore_depth";

pub(super) fn install(lua: &Lua, methods: &Table) -> mlua::Result<()> {
    lua.set_named_registry_value(REG_IGNORE_DEPTH, lua.create_table()?)?;
    methods.set(
        "IgnoreDepth",
        lua.create_function(|lua, (this, ignore): (Table, Option<bool>)| {
            let h = frame_handle_of(lua, &this)?;
            let (kind_ok, id) = {
                let mut model = lua.app_data_mut::<Model>().expect("model");
                let kind = model.arena.frame(h).map(|f| f.kind);
                (kind == Some(FrameKind::WorldFrame), model.frame_id(h))
            };
            if !kind_ok {
                return Err(mlua::Error::runtime("Wrong object type for member function"));
            }
            let depths: Table = lua.named_registry_value(REG_IGNORE_DEPTH)?;
            depths.raw_set(id, ignore.unwrap_or(false))?;
            Ok(())
        })?,
    )?;
    Ok(())
}
