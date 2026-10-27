//! `RegisterStaticConstants`, the 3.3.5a native the GM-ticket registrar batch exposes
//! (ClassicAPI's walk of the 3.3.5a registration tables lists it beside `GetGMTicket`). The
//! stock pattern `STATIC_CONSTANTS = {}` / `RegisterStaticConstants(STATIC_CONSTANTS)` passes it
//! an empty table; Blizzard's own exported API stub keeps the call empty, and nothing in this
//! install's FrameXML reads the table back, so the faithful presentation is the same silence.

use mlua::{Lua, MultiValue};

pub(super) fn install(lua: &Lua) -> mlua::Result<()> {
    lua.globals().set(
        "RegisterStaticConstants",
        lua.create_function(|_, _args: MultiValue| Ok(()))?,
    )
}
