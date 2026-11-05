//! The minimum 3.3.5a `SecureHandler*` surface installed addons need to load. The host has no
//! secure execution sandbox: it never evaluates a state driver or a wrapped script from Lua.
//!
//! This is deliberately a storage/handshake layer, not secure-handler compatibility:
//! - `SecureHandlerStateTemplate`/`SecureHandlerClickTemplate` are registered as native empty
//!   templates so `CreateFrame` resolves them and the frame loads.
//! - `RegisterStateDriver` validates and stores a bounded `(state, condition)` row. A future
//!   controller tick is the only intended consumer; Lua cannot make it run.
//! - `WrapScript` validates and stores bounded before/after bodies. They are never evaluated.
//! - `hasSecureHandler` is a true field on a frame created with one of those templates, matching
//!   the client's marker without claiming the frame is protected from taint.
//!
//! A real implementation needs the secure execution context, taint propagation through wrapped
//! handlers and a controller tick that evaluates the macro condition. None of those exist here,
//! so callers must not treat the stored rows as enforcement.

use std::collections::HashMap;

use mlua::{Lua, Table, Value};

use super::{Model, REG_FRAME_METHODS};
use crate::widget::FrameHandle;

/// The native templates this host recognises. `SecureHandlerBaseTemplate` is registered too so a
/// future list-inheriting template can name it without a lookup failure.
const SECURE_TEMPLATES: &[&str] = &[
    "SecureHandlerBaseTemplate",
    "SecureHandlerClickTemplate",
    "SecureHandlerStateTemplate",
];

const MAX_STATE_LEN: usize = 64;
const MAX_CONDITION_LEN: usize = 1024;
const MAX_WRAP_LEN: usize = 4096;
const MAX_WRAPS_PER_OWNER: usize = 64;
const MAX_WRAPS: usize = 1024;

/// Whether a `CreateFrame` inherits string names a native secure-handler template. Wrath splits
/// `inherits` on commas, so any element counts.
pub(super) fn is_secure_template(template: &str) -> bool {
    template
        .split(',')
        .map(str::trim)
        .any(|name| SECURE_TEMPLATES.iter().any(|t| name.eq_ignore_ascii_case(t)))
}

fn key(h: FrameHandle) -> u64 {
    h.fingerprint_bits()
}

pub(super) fn install(lua: &Lua) -> mlua::Result<()> {
    {
        let model = lua.app_data_ref::<Model>().expect("model app_data");
        let mut templates = model.framexml_templates.borrow_mut();
        for name in SECURE_TEMPLATES {
            if !templates.contains(name) {
                templates.register(name, crate::framexml::Element::empty("Frame"), None);
            }
        }
    }

    let methods: Table = lua.named_registry_value(REG_FRAME_METHODS)?;
    methods.set("WrapScript", lua.create_function(wrap_script)?)?;

    lua.globals().set(
        "RegisterStateDriver",
        lua.create_function(|lua, (frame, state, condition): (Table, Value, Value)| {
            let owner = super::object::frame_handle_of(lua, &frame)?;
            let state = text_arg(lua, &state, "Usage: RegisterStateDriver(frame, state, condition)")?;
            validate_state(&state).map_err(mlua::Error::runtime)?;
            let condition = text_arg(
                lua,
                &condition,
                "Usage: RegisterStateDriver(frame, state, condition)",
            )?;
            validate_condition(&condition).map_err(mlua::Error::runtime)?;
            let mut model = lua.app_data_mut::<Model>().expect("model app_data");
            let rows = model.secure_drivers.entry(key(owner)).or_default();
            if rows.len() >= 16 && !rows.contains_key(&state) {
                return Err(mlua::Error::runtime(
                    "RegisterStateDriver: too many states on one frame",
                ));
            }
            rows.insert(state, condition);
            Ok(())
        })?,
    )?;
    lua.globals().set(
        "UnregisterStateDriver",
        lua.create_function(|lua, (frame, state): (Table, Value)| {
            let owner = super::object::frame_handle_of(lua, &frame)?;
            let state = text_arg(
                lua,
                &state,
                "Usage: UnregisterStateDriver(frame, state)",
            )?;
            let mut model = lua.app_data_mut::<Model>().expect("model app_data");
            if let Some(rows) = model.secure_drivers.get_mut(&key(owner)) {
                rows.remove(&state);
                if rows.is_empty() {
                    model.secure_drivers.remove(&key(owner));
                }
            }
            Ok(())
        })?,
    )?;
    Ok(())
}

