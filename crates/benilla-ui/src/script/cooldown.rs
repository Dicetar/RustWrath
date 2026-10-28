//! The 3.3.5a `Cooldown` widget's Lua surface: `SetCooldown(start, duration)`, `SetReverse`,
//! `GetReverse`, `SetDrawEdge`, `GetDrawEdge`, the whole set the 12340 client binary registers
//! (`Usage: %s:SetCooldown(start, duration)` is its one refusal string; no countdown-font, color
//! or swipe/edge texture setters exist there).
//!
//! The stock `Cooldown.lua` is the only wrapper: `start > 0 and duration > 0 and enable > 0` calls
//! `self:SetCooldown(start, duration)` and shows the frame, else hides it. Every stock consumer and
//! every installed addon observed goes through it, so the widget owns no timer of its own: the
//! start and duration land on the frame-script `GetTime` clock, and the extract
//! ([`crate::script::extract`]) computes the sweep against `Model::now`, the same clock every
//! producer stamps.

use mlua::{Lua, Table, Value};

use super::binding_abi::{flag, flag_or_default, method_number};
use super::object::frame_handle_of;
use super::{Model, UiTarget};

use crate::widget::{CooldownState, KindState};

/// Registry key of the Cooldown method table, linked under the shared frame methods for the
/// Cooldown kind (`kind_method_registries`).
pub(super) const REG_COOLDOWN_METHODS: &str = "__benilla_cooldown_methods";

/// Run `f` over a frame's cooldown state; any other receiver raises, since the method table is a
/// plain Lua value a caller can misapply.
fn with_cooldown<T>(
    lua: &Lua,
    this: &Table,
    f: impl FnOnce(&mut CooldownState) -> T,
) -> mlua::Result<T> {
    let h = frame_handle_of(lua, this)?;
    let mut model = lua.app_data_mut::<Model>().expect("model app_data");
    let frame = model
        .arena
        .frame_mut(h)
        .ok_or_else(|| mlua::Error::runtime("stale frame handle"))?;
    match &mut frame.kind_state {
        KindState::Cooldown(cd) => Ok(f(cd)),
        _ => Err(mlua::Error::runtime("not a Cooldown")),
    }
}

/// The 12340 boolean setters publish no usage string, so their argument goes through the client's
/// truthiness coercion (`GetScriptFlagArg`, `0x6f1c10`): a missing argument is false.
fn truthy(value: Option<Value>) -> bool {
    flag_or_default(value.as_ref(), 0) != 0
}

/// Register the Cooldown methods. The 12340 widget exists only in the Wrath client, so the table
/// is built only for that target and the 1.12 surface gains no lookup.
pub(super) fn install(lua: &Lua) -> mlua::Result<()> {
    if lua.app_data_ref::<Model>().expect("model app_data").target != UiTarget::Wrath335 {
        return Ok(());
    }
    let m = lua.create_table()?;

    // `SetCooldown(start, duration)`: two `is-number`-gated arguments, each else the 12340 usage
    // line naming the receiver. It records the timer; `Cooldown.lua` shows and hides the frame
    // around it.
    m.set(
        "SetCooldown",
        lua.create_function(|lua, (this, start, duration): (Table, Value, Value)| {
            let start = method_number(lua, &this, start, "SetCooldown(start, duration)")?;
            let duration = method_number(lua, &this, duration, "SetCooldown(start, duration)")?;
            with_cooldown(lua, &this, |cd| {
                cd.start = start;
                cd.duration = duration;
            })
        })?,
    )?;

    // `SetReverse(value)`: the sweep runs counter-clockwise (`TargetFrame.xml:30`, `TotemFrame.xml:42`).
    m.set(
        "SetReverse",
        lua.create_function(|lua, (this, value): (Table, Option<Value>)| {
            with_cooldown(lua, &this, |cd| cd.reverse = truthy(value))
        })?,
    )?;
    // `GetReverse()`: 1 or nil.
    m.set(
        "GetReverse",
        lua.create_function(|lua, this: Table| with_cooldown(lua, &this, |cd| flag(cd.reverse)))?,
    )?;

    // `SetDrawEdge(value)`: draw the gold hand at the sweep boundary (`RuneFrame.xml:9`,
    // `TargetFrame.xml`'s three).
    m.set(
        "SetDrawEdge",
        lua.create_function(|lua, (this, value): (Table, Option<Value>)| {
            with_cooldown(lua, &this, |cd| cd.draw_edge = truthy(value))
        })?,
    )?;
    // `GetDrawEdge()`: 1 or nil.
    m.set(
        "GetDrawEdge",
        lua.create_function(|lua, this: Table| with_cooldown(lua, &this, |cd| flag(cd.draw_edge)))?,
    )?;

    lua.set_named_registry_value(REG_COOLDOWN_METHODS, m)?;
    Ok(())
}
