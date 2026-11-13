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

use mlua::{Lua, ObjectLike, Table, Value};

use super::{Model, REG_FRAME_METHODS};
use crate::widget::FrameHandle;

/// The native secure-handler templates this host recognises and marks with `hasSecureHandler`.
const SECURE_TEMPLATES: &[&str] = &[
    "SecureHandlerBaseTemplate",
    "SecureHandlerClickTemplate",
    "SecureHandlerStateTemplate",
    "SecureHandlerAttributeTemplate",
    "SecureHandlerEnterLeaveTemplate",
    "SecureHandlerShowHideTemplate",
    "SecureHandlerMouseUpDownTemplate",
    "SecureHandlerDragTemplate",
];

/// Fallback button/header templates registered under a non-client context (`Some(...)`) so bare
/// VMs and unit tests resolve `CreateFrame(..., "SecureActionButtonTemplate")`, while loading
/// real `SecureTemplates.xml` / `SecureHandlerTemplates.xml` in client context (`None`) replaces
/// them cleanly (`Registration::Replaced`).
const FALLBACK_TEMPLATES: &[(&str, &str)] = &[
    ("SecureActionButtonTemplate", "CheckButton"),
    ("SecureUnitButtonTemplate", "Button"),
    ("ActionButtonTemplate", "CheckButton"),
    ("SecureAuraHeaderTemplate", "Frame"),
    ("SecureGroupHeaderTemplate", "Frame"),
    ("SecurePartyHeaderTemplate", "Frame"),
    ("SecureRaidGroupHeaderTemplate", "Frame"),
];

const FALLBACK_CONTEXT: Option<&str> = Some("__benilla_secure_fallback");

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
                let mut el = crate::framexml::Element::empty("Frame");
                el.attrs.push(("protected".to_string(), "true".to_string()));
                templates.register(name, el, FALLBACK_CONTEXT);
            }
        }
        for &(name, tag) in FALLBACK_TEMPLATES {
            if !templates.contains(name) {
                let mut el = crate::framexml::Element::empty(tag);
                el.attrs.push(("protected".to_string(), "true".to_string()));
                templates.register(name, el, FALLBACK_CONTEXT);
            }
        }
    }

    let methods: Table = lua.named_registry_value(REG_FRAME_METHODS)?;
    methods.set("WrapScript", lua.create_function(wrap_script)?)?;
    methods.set("UnwrapScript", lua.create_function(unwrap_script)?)?;
    methods.set("Execute", lua.create_function(execute_body)?)?;
    methods.set("SetFrameRef", lua.create_function(set_frame_ref)?)?;
    methods.set("GetFrameRef", lua.create_function(get_frame_ref)?)?;

    lua.globals().set(
        "SecureHandlerWrapScript",
        lua.create_function(
            |lua, (frame, script, header, pre, post): (Table, Value, Table, Value, Option<Value>)| {
                wrap_script(lua, (header, frame, script, pre, post))
            },
        )?,
    )?;
    lua.globals().set(
        "SecureHandlerUnwrapScript",
        lua.create_function(|lua, (frame, script): (Table, Value)| {
            unwrap_script(lua, (frame.clone(), frame, script))
        })?,
    )?;
    lua.globals().set(
        "SecureHandlerExecute",
        lua.create_function(|lua, (frame, body): (Table, Value)| {
            execute_body(lua, (frame, body))
        })?,
    )?;
    lua.globals().set(
        "SecureHandlerSetFrameRef",
        lua.create_function(|lua, (frame, label, ref_frame): (Table, Value, Table)| {
            set_frame_ref(lua, (frame, label, ref_frame))
        })?,
    )?;
    lua.globals().set(
        "SecureCmdOptionParse",
        lua.create_function(|lua, text: Value| {
            let Some(s) = lua.coerce_string(text)? else {
                return Ok((Value::Nil, Value::Nil));
            };
            let raw = s.to_string_lossy();
            let (result, target) = eval_secure_cmd_options(lua, &raw);
            let res_val = match result {
                Some(body) => Value::String(lua.create_string(&body)?),
                None => Value::Nil,
            };
            let tgt_val = match target {
                Some(t) => Value::String(lua.create_string(&t)?),
                None => Value::Nil,
            };
            Ok((res_val, tgt_val))
        })?,
    )?;

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
            {
                let mut model = lua.app_data_mut::<Model>().expect("model app_data");
                let rows = model.secure_drivers.entry(key(owner)).or_default();
                if rows.len() >= 16 && !rows.contains_key(&state) {
                    return Err(mlua::Error::runtime(
                        "RegisterStateDriver: too many states on one frame",
                    ));
                }
                rows.insert(state.clone(), condition.clone());
            }
            apply_state_driver_value(lua, &frame, &state, &condition)?;
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

