//! The 3.3.5a FrameScript security natives the stock FrameXML calls at load: `issecure` and
//! `securecall`. The 1.12 stdlib has neither; the real Wrath client builds both into the frame
//! script interpreter itself. `securecall` takes a function or a `_G` function name —
//! `UIDropDownMenu.lua:64` calls `securecall("UIDropDownMenu_InitializeHelper", frame)` because
//! the helper is defined later in the same file — and repasses the call's results. `issecure`
//! reads the existing execution-context owner; its stack-based taint limitations still apply
//! (see [`super::taint`]). This is not a complete Wrath secure-execution implementation.
//! The 1.12 host VM must never see these globals, so the Wrath block alone sets them.

use mlua::Lua;

pub(super) fn install(lua: &Lua) -> mlua::Result<()> {
    let issecure = lua.create_function(|lua, _args: ()| {
        Ok(matches!(super::taint::current(lua), super::taint::Context::Client))
    })?;
    lua.globals().set("issecure", issecure)?;

    lua.load(
        r#"
function securecall(f, ...)
    if type(f) == "string" then
        f = _G[f]
    end
    return f(...)
end
"#,
    )
    .set_name("benilla:wrath-secure-natives")
    .exec()?;
    Ok(())
}
