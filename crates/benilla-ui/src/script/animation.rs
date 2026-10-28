//! The 3.3.5a `<Animations>` object graph and its timeline: one table per `<AnimationGroup>`
//! and per animation inside it, named and parent-keyed like any other XML object, with the
//! frame-script verbs the stock UI calls on them at load (`TutorialFrame.lua:790` stops
//! `TutorialFrameCallOutPulser`, the `Alpha` inside `TutorialFrameCallOut`'s group).
//!
//! Stock XML declares no triggers; stock Lua calls `group:Play()` (often from `OnShow`). A group's
//! `Play` sequences its children by `order` (parallel within one order) and the tick advances
//! them on the frame-script clock. This tracer applies `Alpha` to a frame owner; the timeline,
//! `OnPlay`/`OnStop`/`OnFinished`, parenting and ordering are common to every kind, and the other
//! kinds (`Translation`, `Rotation`, ...) are recorded as the next slice.

use mlua::{Function, Lua, Table, Value};

use super::Model;
use crate::color::{alpha_f64, frame_alpha_byte};
use crate::widget::{FrameHandle, RegionHandle};

/// The registry of every built animation table, indexed for the tick. A named registry value, so
/// it dies with the VM and never leaks a global.
const REG_ANIMATIONS: &str = "__benilla_animations";

const ANIMATION_KIND: &str = "__benilla_animation_kind";
const ANIMATION_NAME: &str = "__benilla_animation_name";
const ANIMATION_OWNER: &str = "__benilla_animation_owner";
const ANIMATION_PARENT: &str = "__benilla_animation_parent";
const ANIMATION_CHILDREN: &str = "__benilla_animation_children";
const ANIMATION_PARAMS: &str = "__benilla_animation_params";
const ANIMATION_SCRIPTS: &str = "__benilla_animation_scripts";
const ANIMATION_PLAYING: &str = "__benilla_animation_playing";
const ANIMATION_FINISHED: &str = "__benilla_animation_finished";
const ANIMATION_PLAYED: &str = "__benilla_animation_played";
const ANIMATION_START: &str = "__benilla_animation_start";
const ANIMATION_FROM: &str = "__benilla_animation_from";

/// Create the tick's registry. Wrath only.
pub(super) fn install(lua: &Lua) -> mlua::Result<()> {
    if lua.app_data_ref::<Model>().expect("model app_data").target != super::UiTarget::Wrath335 {
        return Ok(());
    }
    lua.set_named_registry_value(REG_ANIMATIONS, lua.create_table()?)?;
    Ok(())
}

/// An animation object of `kind` (`AnimationGroup`, `Alpha`, `Translation`, ...), named for the
/// global namespace when the XML names it, its owner the frame or region whose `<Animations>`
/// block holds it, `parent` the group that holds a child animation.
pub(crate) fn build(
    lua: &Lua,
    kind: &str,
    name: Option<&str>,
    attrs: &[(String, String)],
    owner: &Table,
    parent: Option<&Table>,
) -> mlua::Result<Table> {
    let object = lua.create_table()?;
    object.set(ANIMATION_KIND, kind)?;
    object.set(ANIMATION_NAME, name)?;
    object.set(ANIMATION_OWNER, owner.clone())?;
    object.set(ANIMATION_PARENT, parent.cloned())?;
    object.set(ANIMATION_PLAYING, false)?;
    object.set(ANIMATION_FINISHED, false)?;
    object.set(ANIMATION_PLAYED, false)?;
    object.set(ANIMATION_START, 0.0)?;
    object.set(ANIMATION_FROM, 1.0)?;
    let params = lua.create_table()?;
    for (key, value) in attrs {
        params.set(key.clone(), value.clone())?;
    }
    object.set(ANIMATION_PARAMS, params)?;
    object.set(ANIMATION_SCRIPTS, lua.create_table()?)?;

    object.set(
        "Play",
        lua.create_function(|lua, this: Table| play(lua, &this))?,
    )?;
    object.set(
        "Stop",
        lua.create_function(|lua, this: Table| stop(lua, &this, true))?,
    )?;
    object.set(
        "IsPlaying",
        lua.create_function(|_, this: Table| this.get::<bool>(ANIMATION_PLAYING))?,
    )?;
    object.set(
        "GetName",
        lua.create_function(|_, this: Table| this.get::<Option<String>>(ANIMATION_NAME))?,
    )?;
    object.set(
        "GetRegionParent",
        lua.create_function(|_, this: Table| this.get::<Option<Table>>(ANIMATION_OWNER))?,
    )?;
    // `SetStartDelay(seconds)`: AlertFrames' alert Lua re-arms its fade-out animation this way
    // before `Play`; the value is read when the animation or its group starts.
    object.set(
        "SetStartDelay",
        lua.create_function(|lua, (this, value): (Table, Value)| {
            let seconds = super::binding_abi::coerced_number(lua, Some(value));
            let params: Table = this.get(ANIMATION_PARAMS)?;
            params.set("startDelay", seconds.to_string())?;
            Ok(())
        })?,
    )?;
    object.set(
        "SetScript",
        lua.create_function(|_, (this, name, func): (Table, String, Option<Value>)| {
            let scripts: Table = this.get(ANIMATION_SCRIPTS)?;
            scripts.set(name, func)?;
            Ok(())
        })?,
    )?;
    object.set(
        "GetScript",
        lua.create_function(|_, (this, name): (Table, String)| {
            let scripts: Table = this.get(ANIMATION_SCRIPTS)?;
            scripts.get::<Value>(name)
        })?,
    )?;

    let registry: Table = lua.named_registry_value(REG_ANIMATIONS)?;
    registry.raw_set(registry.raw_len() + 1, object.clone())?;
    Ok(object)
}

