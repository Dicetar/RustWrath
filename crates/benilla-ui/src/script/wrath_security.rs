//! The 3.3.5a FrameScript security natives the stock FrameXML calls at load: `issecure` and
//! `securecall`. The 1.12 stdlib has neither; the real Wrath client builds both into the frame
//! script interpreter itself. `securecall` takes a function or a `_G` function name —
//! `UIDropDownMenu.lua:64` calls `securecall("UIDropDownMenu_InitializeHelper", frame)` because
//! the helper is defined later in the same file — and repasses the call's results. `issecure`
//! reads the existing execution-context owner; its stack-based taint limitations still apply
//! (see [`super::taint`]). This is not a complete Wrath secure-execution implementation.
//! The 1.12 host VM must never see these globals, so the Wrath block alone sets them.

use mlua::{Function, Lua, MultiValue, Value};

pub(super) fn install(lua: &Lua) -> mlua::Result<()> {
    // `scrub(...)` returns its arguments unchanged in the 12340 restricted-execution bootstrap;
    // stock `RestrictedExecution.lua`/`SecureHandlers.lua` capture it at load and fail without it.
    lua.globals().set(
        "scrub",
        lua.create_function(|_, args: MultiValue| Ok(args))?,
    )?;
    // `GetFrameHandle(frame, create)`/`IsFrameHandle(value)`: the stock `SecureHandlers.lua`
    // and `RestrictedExecution.lua` use the handle as the restricted closure's `self`. Frames
    // here are Lua tables, so the table itself is the handle; `IsFrameHandle` accepts exactly a
    // live frame wrapper.
    lua.globals().set(
        "GetFrameHandle",
        lua.create_function(|_lua, (frame, _create): (Value, Value)| {
            let Value::Table(table) = frame else {
                return Ok(Value::Nil);
            };
            // A frame wrapper carries its identity in `rawget(table, 0)` (a lightuserdata).
            // Accepting the wrapper itself is the handle the stock restricted-execution Lua
            // expects; a plain table is not a frame handle.
            let marked = table
                .raw_get::<Value>(0)
                .is_ok_and(|v| matches!(v, Value::LightUserData(_)));
            Ok(if marked {
                Value::Table(table)
            } else {
                Value::Nil
            })
        })?,
    )?;
    lua.globals().set(
        "IsFrameHandle",
        lua.create_function(|_lua, value: Value| {
            let Value::Table(table) = value else {
                return Ok(false);
            };
            Ok(table
                .raw_get::<Value>(0)
                .is_ok_and(|v| matches!(v, Value::LightUserData(_))))
        })?,
    )?;
    // `forceinsecure()`: the host never enforces taint, so the call is a no-op.
    lua.globals()
        .set("forceinsecure", lua.create_function(|_, ()| Ok(()))?)?;
    // `securecallfunction(f, ...)`: the function-value form of securecall.
    lua.globals().set(
        "securecallfunction",
        lua.create_function(|_, (f, args): (Function, MultiValue)| f.call::<MultiValue>(args))?,
    )?;

    let issecure = lua.create_function(|lua, _args: ()| {
        Ok(matches!(
            super::taint::current(lua),
            super::taint::Context::Client
        ))
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

#[cfg(test)]
mod tests {
    use crate::script::{UiScript, UiTarget};

    /// The stock `SecureHandlers.lua` uses the frame wrapper as the restricted closure's
    /// `self`; the handle binding must accept exactly a live frame table.
    #[test]
    fn get_frame_handle_accepts_a_wrath_frame_and_rejects_a_plain_table() {
        let s = UiScript::with_target(UiTarget::Wrath335).unwrap();
        let result: (bool, bool, bool) = s
            .eval(
                "local f = CreateFrame('Frame')
                 local h = GetFrameHandle(f)
                 return h ~= nil, IsFrameHandle(h), IsFrameHandle({})",
            )
            .unwrap();
        assert_eq!(result, (true, true, false));
    }
}
