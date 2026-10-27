//! Handler firing: the FrameScript calling convention for events, ticks and show/hide. The
//! reference sets `this` (`0x872e64`) and `arg1..argN` (`0x8722dc`) around each handler's call
//! (`0x704f10`), restoring each after, and `pcall`s with no arguments; so does
//! [`invoke_with_globals`], which sets only the args it is given, so a script fired with none
//! leaves `arg1..argN` as found. `event` (`0x84b648`) is the walk's: set once before the first
//! listener and restored after the last (`0x703ea4`, `0x703f04`), by [`super::tick::fire_event_into`].
//! Handler errors return to the caller, which records them in [`super::Model::errors`].

use std::borrow::Cow;

use mlua::{Function, Lua, MultiValue, Table, Value};

use super::{Model, ScriptValue, REG_SCRIPTS};
use crate::script::object::frame_wrapper;

const REG_SCRIPT_HOOKS: &str = "__benilla_script_hooks";

pub(super) fn install_wrath(lua: &Lua) -> mlua::Result<()> {
    lua.set_named_registry_value(REG_SCRIPT_HOOKS, lua.create_table()?)?;
    // Installed IgnoreMore consumes frame varargs. The independent Wowless events module
    // confirms this unpacked view; use the host's existing ordered registration owner.
    lua.globals().set("GetFramesRegisteredForEvent", lua.create_function(
        |lua, name: String| {
            let ids: Vec<u32> = {
                let model = lua.app_data_ref::<Model>().expect("model");
                model.target.event_key(model.vm, &name)
                    .and_then(|event| model.event_to_frames.get(event))
                    .into_iter().flatten()
                    .filter(|handle| model.arena.frame(**handle).is_some())
                    .filter_map(|handle| model.frame_to_id.get(handle).copied())
                    .collect()
            };
            // Wrapper lookup can reach Lua globals/metamethods; retain no Model borrow.
            let frames = ids.into_iter().map(|id| frame_wrapper(lua, id).map(Value::Table))
                .collect::<mlua::Result<Vec<_>>>()?;
            Ok(MultiValue::from_vec(frames))
        },
    )?)
}

fn script_hooks(lua: &Lua, id: u32, script: &str) -> mlua::Result<Vec<Function>> {
    let Some(hooks) = lua.named_registry_value::<Option<Table>>(REG_SCRIPT_HOOKS)? else {
        return Ok(Vec::new());
    };
    let Some(per) = hooks.raw_get::<Option<Table>>(id)? else { return Ok(Vec::new()) };
    let Some(list) = per.raw_get::<Option<Table>>(script)? else { return Ok(Vec::new()) };
    list.sequence_values::<Function>().collect()
}

pub(crate) fn has_script_hooks(lua: &Lua, id: u32, script: &str) -> bool {
    let read = || -> mlua::Result<bool> {
        let Some(hooks) = lua.named_registry_value::<Option<Table>>(REG_SCRIPT_HOOKS)? else { return Ok(false) };
        let Some(per) = hooks.raw_get::<Option<Table>>(id)? else { return Ok(false) };
        Ok(per.raw_get::<Option<Table>>(script)?.is_some_and(|list| list.raw_len() > 0))
    };
    read().unwrap_or(false)
}

pub(crate) fn append_script_hook(lua: &Lua, id: u32, script: &str, hook: Function) -> mlua::Result<()> {
    let hooks: Table = lua.named_registry_value(REG_SCRIPT_HOOKS)?;
    let per = match hooks.raw_get::<Option<Table>>(id)? {
        Some(per) => per,
        None => {
            let per = lua.create_table()?;
            hooks.raw_set(id, per.clone())?;
            per
        }
    };
    let list = match per.raw_get::<Option<Table>>(script)? {
        Some(list) => list,
        None => {
            let list = lua.create_table()?;
            per.raw_set(script, list.clone())?;
            list
        }
    };
    list.raw_set(list.raw_len() + 1, hook)
}

