//! The 3.3.5a `<Animations>` object graph and its timeline: one table per `<AnimationGroup>`
//! and per animation inside it, named and parent-keyed like any other XML object, with the
//! frame-script verbs the stock UI calls on them at load (`TutorialFrame.lua:790` stops
//! `TutorialFrameCallOutPulser`, the `Alpha` inside `TutorialFrameCallOut`'s group).
//!
//! Stock XML declares no triggers; stock Lua calls `group:Play()` (often from `OnShow`). A group's
//! `Play` sequences its children by `order` (parallel within one order) and the tick advances
//! them on the frame-script clock. `Alpha` writes the owner's alpha (frame or region) and
//! `Translation` shifts the owner's rect through the model's animation-offset maps; the timeline,
//! signals, parenting and ordering are common to every kind, and the remaining kinds
//! (`Rotation`, `Scale`, `Color`, `TextureCoord`) are recorded as the next slice.

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
const ANIMATION_LOOPING: &str = "__benilla_animation_looping";
const ANIMATION_PLAYING: &str = "__benilla_animation_playing";
const ANIMATION_FINISHED: &str = "__benilla_animation_finished";
const ANIMATION_PLAYED: &str = "__benilla_animation_played";
const ANIMATION_PAUSED: &str = "__benilla_animation_paused";
const ANIMATION_PAUSED_AT: &str = "__benilla_animation_paused_at";
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
    object.set(ANIMATION_PAUSED, false)?;
    object.set(ANIMATION_PAUSED_AT, 0.0)?;
    object.set(ANIMATION_START, 0.0)?;
    object.set(ANIMATION_FROM, 1.0)?;
    let params = lua.create_table()?;
    for (key, value) in attrs {
        params.set(key.clone(), value.clone())?;
    }
    object.set(ANIMATION_PARAMS, params)?;
    object.set(ANIMATION_SCRIPTS, lua.create_table()?)?;
    let looping = attrs
        .iter()
        .find(|(key, _)| key.eq_ignore_ascii_case("looping"))
        .map(|(_, value)| value.clone())
        .unwrap_or_default();
    object.set(ANIMATION_LOOPING, looping)?;

    object.set(
        "Play",
        lua.create_function(|lua, this: Table| play(lua, &this))?,
    )?;
    object.set(
        "Stop",
        lua.create_function(|lua, this: Table| stop(lua, &this, true))?,
    )?;
    object.set(
        "Pause",
        lua.create_function(|lua, this: Table| pause(lua, &this))?,
    )?;
    object.set(
        "Resume",
        lua.create_function(|lua, this: Table| resume(lua, &this))?,
    )?;
    object.set(
        "SetDuration",
        lua.create_function(|lua, (this, value): (Table, Value)| {
            set_param(lua, &this, "duration", value)
        })?,
    )?;
    object.set(
        "SetOrder",
        lua.create_function(|lua, (this, value): (Table, Value)| {
            set_param(lua, &this, "order", value)
        })?,
    )?;
    object.set(
        "SetChange",
        lua.create_function(|lua, (this, value): (Table, Value)| {
            set_param(lua, &this, "change", value)
        })?,
    )?;
    object.set(
        "SetOffset",
        lua.create_function(|lua, (this, x, y): (Table, Value, Value)| {
            set_param(lua, &this, "offsetX", x)?;
            set_param(lua, &this, "offsetY", y)
        })?,
    )?;
    object.set(
        "SetDegrees",
        lua.create_function(|lua, (this, value): (Table, Value)| {
            set_param(lua, &this, "degrees", value)
        })?,
    )?;
    // `group:SetLooping("REPEAT"|"BOUNCE"|"NONE")`: the group's XML `looping` attribute, set
    // after construction. `REPEAT` restarts the schedule; `BOUNCE` currently repeats too (the
    // reverse-leg semantics are recorded as the next slice).
    object.set(
        "SetLooping",
        lua.create_function(|_, (this, value): (Table, Value)| {
            let looping = match value {
                Value::String(s) => s.to_string_lossy(),
                _ => String::new(),
            };
            this.set(ANIMATION_LOOPING, looping)?;
            Ok(())
        })?,
    )?;
    // `group:CreateAnimation(type[, name])`: the programmatic child constructor. Stock XML never
    // calls it; installed addons do (bartender4 and friends), and children it makes run through
    // the same timeline as XML ones.
    object.set(
        "CreateAnimation",
        lua.create_function(|lua, (this, kind, name): (Table, Value, Value)| {
            if !is_group(&this)? {
                return Err(mlua::Error::runtime("CreateAnimation on a non-group"));
            }
            let kind = super::binding_abi::string_arg(lua, kind, "CreateAnimation(type)")?;
            let name = super::binding_abi::optional_string(lua, &name);
            create_child(lua, &this, &kind, name.as_deref())
        })?,
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

/// `frame:CreateAnimationGroup([name])`: a programmatic group owned by `owner`. A name publishes
/// it in the global namespace, as a named XML group does.
pub(crate) fn create_group(lua: &Lua, owner: &Table, name: Option<&str>) -> mlua::Result<Table> {
    let group = build(lua, "AnimationGroup", name, &[], owner, None)?;
    link_children(&group, lua.create_table()?)?;
    if let Some(name) = name {
        lua.globals().set(name, group.clone())?;
    }
    Ok(group)
}

/// `group:CreateAnimation(type[, name])`: append a child owned by the group's owner.
fn create_child(
    lua: &Lua,
    group: &Table,
    kind: &str,
    name: Option<&str>,
) -> mlua::Result<Table> {
    let owner: Table = group.get(ANIMATION_OWNER)?;
    let child = build(lua, kind, name, &[], &owner, Some(group))?;
    let children = match group.get::<Option<Table>>(ANIMATION_CHILDREN)? {
        Some(children) => children,
        None => {
            let children = lua.create_table()?;
            group.set(ANIMATION_CHILDREN, children.clone())?;
            children
        }
    };
    children.raw_set(children.raw_len() + 1, child.clone())?;
    Ok(child)
}

/// `animation:SetDuration(seconds)` and friends: the numeric XML parameter of the same name.
fn set_param(lua: &Lua, this: &Table, key: &str, value: Value) -> mlua::Result<()> {
    let number = super::binding_abi::coerced_number(lua, Some(value));
    let params: Table = this.get(ANIMATION_PARAMS)?;
    params.set(key, number.to_string())
}

/// `Pause`: freeze the run where it is; a group freezes its children.
fn pause(lua: &Lua, this: &Table) -> mlua::Result<()> {
    if !this.get::<bool>(ANIMATION_PLAYING)? || this.get::<bool>(ANIMATION_PAUSED)? {
        return Ok(());
    }
    let now = lua.app_data_ref::<Model>().expect("model app_data").now;
    this.set(ANIMATION_PAUSED, true)?;
    this.set(ANIMATION_PAUSED_AT, now)?;
    if is_group(this)? {
        if let Some(children) = this.get::<Option<Table>>(ANIMATION_CHILDREN)? {
            for child in children.sequence_values::<Table>() {
                pause(lua, &child?)?;
            }
        }
    }
    Ok(())
}

/// `Resume`: carry the frozen elapsed time into the start clock.
fn resume(lua: &Lua, this: &Table) -> mlua::Result<()> {
    if !this.get::<bool>(ANIMATION_PAUSED)? {
        return Ok(());
    }
    let now = lua.app_data_ref::<Model>().expect("model app_data").now;
    let paused_at: f64 = this.get(ANIMATION_PAUSED_AT)?;
    if !is_group(this)? {
        let start: f64 = this.get(ANIMATION_START)?;
        this.set(ANIMATION_START, start + (now - paused_at))?;
    }
    this.set(ANIMATION_PAUSED, false)?;
    if is_group(this)? {
        if let Some(children) = this.get::<Option<Table>>(ANIMATION_CHILDREN)? {
            for child in children.sequence_values::<Table>() {
                resume(lua, &child?)?;
            }
        }
    }
    Ok(())
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
fn start_group(_lua: &Lua, group: &Table, now: f64) -> mlua::Result<()> {
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
    this.set(ANIMATION_PAUSED, false)?;
    Ok(())
}

/// `Stop`: end a run and fire `OnStop` (the reference's stop signal), recursively for a group.
fn stop(lua: &Lua, this: &Table, signal: bool) -> mlua::Result<()> {
    this.set(ANIMATION_PLAYING, false)?;
    this.set(ANIMATION_PAUSED, false)?;
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
        if !playing || this.get::<bool>(ANIMATION_PAUSED).unwrap_or(false) {
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
            let looping: String = group
                .get::<Option<String>>(ANIMATION_LOOPING)
                .ok()
                .flatten()
                .unwrap_or_default();
            if looping.eq_ignore_ascii_case("REPEAT") || looping.eq_ignore_ascii_case("BOUNCE") {
                if let Err(error) = start_group(lua, &group, now) {
                    record(lua, &error);
                }
            } else if let Err(error) = finish(lua, &group) {
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
    } else if kind.eq_ignore_ascii_case("Translation") {
        apply_translation(lua, this, t);
    } else if kind.eq_ignore_ascii_case("Rotation") {
        apply_rotation(lua, this, t);
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

/// `<Translation>`: the owner's rect moves by `offsetX/offsetY × t` from its un-animated place.
/// The offset lands in the `Model` maps the rect readers add, so draw and hit tests move together.
fn apply_translation(lua: &Lua, this: &Table, t: f64) {
    let dx = param_number(this, "offsetX", 0.0) as f32 * t as f32;
    let dy = param_number(this, "offsetY", 0.0) as f32 * t as f32;
    let Some(owner) = owner_kind(lua, this) else {
        return;
    };
    let mut model = lua.app_data_mut::<Model>().expect("model app_data");
    match owner {
        Owner::Frame(h) => {
            model.animation_frame_offsets.insert(h, (dx, dy));
        }
        Owner::Region(rh) => {
            model.animation_region_offsets.insert(rh, (dx, dy));
        }
    }
}

/// `<Rotation>`: the owner frame turns by `degrees × t`, counter-clockwise in UI space. Region
/// owners are skipped until the region-rotation seam lands (frame rotation covers the addon
/// spinners, which rotate a frame holding their texture).
fn apply_rotation(lua: &Lua, this: &Table, t: f64) {
    let degrees = param_number(this, "degrees", 0.0);
    let angle = (degrees as f32).to_radians() * t as f32;
    let Some(Owner::Frame(h)) = owner_kind(lua, this) else {
        return;
    };
    lua.app_data_mut::<Model>()
        .expect("model app_data")
        .animation_frame_rotations
        .insert(h, angle);
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
fn fire(_lua: &Lua, this: &Table, name: &str) -> mlua::Result<()> {
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

#[cfg(test)]
mod tests {
    use mlua::{Function, ObjectLike, Table};

    use crate::order::ZTarget;
    use crate::script::{QuadContent, UiScript, UiTarget};

    fn attrs(pairs: &[(&str, &str)]) -> Vec<(String, String)> {
        pairs
            .iter()
            .map(|(key, value)| ((*key).to_string(), (*value).to_string()))
            .collect()
    }

    fn owner(s: &UiScript) -> Table {
        s.lua().globals().get("f").unwrap()
    }

    fn counter(lua: &mlua::Lua) -> Function {
        lua.load("return function() HITS = (HITS or 0) + 1 end")
            .eval::<Function>()
            .unwrap()
    }

    #[test]
    fn alpha_groups_sequence_and_fire_finished_once() {
        let mut s = UiScript::with_target(UiTarget::Wrath335).unwrap();
        s.set_screen_size(100.0, 100.0);
        s.run(
            r#"
            f = CreateFrame("Frame")
            f:SetSize(20, 20)
            f:SetPoint("CENTER", 0, 0)
            f:SetAlpha(1)
        "#,
        )
        .unwrap();
        let owner = owner(&s);
        let lua = s.lua();
        let group = super::build(lua, "AnimationGroup", None, &[], &owner, None).unwrap();
        let fade_out = super::build(
            lua,
            "Alpha",
            None,
            &attrs(&[("change", "-1"), ("duration", "0"), ("order", "1")]),
            &owner,
            Some(&group),
        )
        .unwrap();
        let fade_in = super::build(
            lua,
            "Alpha",
            None,
            &attrs(&[("change", "1"), ("duration", "0.2"), ("order", "2")]),
            &owner,
            Some(&group),
        )
        .unwrap();
        let children = lua.create_table().unwrap();
        children.raw_set(1, fade_out.clone()).unwrap();
        children.raw_set(2, fade_in.clone()).unwrap();
        super::link_children(&group, children).unwrap();
        let handler = counter(lua);
        fade_out
            .call_method::<()>("SetScript", ("OnFinished", Some(handler.clone())))
            .unwrap();
        fade_in
            .call_method::<()>("SetScript", ("OnFinished", Some(handler.clone())))
            .unwrap();
        group
            .call_method::<()>("SetScript", ("OnFinished", Some(handler)))
            .unwrap();

        group.call_method::<()>("Play", ()).unwrap();
        assert_eq!(
            s.eval::<i64>("return HITS or 0").unwrap(),
            0,
            "nothing fires before a tick"
        );
        s.tick(0.01);
        assert_eq!(
            s.eval::<i64>("return HITS").unwrap(),
            1,
            "the instant fade-out finished"
        );
        let mid = s.eval::<f64>("return f:GetAlpha()").unwrap();
        assert!(mid > 0.0 && mid < 0.1, "the fade-in barely started: {mid}");
        s.tick(0.25);
        assert_eq!(
            s.eval::<i64>("return HITS").unwrap(),
            3,
            "the fade-in and the group each finished once"
        );
        assert_eq!(s.eval::<f64>("return f:GetAlpha()").unwrap(), 1.0);
        s.tick(0.5);
        assert_eq!(
            s.eval::<i64>("return HITS").unwrap(),
            3,
            "finished animations do not refire"
        );
    }

    #[test]
    fn a_translation_moves_the_frame_rect_between_readers() {
        let mut s = UiScript::with_target(UiTarget::Wrath335).unwrap();
        s.set_screen_size(100.0, 100.0);
        s.run(
            r#"
            f = CreateFrame("Frame")
            f:SetSize(20, 20)
            f:SetPoint("CENTER", 0, 0)
        "#,
        )
        .unwrap();
        let owner = owner(&s);
        let lua = s.lua();
        let h = crate::script::object::frame_handle_of(lua, &owner).unwrap();
        let group = super::build(lua, "AnimationGroup", None, &[], &owner, None).unwrap();
        let shift = super::build(
            lua,
            "Translation",
            None,
            &attrs(&[("offsetX", "10"), ("offsetY", "4"), ("duration", "0.2")]),
            &owner,
            Some(&group),
        )
        .unwrap();
        let children = lua.create_table().unwrap();
        children.raw_set(1, shift).unwrap();
        super::link_children(&group, children).unwrap();
        group.call_method::<()>("Play", ()).unwrap();

        s.tick(0.1);
        let rect = frame_rect(&s, h);
        assert!((rect.left - 45.0).abs() < 0.01, "x half-moved: {}", rect.left);
        assert!(
            (rect.bottom - 42.0).abs() < 0.01,
            "y half-moved: {}",
            rect.bottom
        );
        s.tick(0.15);
        let rect = frame_rect(&s, h);
        assert!((rect.left - 50.0).abs() < 0.01, "x landed: {}", rect.left);
        assert!(
            (rect.bottom - 44.0).abs() < 0.01,
            "y landed: {}",
            rect.bottom
        );
    }

    #[test]
    fn the_programmatic_constructors_and_setters_drive_a_run() {
        let mut s = UiScript::with_target(UiTarget::Wrath335).unwrap();
        s.set_screen_size(100.0, 100.0);
        s.run(
            r#"
            f = CreateFrame("Frame")
            f:SetSize(10, 10)
            f:SetPoint("CENTER", 0, 0)
            f:SetAlpha(1)
            g = f:CreateAnimationGroup()
            a = g:CreateAnimation("Alpha")
            a:SetDuration(0.2)
            a:SetChange(-1)
            HITS = 0
            a:SetScript("OnFinished", function() HITS = HITS + 1 end)
        "#,
        )
        .unwrap();
        s.run("g:Play()").unwrap();
        s.tick(0.1);
        s.run("g:Pause()").unwrap();
        let frozen = s.eval::<f64>("return f:GetAlpha()").unwrap();
        assert!(frozen > 0.4 && frozen < 0.6, "half faded: {frozen}");
        s.tick(0.5);
        assert_eq!(
            s.eval::<f64>("return f:GetAlpha()").unwrap(),
            frozen,
            "paused runs do not advance"
        );
        s.run("g:Resume()").unwrap();
        s.tick(0.15);
        assert_eq!(s.eval::<f64>("return f:GetAlpha()").unwrap(), 0.0);
        assert_eq!(s.eval::<i64>("return HITS").unwrap(), 1);
    }

    #[test]
    fn a_frame_rotation_orbits_its_region_quad() {
        let mut s = UiScript::with_target(UiTarget::Wrath335).unwrap();
        s.set_screen_size(100.0, 100.0);
        s.run(
            r#"
            f = CreateFrame("Frame")
            f:SetSize(20, 20)
            f:SetPoint("CENTER", 0, 0)
            t = f:CreateTexture()
            t:SetSize(4, 4)
            t:SetPoint("CENTER", f, "CENTER", 8, 0)
            g = f:CreateAnimationGroup()
            r = g:CreateAnimation("Rotation")
            r:SetDegrees(90)
            r:SetDuration(0.2)
        "#,
        )
        .unwrap();
        let tex: Table = s.lua().globals().get("t").unwrap();
        let rh = crate::script::region::region_handle_of(s.lua(), &tex).unwrap();
        s.run("g:Play()").unwrap();
        s.tick(0.1);
        let quad = s
            .extract()
            .into_iter()
            .find(|quad| quad.target == ZTarget::Region(rh))
            .expect("texture quad");
        let rect = quad.rect.expect("resolved rect");
        let (cx, cy) = (
            (rect.left + rect.right) * 0.5,
            (rect.bottom + rect.top) * 0.5,
        );
        assert!((cx - 55.657).abs() < 0.01, "x orbited: {cx}");
        assert!((cy - 55.657).abs() < 0.01, "y orbited: {cy}");
        assert!(
            (quad.rotation - std::f32::consts::FRAC_PI_4).abs() < 1e-4,
            "45 degrees counter-clockwise: {}",
            quad.rotation
        );
    }

    #[test]
    fn a_repeating_group_restarts_its_rotation() {
        let mut s = UiScript::with_target(UiTarget::Wrath335).unwrap();
        s.set_screen_size(100.0, 100.0);
        s.run(
            r#"
            f = CreateFrame("Frame")
            f:SetSize(20, 20)
            f:SetPoint("CENTER", 0, 0)
            t = f:CreateTexture()
            t:SetSize(4, 4)
            t:SetPoint("CENTER", f, "CENTER", 8, 0)
            g = f:CreateAnimationGroup()
            r = g:CreateAnimation("Rotation")
            r:SetDegrees(360)
            r:SetDuration(0.2)
            g:SetLooping("repeat")
        "#,
        )
        .unwrap();
        let tex: Table = s.lua().globals().get("t").unwrap();
        let rh = crate::script::region::region_handle_of(s.lua(), &tex).unwrap();
        s.run("g:Play()").unwrap();
        s.tick(0.25);
        s.tick(0.05);
        let quad = s
            .extract()
            .into_iter()
            .find(|quad| quad.target == ZTarget::Region(rh))
            .expect("texture quad");
        assert!(
            (quad.rotation - std::f32::consts::FRAC_PI_2).abs() < 1e-3,
            "a quarter turn after the restart: {}",
            quad.rotation
        );
        assert!(s.eval::<bool>("return r:IsPlaying()").unwrap());
    }

    fn frame_rect(s: &UiScript, h: crate::widget::FrameHandle) -> crate::layout::Rect {
        s.extract()
            .into_iter()
            .find_map(|quad| match quad.target {
                ZTarget::Frame(fh)
                    if fh == h && matches!(quad.content, QuadContent::Frame) =>
                {
                    quad.rect
                }
                _ => None,
            })
            .expect("the frame's own quad")
    }
}