/// Link a group's child animations, in document order, for the group's `Play` cascade.
pub(crate) fn link_children(group: &Table, children: Table) -> mlua::Result<()> {
    group.set(ANIMATION_CHILDREN, children)
}

/// `Play`: start this animation (and, for a group, its children by `order`).
fn play(lua: &Lua, this: &Table) -> mlua::Result<()> {
    let now = lua.app_data_ref::<Model>().expect("model app_data").now;
    if is_group(this)? {
        start_group(lua, this, now)?;
        fire(lua, this, "OnPlay")
    } else {
        start_one(this, now)
    }
}

/// Start a group's children: one `order` batch after the previous, parallel within a batch.
fn start_group(lua: &Lua, group: &Table, now: f64) -> mlua::Result<()> {
    group.set(ANIMATION_PLAYING, true)?;
    group.set(ANIMATION_FINISHED, false)?;
    let Some(children) = group.get::<Option<Table>>(ANIMATION_CHILDREN)? else {
        return Ok(());
    };
    let mut orders: Vec<(u32, Vec<Table>)> = Vec::new();
    for child in children.sequence_values::<Table>() {
        let child = child?;
        let order = param_number(&child, "order", 0.0) as u32;
        match orders.iter_mut().find(|(o, _)| *o == order) {
            Some((_, list)) => list.push(child),
            None => orders.push((order, vec![child])),
        }
    }
    orders.sort_by_key(|(order, _)| *order);
    let mut batch_start = now;
    for (_, list) in orders {
        let mut batch_span: f64 = 0.0;
        for child in &list {
            let delay = param_number(child, "startDelay", 0.0).max(0.0);
            start_one(child, batch_start + delay)?;
            batch_span = batch_span.max(delay + param_number(child, "duration", 0.0).max(0.0));
        }
        batch_start += batch_span;
    }
    Ok(())
}

/// Schedule one animation to start at `at`; its base alpha is captured when it starts, not now,
/// so a later `order` batch fades from what the earlier batch left.
fn start_one(this: &Table, at: f64) -> mlua::Result<()> {
    this.set(ANIMATION_START, at)?;
    this.set(ANIMATION_PLAYING, true)?;
    this.set(ANIMATION_FINISHED, false)?;
    this.set(ANIMATION_PLAYED, false)?;
    Ok(())
}

/// `Stop`: end a run and fire `OnStop` (the reference's stop signal), recursively for a group.
fn stop(lua: &Lua, this: &Table, signal: bool) -> mlua::Result<()> {
    this.set(ANIMATION_PLAYING, false)?;
    if is_group(this)? {
        let Some(children) = this.get::<Option<Table>>(ANIMATION_CHILDREN)? else {
            return Ok(());
        };
        for child in children.sequence_values::<Table>() {
            stop(lua, &child?, false)?;
        }
    } else if signal && !this.get::<bool>(ANIMATION_FINISHED)? {
        fire(lua, this, "OnStop")?;
    }
    Ok(())
}

/// The tick's animation step: advance every playing animation on `Model::now`.
pub(super) fn update(lua: &Lua) {
    let now = lua.app_data_ref::<Model>().expect("model app_data").now;
    let registry: Table = match lua.named_registry_value(REG_ANIMATIONS) {
        Ok(table) => table,
        Err(_) => return,
    };
    let mut groups: Vec<Table> = Vec::new();
    for entry in registry.sequence_values::<Table>() {
        let Ok(this) = entry else { continue };
        let playing = this.get::<bool>(ANIMATION_PLAYING).unwrap_or(false);
        if !playing {
            continue;
        }
        if is_group(&this).unwrap_or(false) {
            groups.push(this);
            continue;
        }
        if let Err(error) = advance_one(lua, &this, now) {
            record(lua, &error);
        }
    }
    for group in groups {
        if !group.get::<bool>(ANIMATION_PLAYING).unwrap_or(false) {
            continue;
        }
        let Some(children) = group.get::<Option<Table>>(ANIMATION_CHILDREN).unwrap_or(None) else {
            if let Err(error) = finish(lua, &group) {
                record(lua, &error);
            }
            continue;
        };
        let mut all_finished = true;
        for child in children.sequence_values::<Table>() {
            let Ok(child) = child else { continue };
            if !child.get::<bool>(ANIMATION_FINISHED).unwrap_or(false) {
                all_finished = false;
                break;
            }
        }
        if all_finished {
            if let Err(error) = finish(lua, &group) {
                record(lua, &error);
            }
        }
    }
}

