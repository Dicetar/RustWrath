//! The 3.3.5a `<Animations>` object graph: one table per `<AnimationGroup>` and per animation
//! inside it, named and parent-keyed like any other XML object, with the frame-script verbs the
//! stock UI calls on them at load (`TutorialFrame.lua:790` stops `TutorialFrameCallOutPulser`,
//! the `Alpha` inside `TutorialFrameCallOut`'s group). The timeline that interpolates these
//! objects' parameters, and fires their `OnPlay`/`OnStop`/`OnFinished`, is the animation engine's
//! piece; here the objects carry their XML parameters and play state.

use mlua::{Lua, Table, Value};

/// An animation object of `kind` (`AnimationGroup`, `Alpha`, `Translation`, ...), named for the
/// global namespace when the XML names it, its owner the frame or region whose `<Animations>`
/// block holds it.
pub(crate) fn build(
    lua: &Lua,
    kind: &str,
    name: Option<&str>,
    attrs: &[(String, String)],
    owner: &Table,
) -> mlua::Result<Table> {
    let object = lua.create_table()?;
    object.set("__benilla_animation_kind", kind)?;
    object.set("__benilla_animation_name", name)?;
    object.set("__benilla_animation_owner", owner.clone())?;
    object.set("__benilla_animation_playing", false)?;
    let params = lua.create_table()?;
    for (key, value) in attrs {
        params.set(key.clone(), value.clone())?;
    }
    object.set("__benilla_animation_params", params)?;
    object.set("__benilla_animation_scripts", lua.create_table()?)?;

    object.set(
        "Play",
        lua.create_function(|_, this: Table| {
            this.set("__benilla_animation_playing", true)?;
            Ok(())
        })?,
    )?;
    object.set(
        "Stop",
        lua.create_function(|_, this: Table| {
            this.set("__benilla_animation_playing", false)?;
            Ok(())
        })?,
    )?;
    object.set(
        "IsPlaying",
        lua.create_function(|_, this: Table| {
            Ok(this.get::<bool>("__benilla_animation_playing")?)
        })?,
    )?;
    object.set(
        "GetName",
        lua.create_function(|_, this: Table| {
            this.get::<Option<String>>("__benilla_animation_name")
        })?,
    )?;
    object.set(
        "GetRegionParent",
        lua.create_function(|_, this: Table| {
            this.get::<Option<Table>>("__benilla_animation_owner")
        })?,
    )?;
    object.set(
        "SetScript",
        lua.create_function(|_, (this, name, func): (Table, String, Option<Value>)| {
            let scripts: Table = this.get("__benilla_animation_scripts")?;
            scripts.set(name, func)?;
            Ok(())
        })?,
    )?;
    object.set(
        "GetScript",
        lua.create_function(|_, (this, name): (Table, String)| {
            let scripts: Table = this.get("__benilla_animation_scripts")?;
            scripts.get::<Value>(name)
        })?,
    )?;
    Ok(object)
}