/// Re-evaluates all registered state drivers (`RegisterStateDriver`) against the current UI and
/// gameplay state, updating `state-<name>` attributes and `visibility` (`Show`/`Hide`) whenever a
/// condition's resolved value changes.
pub(super) fn tick_state_drivers(lua: &Lua) {
    let entries: Vec<(u32, Vec<(String, String)>)> = {
        let Some(model) = lua.app_data_ref::<Model>() else {
            return;
        };
        if model.secure_drivers.is_empty() {
            return;
        }
        model
            .secure_drivers
            .iter()
            .filter_map(|(&bits, rows)| {
                let handle = FrameHandle::from_fingerprint_bits(bits);
                let id = model.frame_to_id.get(&handle).copied()?;
                let pairs = rows
                    .iter()
                    .map(|(s, c)| (s.clone(), c.clone()))
                    .collect::<Vec<_>>();
                Some((id, pairs))
            })
            .collect()
    };
    for (id, pairs) in entries {
        let Ok(frame) = super::object::frame_wrapper(lua, id) else {
            continue;
        };
        for (state, condition) in pairs {
            let _ = apply_state_driver_value(lua, &frame, &state, &condition);
        }
    }
}

fn apply_state_driver_value(
    lua: &Lua,
    frame: &Table,
    state: &str,
    condition: &str,
) -> mlua::Result<()> {
    let (resolved, _) = eval_secure_cmd_options(lua, condition);
    let attr_name = format!("state-{state}");
    let current = frame
        .call_method::<Value>("GetAttribute", attr_name.as_str())
        .ok()
        .and_then(|v| match v {
            Value::String(s) => s.to_str().ok().map(|s| s.to_owned()),
            _ => None,
        });
    match resolved.as_deref() {
        None | Some("nil") => {
            if current.is_some() {
                let _ = frame.call_method::<()>("SetAttribute", (attr_name, Value::Nil));
            }
        }
        Some(val) => {
            if current.as_deref() != Some(val) {
                let _ = frame.call_method::<()>("SetAttribute", (attr_name, val));
                if state.eq_ignore_ascii_case("visibility") {
                    if val.eq_ignore_ascii_case("show") {
                        let _ = frame.call_method::<()>("Show", ());
                    } else if val.eq_ignore_ascii_case("hide") {
                        let _ = frame.call_method::<()>("Hide", ());
                    }
                }
            }
        }
    }
    Ok(())
}

fn eval_secure_cmd_options(lua: &Lua, raw: &str) -> (Option<String>, Option<String>) {
    for clause in raw.split(';') {
        let trimmed = clause.trim();
        if !trimmed.starts_with('[') {
            return (Some(trimmed.to_string()), None);
        }
        let mut rest = trimmed;
        let mut groups = Vec::new();
        while let Some(after_open) = rest.strip_prefix('[') {
            let Some(close_idx) = after_open.find(']') else {
                break;
            };
            groups.push(&after_open[..close_idx]);
            rest = after_open[close_idx + 1..].trim_start();
        }
        let action = rest.trim();
        for group in groups {
            if let (true, target) = eval_condition_group(lua, group) {
                return (Some(action.to_string()), target);
            }
        }
    }
    (None, None)
}

fn eval_condition_group(lua: &Lua, group: &str) -> (bool, Option<String>) {
    let mut target: Option<String> = None;
    for raw_term in group.split(',') {
        let term = raw_term.trim();
        if term.is_empty() {
            continue;
        }
        if let Some(unit) = term.strip_prefix('@') {
            let unit = unit.trim();
            if !unit.is_empty() {
                target = Some(unit.to_string());
            }
            continue;
        }
        if let Some((k, v)) = term.split_once('=') {
            if k.trim().eq_ignore_ascii_case("target") {
                let unit = v.trim();
                if !unit.is_empty() {
                    target = Some(unit.to_string());
                }
                continue;
            }
        }
        let unit = target.as_deref().unwrap_or("target");
        if !eval_single_condition(lua, term, unit) {
            return (false, target);
        }
    }
    (true, target)
}