/// `UiScript::fire_event` for engine code that holds only the Lua context.
pub(super) fn fire_global(lua: &Lua, event: &str, args: &[ScriptValue]) {
    super::tick::fire_event_into(lua, event, args.to_vec());
}

/// Fire a frame's `OnEvent` with the given args; the walk has set `event`.
pub(super) fn fire_event_handler(lua: &Lua, id: u32, event: &str, args: &[ScriptValue]) -> mlua::Result<()> {
    let mut extra: Vec<Value> = args
        .iter()
        .cloned()
        .map(|a| a.into_lua(lua))
        .collect::<mlua::Result<_>>()?;
    if lua.app_data_ref::<Model>().expect("model").target == super::UiTarget::Wrath335 {
        extra.insert(0, Value::String(lua.create_string(event)?));
    }
    fire(lua, id, "OnEvent", extra)
}

/// Fire a frame's `OnUpdate` with `arg1 = elapsed`.
pub(super) fn fire_update_handler(lua: &Lua, id: u32, elapsed: f32) -> mlua::Result<()> {
    fire(lua, id, "OnUpdate", vec![Value::Number(f64::from(elapsed))])
}

/// Fire a handler that carries no `event` name (`OnEnter`, `OnClick`, `OnValueChanged`, ...), with
/// `extra` as its `arg1..argN`; a no-op when the frame has no such script.
pub(super) fn fire_widget_handler(
    lua: &Lua,
    id: u32,
    script: &str,
    extra: Vec<Value>,
) -> mlua::Result<()> {
    fire(lua, id, script, extra)
}

/// Drain [`Model::pending_size_changed`], queued by the resolve pass, and fire `OnSizeChanged` for
/// each, recording errors. It drains once: a size a handler changes fires on the next resolve, as
/// the reference's `ApplyRect` does, so a handler that grows its own frame cannot spin forever.
pub(super) fn fire_size_changes(lua: &Lua) {
    let pending = std::mem::take(
        &mut lua
            .app_data_mut::<Model>()
            .expect("model")
            .pending_size_changed,
    );
    for (id, w, h) in pending {
        fire_size_changed(lua, id, w, h);
    }
}

/// `OnSizeChanged(width, height)` on frame `id`, its error recorded.
pub(super) fn fire_size_changed(lua: &Lua, id: u32, w: f32, h: f32) {
    if let Err(e) = fire(
        lua,
        id,
        "OnSizeChanged",
        vec![Value::Number(f64::from(w)), Value::Number(f64::from(h))],
    ) {
        lua.app_data_mut::<Model>()
            .expect("model")
            .record_lua_error(&e);
    }
}

/// Whether a handler is bound under `script`, without firing it: the keyboard walk consumes on
/// the slot's presence (`0x76b7d0`). A failed lookup reads absent, so the gate never raises.
pub(super) fn has_widget_handler(lua: &Lua, id: u32, script: &str) -> bool {
    let Ok(scripts) = lua.named_registry_value::<Table>(REG_SCRIPTS) else {
        return false;
    };
    let primary = match scripts.get::<Value>(id) {
        Ok(Value::Table(per)) => matches!(per.get::<Value>(script), Ok(Value::Function(_))),
        _ => false,
    };
    primary || has_script_hooks(lua, id, script)
}

/// The one firing path: `extra` is the `arg1..argN`. Holds no model borrow across the call. An
/// engine step that cannot lend the host holds the handler for it ([`super::host::holding`]).
fn fire(lua: &Lua, id: u32, script: &str, extra: Vec<Value>) -> mlua::Result<()> {
    let scripts: Table = lua.named_registry_value(REG_SCRIPTS)?;
    let func: Option<Function> = match scripts.get::<Value>(id)? {
        Value::Table(per) => match per.get::<Value>(script)? {
            Value::Function(f) => Some(f),
            _ => None,
        },
        _ => None,
    };
    let hooks = script_hooks(lua, id, script)?;
    if func.is_none() && hooks.is_empty() { return Ok(()); }
    if super::host::holding(lua) {
        let script = script.to_string();
        super::calls::hold(lua, super::calls::Held::Handler { id, script, extra });
        return Ok(());
    }

    let wrapper = frame_wrapper(lua, id)?;
    // The profiler's fire opens after `frame_wrapper`, where no model borrow is held, and closes
    // when the guard drops, unwinds included.
    let _fire =
        super::handler_prof::armed().then(|| super::handler_prof::Fire::open(lua, id, script));
    invoke_with_hooks(lua, wrapper, func.as_ref(), hooks, extra, script == "OnEvent")
}