/// Advance one animation; finish it at the end of its duration.
fn advance_one(lua: &Lua, this: &Table, now: f64) -> mlua::Result<()> {
    let start: f64 = this.get(ANIMATION_START)?;
    let elapsed = now - start;
    if elapsed < 0.0 {
        return Ok(());
    }
    if !this.get::<bool>(ANIMATION_PLAYED)? {
        let from = owner_alpha(lua, this).unwrap_or(1.0);
        this.set(ANIMATION_FROM, from)?;
        this.set(ANIMATION_PLAYED, true)?;
        fire(lua, this, "OnPlay")?;
    }
    let duration = param_number(this, "duration", 0.0).max(0.0);
    let t = if duration > 0.0 {
        (elapsed / duration).clamp(0.0, 1.0)
    } else {
        1.0
    };
    let kind: String = this.get(ANIMATION_KIND)?;
    if kind.eq_ignore_ascii_case("Alpha") {
        apply_alpha(lua, this, t);
    }
    if t >= 1.0 {
        finish(lua, this)?;
    }
    Ok(())
}

/// Set the owner's alpha (frame or region) to `from + change * t`.
fn apply_alpha(lua: &Lua, this: &Table, t: f64) {
    let change = param_number(this, "change", 0.0);
    let from: f64 = this.get(ANIMATION_FROM).unwrap_or(1.0);
    let alpha = frame_alpha_byte(from + change * t);
    let Some(owner) = owner_kind(lua, this) else {
        return;
    };
    let mut model = lua.app_data_mut::<Model>().expect("model app_data");
    match owner {
        Owner::Frame(h) => model.arena.set_alpha(h, alpha),
        Owner::Region(rh) => {
            model.region_data.entry(rh).or_default().alpha = Some(alpha);
        }
    }
}

/// The table's owner: a frame or a region, the same `GetRegionParent` answer stock scripts use.
enum Owner {
    Frame(FrameHandle),
    Region(RegionHandle),
}

fn owner_kind(lua: &Lua, this: &Table) -> Option<Owner> {
    let owner = this.get::<Table>(ANIMATION_OWNER).ok()?;
    if let Ok(h) = super::object::frame_handle_of(lua, &owner) {
        return Some(Owner::Frame(h));
    }
    super::region::region_handle_of(lua, &owner)
        .ok()
        .map(Owner::Region)
}

/// Mark an animation finished and fire `OnFinished` once.
fn finish(lua: &Lua, this: &Table) -> mlua::Result<()> {
    if this.get::<bool>(ANIMATION_FINISHED)? && !this.get::<bool>(ANIMATION_PLAYING)? {
        return Ok(());
    }
    this.set(ANIMATION_PLAYING, false)?;
    this.set(ANIMATION_FINISHED, true)?;
    fire(lua, this, "OnFinished")
}

/// Run one stored handler with the animation as its `self`.
fn fire(lua: &Lua, this: &Table, name: &str) -> mlua::Result<()> {
    let scripts: Table = this.get(ANIMATION_SCRIPTS)?;
    let Some(func) = scripts.get::<Option<Function>>(name)? else {
        return Ok(());
    };
    func.call::<()>(this.clone())
}

/// The owner's current alpha as a fraction (frame or region); `None` for a bad owner.
fn owner_alpha(lua: &Lua, this: &Table) -> Option<f64> {
    let owner = owner_kind(lua, this)?;
    let model = lua.app_data_ref::<Model>().expect("model app_data");
    match owner {
        Owner::Frame(h) => model.arena.frame(h).map(|f| alpha_f64(f.alpha)),
        Owner::Region(rh) => Some(
            model
                .region_data
                .get(&rh)
                .and_then(|d| d.alpha)
                .map_or(1.0, alpha_f64),
        ),
    }
}

fn is_group(this: &Table) -> mlua::Result<bool> {
    let kind: String = this.get(ANIMATION_KIND)?;
    Ok(kind.eq_ignore_ascii_case("AnimationGroup"))
}

/// A numeric XML parameter, defaulting when absent or malformed.
fn param_number(this: &Table, key: &str, default: f64) -> f64 {
    let Ok(params) = this.get::<Table>(ANIMATION_PARAMS) else {
        return default;
    };
    params
        .get::<Option<String>>(key)
        .ok()
        .flatten()
        .and_then(|text| text.trim().parse::<f64>().ok())
        .unwrap_or(default)
}

fn record(lua: &Lua, error: &mlua::Error) {
    lua.app_data_mut::<Model>()
        .expect("model app_data")
        .record_lua_error(error);
}