fn lua_bool_call<A: mlua::IntoLuaMulti>(lua: &Lua, func: &str, args: A) -> bool {
    let Ok(Value::Function(f)) = lua.globals().get::<Value>(func) else {
        return false;
    };
    match f.call::<Value>(args) {
        Ok(Value::Nil) | Ok(Value::Boolean(false)) => false,
        Ok(Value::Integer(0)) => false,
        Ok(Value::Number(n)) if n == 0.0 => false,
        Ok(_) => true,
        Err(_) => false,
    }
}

fn lua_i64_call<A: mlua::IntoLuaMulti>(lua: &Lua, func: &str, args: A) -> i64 {
    let Ok(Value::Function(f)) = lua.globals().get::<Value>(func) else {
        return 0;
    };
    f.call::<i64>(args).unwrap_or(0)
}

fn matches_int_arg(arg: Option<&str>, current: i64) -> bool {
    match arg {
        None | Some("") => current > 0,
        Some(list) => list
            .split('/')
            .map(str::trim)
            .filter_map(|s| s.parse::<i64>().ok())
            .any(|n| n == current),
    }
}

fn eval_single_condition(lua: &Lua, raw: &str, unit: &str) -> bool {
    let (name_part, arg_part) = match raw.split_once(':') {
        Some((k, v)) => (k.trim(), Some(v.trim())),
        None => (raw.trim(), None),
    };
    let lower = name_part.to_ascii_lowercase();
    let (negated, key) = match lower.strip_prefix("no") {
        Some(rest) if !rest.is_empty() => (true, rest),
        _ => (false, lower.as_str()),
    };
    let matched = match key {
        "bonusbar" => matches_int_arg(arg_part, lua_i64_call(lua, "GetBonusBarOffset", ())),
        "bar" | "actionbar" => matches_int_arg(arg_part, lua_i64_call(lua, "GetActionBarPage", ())),
        "form" | "stance" => matches_int_arg(arg_part, lua_i64_call(lua, "GetShapeshiftForm", ())),
        "spec" => matches_int_arg(arg_part, lua_i64_call(lua, "GetActiveTalentGroup", ())),
        "stealth" => lua_bool_call(lua, "IsStealthed", ()),
        "combat" => lua_bool_call(lua, "UnitAffectingCombat", "player"),
        "mounted" => lua_bool_call(lua, "IsMounted", ()),
        "swimming" => lua_bool_call(lua, "IsSwimming", ()),
        "flying" => lua_bool_call(lua, "IsFlying", ()),
        "flyable" => lua_bool_call(lua, "IsFlyableArea", ()),
        "indoors" => lua_bool_call(lua, "IsIndoors", ()),
        "outdoors" => lua_bool_call(lua, "IsOutdoors", ()),
        "vehicleui" => lua_bool_call(lua, "HasVehicleActionBar", ()),
        "possessbar" => lua_bool_call(lua, "IsPossessBarVisible", ()),
        "channeling" => lua_bool_call(lua, "UnitChannelInfo", "player"),
        "pet" => lua_bool_call(lua, "UnitExists", "pet"),
        "exists" => lua_bool_call(lua, "UnitExists", unit),
        "help" => lua_bool_call(lua, "UnitCanAssist", ("player", unit)),
        "harm" => lua_bool_call(lua, "UnitCanAttack", ("player", unit)),
        "dead" => lua_bool_call(lua, "UnitIsDeadOrGhost", unit),
        "group" => match arg_part.map(|s| s.to_ascii_lowercase()).as_deref() {
            Some("raid") => lua_i64_call(lua, "GetNumRaidMembers", ()) > 0,
            Some("party") => lua_i64_call(lua, "GetNumPartyMembers", ()) > 0,
            _ => {
                lua_i64_call(lua, "GetNumPartyMembers", ()) > 0
                    || lua_i64_call(lua, "GetNumRaidMembers", ()) > 0
            }
        },
        "mod" | "modifier" => match arg_part.map(|s| s.to_ascii_lowercase()).as_deref() {
            Some("shift") => lua_bool_call(lua, "IsShiftKeyDown", ()),
            Some("ctrl") | Some("control") => lua_bool_call(lua, "IsControlKeyDown", ()),
            Some("alt") => lua_bool_call(lua, "IsAltKeyDown", ()),
            _ => lua_bool_call(lua, "IsModifierKeyDown", ()),
        },
        _ => false,
    };
    if negated { !matched } else { matched }
}

