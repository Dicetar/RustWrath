//! The Wrath sandbox's `newproxy`: the real 3.3.5a FrameScript carries 5.1's full stdlib, so the
//! restricted-frames engine `FrameXML` ships (`RestrictedFrames.lua:39`) binds it. The 1.12 stdlib
//! drops it, so the base library never registers it and the 1.12 VM stays without the global.

use mlua::benilla_lua::newproxy;

use crate::mlua::Lua;

pub(super) fn install(lua: &Lua) -> mlua::Result<()> {
    lua.globals().set("newproxy", lua.create_native("newproxy", newproxy)?)
}