/// The client stores secure handlers on a frame created with a secure template; here it is the
/// same marker plus the owner set `WrapScript` checks.
pub(super) fn mark_frame(model: &mut Model, h: FrameHandle) {
    model.secure_frames.insert(key(h));
}

/// `SecureHandler:WrapScript(frame, script, preBody [, postBody])`. The reference runs `preBody`
/// before and `postBody` after the wrapped script inside its secure context. This host has no
/// secure context, so the bodies are stored and never evaluated.
fn wrap_script(
    lua: &Lua,
    (this, target, script, before, after): (Table, Table, Value, Value, Option<Value>),
) -> mlua::Result<()> {
    const USAGE: &str =
        "Usage: SecureHandler:WrapScript(frame, scriptName, preBody [, postBody])";
    let owner = super::object::frame_handle_of(lua, &this)?;
    let target = super::object::frame_handle_of(lua, &target)?;
    let script = text_arg(lua, &script, USAGE)?;
    validate_script_name(&script).map_err(mlua::Error::runtime)?;
    let before = text_arg(lua, &before, USAGE)?;
    let after = match after {
        Some(value) => text_arg(lua, &value, USAGE)?,
        None => String::new(),
    };
    if before.len() > MAX_WRAP_LEN || after.len() > MAX_WRAP_LEN {
        return Err(mlua::Error::runtime(
            "WrapScript: a before/after body exceeds the 4096-byte limit",
        ));
    }
    if before.contains('\0') || after.contains('\0') {
        return Err(mlua::Error::runtime("WrapScript: body contains a NUL"));
    }
    let mut model = lua.app_data_mut::<Model>().expect("model app_data");
    if !model.secure_frames.contains(&key(owner)) {
        // Do not pretend a plain frame protects the wrapped handler.
        return Err(mlua::Error::runtime(
            "WrapScript: frame is not a secure handler in this host",
        ));
    }
    let total: usize = model.secure_wraps.values().map(Vec::len).sum();
    if total >= MAX_WRAPS {
        return Err(mlua::Error::runtime(
            "WrapScript: the stored wrapped-script budget is exhausted",
        ));
    }
    let rows = model.secure_wraps.entry(key(owner)).or_default();
    if rows.len() >= MAX_WRAPS_PER_OWNER {
        return Err(mlua::Error::runtime(
            "WrapScript: too many wrapped scripts on one frame",
        ));
    }
    rows.push(super::model::SecureWrap {
        target: key(target),
        script,
        before,
        after,
    });
    Ok(())
}

fn text_arg(lua: &Lua, value: &Value, usage: &'static str) -> mlua::Result<String> {
    let Some(text) = lua.coerce_string(value.clone())? else {
        return Err(mlua::Error::runtime(usage));
    };
    Ok(text.to_string_lossy())
}

fn validate_script_name(script: &str) -> Result<(), String> {
    if script.is_empty() || script.len() > MAX_STATE_LEN {
        return Err("WrapScript: script name is empty or too long".to_owned());
    }
    if script.contains(['\0', '\n', '\r']) {
        return Err("WrapScript: script name contains a control character".to_owned());
    }
    Ok(())
}

fn validate_state(state: &str) -> Result<(), String> {
    if state.is_empty() || state.len() > MAX_STATE_LEN {
        return Err("RegisterStateDriver: state is empty or too long".to_owned());
    }
    if !state
        .chars()
        .all(|c| c.is_ascii_alphanumeric() || matches!(c, '_' | '-'))
    {
        return Err("RegisterStateDriver: state must be [A-Za-z0-9_-]".to_owned());
    }
    Ok(())
}