/// The client stores secure handlers on a frame created with a secure template; here it is the
/// same marker plus the owner set `WrapScript` checks.
pub(super) fn mark_frame(model: &mut Model, h: FrameHandle) {
    model.secure_frames.insert(key(h));
}

fn unwrap_script(lua: &Lua, (this, target, script): (Table, Table, Value)) -> mlua::Result<()> {
    let owner = super::object::frame_handle_of(lua, &this)?;
    let target = super::object::frame_handle_of(lua, &target)?;
    let script = text_arg(
        lua,
        &script,
        "Usage: SecureHandler:UnwrapScript(frame, scriptName)",
    )?;
    let mut model = lua.app_data_mut::<Model>().expect("model app_data");
    if let Some(rows) = model.secure_wraps.get_mut(&key(owner)) {
        if let Some(pos) = rows
            .iter()
            .rposition(|w| w.target == key(target) && w.script.eq_ignore_ascii_case(&script))
        {
            rows.remove(pos);
        }
        if rows.is_empty() {
            model.secure_wraps.remove(&key(owner));
        }
    }
    Ok(())
}

fn execute_body(lua: &Lua, (this, body): (Table, Value)) -> mlua::Result<()> {
    let owner = super::object::frame_handle_of(lua, &this)?;
    let body = text_arg(lua, &body, "Usage: SecureHandler:Execute(body)")?;
    if body.len() > MAX_WRAP_LEN {
        return Err(mlua::Error::runtime(
            "Execute: body exceeds the 4096-byte limit",
        ));
    }
    if body.contains('\0') {
        return Err(mlua::Error::runtime("Execute: body contains a NUL"));
    }
    let model = lua.app_data_ref::<Model>().expect("model app_data");
    if !model.secure_frames.contains(&key(owner)) {
        return Err(mlua::Error::runtime(
            "Execute: frame is not a secure handler in this host",
        ));
    }
    Ok(())
}

fn set_frame_ref(
    lua: &Lua,
    (this, label, ref_frame): (Table, Value, Table),
) -> mlua::Result<()> {
    let _owner = super::object::frame_handle_of(lua, &this)?;
    let _ref_h = super::object::frame_handle_of(lua, &ref_frame)?;
    let label = text_arg(
        lua,
        &label,
        "Usage: SecureHandler:SetFrameRef(label, frame)",
    )?;
    validate_state(&label).map_err(mlua::Error::runtime)?;
    let attr_key = format!("_frameref-{}", label.to_ascii_lowercase());
    this.call_method::<()>("SetAttribute", (attr_key, ref_frame))
}

