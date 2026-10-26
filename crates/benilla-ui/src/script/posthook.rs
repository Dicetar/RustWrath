//! Wrath post-hooks. Functions and hook chains live in Lua closures, not Rust-held roots.
//! This does not implement Wrath's value-level taint or secure-frame execution machinery.

use mlua::{Function, Lua, MultiValue, Value, Variadic};

use super::{diagnostics::DiagnosticKind, model::HandlerArg, taint, Model};

const REG_FACTORY: &str = "__benilla_posthook_factory";
const USAGE: &str = "Usage: hooksecurefunc([table,] functionName, hookFunction)";

pub(super) fn install(lua: &Lua) -> mlua::Result<()> {
    let run_hook = lua.create_function(|lua, (hook, args): (Function, Variadic<Value>)| {
        if let Err(error) = hook.call::<()>(args) {
            let message = format!("hooksecurefunc: {}", super::lua_message(&error));
            let mut model = lua.app_data_mut::<Model>().expect("model");
            model.diagnostics.record(DiagnosticKind::Error, &message);
            model.errors.push(message);
            model.pending_error_dispatch.push(HandlerArg::Error(error));
        }
        Ok(())
    })?;
    let factory: Function = lua.load(FACTORY)
        .set_name("=[benilla posthook factory]")
        .set_mode(mlua::ChunkMode::Text)
        .call(run_hook)?;
    lua.set_named_registry_value(REG_FACTORY, factory)?;
    lua.globals().set("hooksecurefunc", lua.create_function(register)?)
}

fn register(lua: &Lua, args: MultiValue) -> mlua::Result<()> {
    let invalid = || mlua::Error::RuntimeError(USAGE.into());
    let (table, name, hook) = match args.front() {
        Some(Value::String(name)) => {
            let Some(Value::Function(hook)) = args.get(1) else { return Err(invalid()) };
            (lua.globals(), name.clone(), hook.clone())
        }
        Some(Value::Table(table)) => {
            let (Some(Value::String(name)), Some(Value::Function(hook))) = (args.get(1), args.get(2))
                else { return Err(invalid()) };
            (table.clone(), name.clone(), hook.clone())
        }
        _ => return Err(invalid()),
    };
    let original: Value = table.get(name.clone())?;
    let Value::Function(original) = original else { return Err(invalid()) };
    let factory: Function = lua.named_registry_value(REG_FACTORY)?;
    let (wrapper, invoke): (Function, Function) = factory.call((original, hook))?;
    // Attribute the post-hook invocation only. Registering an insecure hook must not stamp the
    // original function or the wrapper that invokes it with the registering addon's context.
    taint::tag(lua, &invoke, &taint::current(lua));
    table.set(name, wrapper)
}

// Cache the primitive functions before addons can replace them. Explicit n retains trailing
// nils and distinguishes zero results from one nil. invoke deliberately does not tail-call:
// its registered execution context must remain on the stack while the hook runs.
const FACTORY: &str = r#"
local run_hook = ...
local select, unpack = select, unpack
local function pack(...) return { n = select('#', ...), ... } end
return function(original, hook)
    local function invoke(...) run_hook(hook, ...) end
    return function(...)
        local results = pack(original(...))
        invoke(...)
        return unpack(results, 1, results.n)
    end, invoke
end
"#;