pub(crate) fn invoke_onload(lua: &Lua, wrapper: Table, func: &Function) -> mlua::Result<()> {
    let id = super::object::decode_id(&wrapper)?;
    let hooks = script_hooks(lua, id, "OnLoad")?;
    invoke_with_hooks(lua, wrapper, Some(func), hooks, Vec::new(), false)
}

fn invoke_with_hooks(
    lua: &Lua,
    wrapper: Table,
    func: Option<&Function>,
    hooks: Vec<Function>,
    extra: Vec<Value>,
    event_payload: bool,
) -> mlua::Result<()> {
    if hooks.is_empty() {
        return match func {
            Some(func) => invoke_with_globals(lua, wrapper, func, extra, event_payload),
            None => Ok(()),
        };
    }
    let result = match func {
        Some(func) => invoke_with_globals(lua, wrapper.clone(), func, extra.clone(), event_payload),
        None => Ok(()),
    };
    for hook in hooks {
        if let Err(error) = invoke_with_globals(lua, wrapper.clone(), &hook, extra.clone(), event_payload) {
            lua.app_data_mut::<Model>().expect("model").record_lua_error(&error);
        }
    }
    result
}

/// The `argN` global names, spelled out so the common arities allocate nothing.
const ARG_NAMES: [&str; 16] = [
    "arg1", "arg2", "arg3", "arg4", "arg5", "arg6", "arg7", "arg8", "arg9", "arg10", "arg11",
    "arg12", "arg13", "arg14", "arg15", "arg16",
];

/// The global name for arg `i`, 1-based like the globals.
fn arg_name(i: usize) -> Cow<'static, str> {
    match ARG_NAMES.get(i - 1) {
        Some(&name) => Cow::Borrowed(name),
        None => Cow::Owned(format!("arg{i}")),
    }
}

/// The calling convention, shared by [`fire`] and the loader's `OnLoad`: sets `this` and
/// `arg1..argN`. Vanilla passes no arguments; Wrath also passes self and the explicit payload.
/// OnEvent's event name is an explicit argument, not legacy arg1. Globals restore on errors too.
pub(crate) fn invoke_with_globals(
    lua: &Lua,
    wrapper: Table,
    func: &Function,
    extra: Vec<Value>,
    event_payload: bool,
) -> mlua::Result<()> {
    let wrath = lua.app_data_ref::<Model>().expect("model").target == super::UiTarget::Wrath335;
    let arguments = if wrath {
        let mut arguments = Vec::with_capacity(extra.len() + 1);
        arguments.push(Value::Table(wrapper.clone()));
        arguments.extend(extra.iter().cloned());
        Some(mlua::MultiValue::from_vec(arguments))
    } else { None };
    let payload: Vec<Value> = if wrath && event_payload { extra.into_iter().skip(1).collect() } else { extra };
    let mut globals: Vec<(&str, Value)> = Vec::with_capacity(2 + payload.len());
    globals.push(("this", Value::Table(wrapper)));
    let names: Vec<Cow<'static, str>> = (1..=payload.len()).map(arg_name).collect();
    for (name, v) in names.iter().zip(payload) {
        globals.push((name.as_ref(), v));
    }
    // A protected call; the globals are put back before its outcome returns.
    match arguments {
        Some(arguments) => lua.call_with_globals_args(func, &globals, arguments),
        None => lua.call_with_globals(func, &globals),
    }
}