/// A bounded syntax check, not a macro evaluator: brackets/braces balance, quotes close and no
/// control characters cross. A valid condition is stored for a future controller tick; it is
/// never evaluated here.
fn validate_condition(condition: &str) -> Result<(), String> {
    if condition.is_empty() || condition.len() > MAX_CONDITION_LEN {
        return Err("RegisterStateDriver: condition is empty or too long".to_owned());
    }
    let mut depth = 0i32;
    let mut quote = None;
    let mut escaped = false;
    for c in condition.chars() {
        if matches!(c, '\0' | '\n' | '\r') {
            return Err("RegisterStateDriver: condition contains a control character".to_owned());
        }
        if let Some(q) = quote {
            if escaped {
                escaped = false;
            } else if c == '\\' {
                escaped = true;
            } else if c == q {
                quote = None;
            }
            continue;
        }
        match c {
            '\'' | '"' => quote = Some(c),
            '[' | '{' => depth += 1,
            ']' | '}' => {
                depth -= 1;
                if depth < 0 {
                    return Err("RegisterStateDriver: unbalanced condition bracket".to_owned());
                }
            }
            _ => {}
        }
    }
    if quote.is_some() || depth != 0 {
        return Err("RegisterStateDriver: unbalanced condition".to_owned());
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::script::{UiScript, UiTarget};

    fn script() -> UiScript {
        let mut script = UiScript::with_target(UiTarget::Wrath335).unwrap();
        script
            .run(r#"
                Bar = CreateFrame("Frame", "SecureHandlerTestBar", nil, "SecureHandlerStateTemplate")
                Plain = CreateFrame("Frame", "SecureHandlerTestPlain")
            "#)
            .unwrap();
        script
    }

    #[test]
    fn secure_templates_resolve_and_mark_the_frame() {
        let script = script();
        assert!(script
            .eval::<bool>("return Bar.hasSecureHandler == true")
            .unwrap());
        assert!(script
            .eval::<bool>("return Plain.hasSecureHandler == nil")
            .unwrap());
        // ClickTemplate is native too, even though this installed addon does not use it.
        script
            .run(r#"local b = CreateFrame("Button", "SecureHandlerTestButton", nil, "SecureHandlerClickTemplate"); assert(b.hasSecureHandler)"#)
            .unwrap();
    }

    #[test]
    fn state_drivers_are_validated_stored_and_removed_without_running() {
        let mut script = script();
        script
            .run(r#"
                Exploded = nil
                RegisterStateDriver(Bar, "page", "[bonusbar:1]1;0")
                RegisterStateDriver(Bar, "assist-help", "[help]nil; [target=targettarget, help]targettarget; nil")
            "#)
            .unwrap();
        {
            let model = script.model_ref();
            let rows = model.secure_drivers.values().next().expect("one frame's rows");
            assert_eq!(rows.get("page").map(String::as_str), Some("[bonusbar:1]1;0"));
            assert_eq!(
                rows.get("assist-help").map(String::as_str),
                Some("[help]nil; [target=targettarget, help]targettarget; nil")
            );
            assert!(model.secure_wraps.is_empty());
        }
        assert!(script
            .eval::<bool>("return Exploded == nil")
            .unwrap());
        script
            .run(r#"UnregisterStateDriver(Bar, "page")"#)
            .unwrap();
        {
            let model = script.model_ref();
            let rows = model.secure_drivers.values().next().expect("the other row remains");
            assert!(!rows.contains_key("page"));
            assert!(rows.contains_key("assist-help"));
        }
        // Bad states and unbalanced conditions are refused before anything is stored.
        assert!(script
            .run(r#"RegisterStateDriver(Bar, "bad state", "0")"#)
            .is_err());
        assert!(script
            .run(r#"RegisterStateDriver(Bar, "page", "[bonusbar:1")"#)
            .is_err());
    }

    #[test]
    fn wrap_script_stores_bounded_bodies_and_never_executes_them() {
        let mut script = script();
        script
            .run(r#"
                Executed = nil
                Bar:WrapScript(Bar, "OnShow", [[Executed = "before"]], [[Executed = "after"]])
            "#)
            .unwrap();
        {
            let model = script.model_ref();
            let wraps: Vec<_> = model.secure_wraps.values().flatten().collect();
            assert_eq!(wraps.len(), 1);
            assert_eq!(wraps[0].script, "OnShow");
            assert_eq!(wraps[0].before, r#"Executed = "before""#);
            assert_eq!(wraps[0].after, r#"Executed = "after""#);
        }
        assert!(
            script
                .eval::<bool>("return Executed == nil")
                .unwrap(),
            "storing a body must never run it"
        );
        // A plain frame is refused: no fake secure owner, no stored body.
        assert!(script
            .run(r#"Plain:WrapScript(Plain, "OnShow", "Executed = true")"#)
            .is_err());
        assert!(script
            .eval::<bool>("return Executed == nil")
            .unwrap());
        // Oversized bodies are refused at admission.
        let oversized = "x".repeat(MAX_WRAP_LEN + 1);
        let chunk = format!(
            r#"Bar:WrapScript(Bar, "OnShow", [=[{oversized}]=])"#
        );
        assert!(script.run(&chunk).is_err());
    }
}