fn get_frame_ref(lua: &Lua, (this, label): (Table, Value)) -> mlua::Result<Value> {
    let _owner = super::object::frame_handle_of(lua, &this)?;
    let label = text_arg(lua, &label, "Usage: SecureHandler:GetFrameRef(label)")?;
    let attr_key = format!("_frameref-{}", label.to_ascii_lowercase());
    this.call_method::<Value>("GetAttribute", attr_key)
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

    #[test]
    fn bartender4_secure_state_driver_surface() {
        let mut script = script();
        script
            .run(
                r#"
                local btn = CreateFrame("CheckButton", "BT4Button1", Bar, "SecureActionButtonTemplate")
                Bar:WrapScript(btn, "OnClick", [[return "wrapped"]])
                Bar:SetFrameRef("action1", btn)
                assert(Bar:GetFrameRef("action1") == btn)
                Bar:UnwrapScript(btn, "OnClick")
                local cmd, unit = SecureCmdOptionParse("[nocombat, target=player] heal; harm")
                assert(cmd == "heal" and unit == "player")
                RegisterStateDriver(Bar, "page", "[bonusbar:1] 7; 1")
                assert(Bar:GetAttribute("state-page") == "1")
                Bar:Hide()
                RegisterStateDriver(Bar, "visibility", "[petbattle]hide;show")
                assert(Bar:IsShown() == 1)
                UnregisterStateDriver(Bar, "visibility")
                "#,
            )
            .unwrap();
        // The Wrath verb exists and answers nothing for an empty action slot (the reference's
        // `GetActionInfo` miss); the secure surface's own checks are above.
        assert_eq!(script.arity("GetActionInfo(1)").unwrap(), 0);
    }

    #[test]
    fn real_framexml_replaces_fallback_secure_templates_cleanly() {
        let script = script();
        let model = script.model_ref();
        let mut templates = model.framexml_templates.borrow_mut();
        assert_eq!(
            templates.register(
                "SecureActionButtonTemplate",
                crate::framexml::Element::empty("CheckButton"),
                None,
            ),
            crate::framexml::Registration::Replaced
        );
        assert_eq!(
            templates.register(
                "SecureHandlerStateTemplate",
                crate::framexml::Element::empty("Frame"),
                None,
            ),
            crate::framexml::Registration::Replaced
        );
    }

    #[test]
    fn is_protected_and_can_change_protected_state_follow_hierarchy_and_combat() {
        let mut script = script();
        script
            .run(
                r#"
                Child = CreateFrame("Frame", "SecureHandlerTestChild", Bar)
                local btn = CreateFrame("CheckButton", "SecureActionBtn", nil, "SecureActionButtonTemplate")
                local p_bar, e_bar = Bar:IsProtected()
                local p_child, e_child = Child:IsProtected()
                local p_plain, e_plain = Plain:IsProtected()
                local p_btn, e_btn = btn:IsProtected()
                assert(p_bar == true and e_bar == true, "secure template is explicitly protected")
                assert(p_child == true and e_child == false, "child inherits protection non-explicitly")
                assert(p_plain == false and e_plain == false, "plain frame is unprotected")
                assert(p_btn == true and e_btn == true, "SecureActionButtonTemplate is explicitly protected")
                assert(Bar:CanChangeProtectedState() == true)
                assert(Child:CanChangeProtectedState() == true)
                assert(Plain:CanChangeProtectedState() == true)
                "#,
            )
            .unwrap();

        let mut player = crate::script::UnitState::default();
        player.exists = true;
        player.has_object = true;
        player.in_combat = true;
        script.set_unit("player", Some(player.clone()));

        assert!(!script.eval::<bool>("return Bar:CanChangeProtectedState()").unwrap());
        assert!(!script.eval::<bool>("return Child:CanChangeProtectedState()").unwrap());
        assert!(script.eval::<bool>("return Plain:CanChangeProtectedState()").unwrap());

        player.in_combat = false;
        script.set_unit("player", Some(player));
        assert!(script.eval::<bool>("return Bar:CanChangeProtectedState()").unwrap());

        // XML `protected="true"` and inheriting from an explicit template both mark protected.
        let xml = r#"<Ui>
            <Frame name="XmlProtectedBase" virtual="true" protected="true"/>
            <Frame name="XmlProtectedFrame" inherits="XmlProtectedBase">
                <Frames>
                    <Frame name="XmlProtectedChild"/>
                </Frames>
            </Frame>
        </Ui>"#;
        let doc = crate::framexml::parse(xml).unwrap();
        let report = crate::loader::load(&script, &doc, &|_| None);
        assert!(report.errors.is_empty(), "{:?}", report.errors);
        script
            .run(
                r#"
                local p_xml, e_xml = XmlProtectedFrame:IsProtected()
                local p_xchild, e_xchild = XmlProtectedChild:IsProtected()
                assert(p_xml == true and e_xml == true)
                assert(p_xchild == true and e_xchild == false)

                Plain:SetParent(Bar)
                local p_rep, e_rep = Plain:IsProtected()
                assert(p_rep == true and e_rep == false)
                Plain:SetParent(nil)
                local p_un, e_un = Plain:IsProtected()
                assert(p_un == false and e_un == false)
                "#,
            )
            .unwrap();
    }
}
