//! The world map's arrow frames (`0x4a8bb0`), the seven bindings `WorldMapFrame.lua` and
//! `Blizzard_BattlefieldMinimap.lua` call. The reference keeps two singletons per UI session, the
//! world map's and the battlefield minimap's, each a pair of stock `Model` frames made by the first
//! `Create`: the player marker and the facing-cone effect beside it (`CGWorldMap::CreateArrowFrame`
//! and `CreateMiniArrowFrame`). Both draw `MinimapArrow.mdx`, are never freed, take their rect
//! from the file's bounding box, and face with the player.

use mlua::{Lua, MultiValue, ObjectLike, Table, Value};

use super::binding_abi::bool_or_default;
use super::object::{create_frame, decode_id, frame_handle_of, frame_wrapper, point_from_str};
use super::Model;

/// The arrow's model (`0x8453c0`), not `Rotating-MinimapArrow`.
pub const ARROW_MODEL: &str = "Interface\\Minimap\\MinimapArrow.mdx";

/// Half the file's bounding box, which is the size-less pane's rect in layout units (33.5 x 33.7
/// FrameXML units at 4:3); `0x4a7b20` centres the model there.
fn centre_of(facts: Option<&crate::widget::ModelFileFacts>) -> (f32, f32, f32) {
    facts.map_or((0.0, 0.0, 0.0), |f| {
        let (x, y) = f.extent();
        (x * 0.5, y * 0.5, 0.0)
    })
}

/// Which of the two singletons a binding addresses.
#[derive(Clone, Copy)]
enum Arrow {
    World,
    Mini,
}

/// The marker and the facing-cone effect drawn over it. The reference makes two model frames per
/// singleton and drives them together: `m_playerArrowFrame` / `m_playerArrowEffectFrame` and the
/// mini pair (`CreateArrowFrame`, `CreateMiniArrowFrame`).
#[derive(Clone, Copy)]
enum Part {
    Marker,
    Effect,
}

impl Part {
    const BOTH: [Part; 2] = [Part::Marker, Part::Effect];
}

impl Arrow {
    fn slot(self, part: Part, model: &Model) -> Option<u32> {
        match (self, part) {
            (Arrow::World, Part::Marker) => model.worldmap.arrow_world,
            (Arrow::World, Part::Effect) => model.worldmap.arrow_world_effect,
            (Arrow::Mini, Part::Marker) => model.worldmap.arrow_mini,
            (Arrow::Mini, Part::Effect) => model.worldmap.arrow_mini_effect,
        }
    }
    fn set_slot(self, part: Part, model: &mut Model, id: u32) {
        match (self, part) {
            (Arrow::World, Part::Marker) => model.worldmap.arrow_world = Some(id),
            (Arrow::World, Part::Effect) => model.worldmap.arrow_world_effect = Some(id),
            (Arrow::Mini, Part::Marker) => model.worldmap.arrow_mini = Some(id),
            (Arrow::Mini, Part::Effect) => model.worldmap.arrow_mini_effect = Some(id),
        }
    }
    /// The global name the reference gives each model (`SetName`), so `WorldMapFrame.lua` can set
    /// the effect's alpha and frame level by name.
    fn frame_name(self, part: Part) -> &'static str {
        match (self, part) {
            (Arrow::World, Part::Marker) => "PlayerArrowFrame",
            (Arrow::World, Part::Effect) => "PlayerArrowEffectFrame",
            (Arrow::Mini, Part::Marker) => "PlayerMiniArrowFrame",
            (Arrow::Mini, Part::Effect) => "PlayerMiniArrowEffectFrame",
        }
    }
    /// The model scale over `G48`: `5/3` for the world map, `10/9` for the mini, so the arrow keeps
    /// its apparent size as the window's shape changes (`0x4a7ad8`). Both parts of a pair use it.
    fn scale_over_g48(self) -> f32 {
        match self {
            Arrow::World => 5.0 / 3.0,
            Arrow::Mini => 10.0 / 9.0,
        }
    }
    fn usage_create(self) -> &'static str {
        match self {
            Arrow::World => "Usage: CreateWorldMapArrowFrame(parent)",
            Arrow::Mini => "Usage: CreateMiniWorldMapArrowFrame(parent)",
        }
    }
    fn no_this(self) -> &'static str {
        match self {
            Arrow::World => "CreateWorldMapArrowFrame(): Couldn't find 'this' in parent object",
            Arrow::Mini => "CreateMiniWorldMapArrowFrame(): Couldn't find 'this' in parent object",
        }
    }
    fn not_a_frame(self) -> &'static str {
        match self {
            Arrow::World => "CreateWorldMapArrowFrame(): Wrong object type, expected frame",
            Arrow::Mini => "CreateMiniWorldMapArrowFrame(): Wrong object type, expected frame",
        }
    }
    fn usage_position(self) -> &'static str {
        match self {
            Arrow::World => {
                "Usage: PositionWorldMapArrowFrame(\"point\" \"frame\" [, relativePoint] [, offsetX, offsetY])"
            }
            Arrow::Mini => {
                "Usage: PositionMiniWorldMapArrowFrame(\"point\" \"frame\" [, relativePoint] [, offsetX, offsetY])"
            }
        }
    }
}

/// The arrow's model scale for this screen: `G48`, the cell `K2` (`0x41ae70(1.0)` reads
/// `[0x832a48]`), times its factor (`0x4a7ad3`, `0x4a7ad8`).
fn arrow_scale(model: &Model, which: Arrow) -> f32 {
    model.layout_units().k2() * which.scale_over_g48()
}

/// Both pairs' model scale for new cells. The reference fixes it at creation, which a change of
/// cells repeats, as the rebuilt interface runs `CreateWorldMapArrowFrame` again
/// (`WorldMapFrame.lua:15`); a live resize sets it here instead.
pub(super) fn rescale(model: &mut Model) {
    for which in [Arrow::World, Arrow::Mini] {
        for part in Part::BOTH {
            let Some(id) = which.slot(part, model) else {
                continue;
            };
            let scale = arrow_scale(model, which);
            let Some(&h) = model.id_to_frame.get(&id) else {
                continue;
            };
            if let Some(crate::widget::KindState::Model(m)) =
                model.arena.frame_mut(h).map(|f| &mut f.kind_state)
            {
                m.scale = scale;
            }
        }
    }
}

/// `lua_isstring`: a string, or a number (which `lua_tostring` would print).
fn is_string(v: &Value) -> bool {
    matches!(v, Value::String(_) | Value::Integer(_) | Value::Number(_))
}

fn as_string(v: &Value) -> mlua::Result<String> {
    match v {
        Value::String(s) => Ok(s.to_str()?.to_string()),
        Value::Integer(i) => Ok(i.to_string()),
        Value::Number(n) => Ok(super::binding_abi::lua_number_text(*n)),
        _ => Ok(String::new()),
    }
}

/// `lua_isnumber`: a number, or a string that parses as one.
fn as_number(lua: &Lua, v: &Value) -> Option<f64> {
    lua.coerce_number(v.clone()).ok().flatten()
}

fn create(lua: &Lua, which: Arrow, parent: Value) -> mlua::Result<()> {
    let Value::Table(t) = parent else {
        return Err(mlua::Error::runtime(which.usage_create()));
    };
    if decode_id(&t).is_err() {
        return Err(mlua::Error::runtime(which.no_this()));
    }
    if frame_handle_of(lua, &t).is_err() {
        return Err(mlua::Error::runtime(which.not_a_frame()));
    }
    // The reference tests each pointer separately (`0x4a7a80`): a pair made by an earlier call is
    // left alone, and only a missing part is created. In a fresh session both are built, the
    // marker first as the reference does.
    for part in Part::BOTH {
        let exists = {
            let model = lua.app_data_ref::<Model>().expect("model app_data");
            which.slot(part, &model).is_some()
        };
        if !exists {
            create_one(lua, which, part, &t)?;
        }
    }
    Ok(())
}

/// One model frame of a pair: the reference's `CSimpleModel(parent)`, `SetModel`, `SetName`, and
/// the `CenterArrowModel` its loaded-file check runs (`CreateArrowFrame`).
fn create_one(lua: &Lua, which: Arrow, part: Part, parent: &Table) -> mlua::Result<()> {
    let name = which.frame_name(part);
    let wrapper = create_frame(
        lua,
        (
            Some(Value::String(lua.create_string("Model")?)),
            Some(Value::String(lua.create_string(name)?)),
            Some(Value::Table(parent.clone())),
            None,
        ),
    )?;
    let id = decode_id(&wrapper)?;
    let (scale, facts) = {
        let mut model = lua.app_data_mut::<Model>().expect("model app_data");
        let scale = arrow_scale(&model, which);
        (scale, model.model_facts_for(ARROW_MODEL))
    };
    // The reference's `SetModel` (`0x4a7a80` calls `0x76c8e0`) with no authored size: the rect is
    // the file's bounding box, and `0x4a7b20` centres the model on it.
    super::modelframe::with_model(lua, &wrapper, |m| {
        m.set_file(ARROW_MODEL.to_string(), facts.as_deref());
        m.scale = scale;
        m.position = centre_of(facts.as_deref());
    })?;
    let h = frame_handle_of(lua, &wrapper)?;
    let mut model = lua.app_data_mut::<Model>().expect("model app_data");
    model.apply_implicit_rect(h);
    which.set_slot(part, &mut model, id);
    Ok(())
}

fn arrow_wrapper(lua: &Lua, which: Arrow, part: Part) -> mlua::Result<Option<Table>> {
    let id = {
        let model = lua.app_data_ref::<Model>().expect("model app_data");
        which.slot(part, &model)
    };
    id.map(|id| frame_wrapper(lua, id)).transpose()
}

/// Both frames of a pair, or none: every driving verb is silent until both exist, as the
/// reference's `if (!frame || !effect) return 0;` guards.
fn arrow_pair(lua: &Lua, which: Arrow) -> mlua::Result<Option<(Table, Table)>> {
    let marker = arrow_wrapper(lua, which, Part::Marker)?;
    let effect = arrow_wrapper(lua, which, Part::Effect)?;
    Ok(match (marker, effect) {
        (Some(marker), Some(effect)) => Some((marker, effect)),
        _ => None,
    })
}

/// `UpdateWorldMapArrowFrames()` (`0x4a8d10`, `0x4a7c20`): re-centre every frame in both pairs
/// and copy the camera-tracked object's facing, the player's, to all of them; with no player the
/// facing stays.
fn update(lua: &Lua) -> mlua::Result<()> {
    for which in [Arrow::World, Arrow::Mini] {
        let (facing, tracked, facts) = {
            let mut model = lua.app_data_mut::<Model>().expect("model app_data");
            (
                model.worldmap.player_facing,
                model.worldmap.player_uv.is_some(),
                model.model_facts_for(ARROW_MODEL),
            )
        };
        for part in Part::BOTH {
            let Some(wrapper) = arrow_wrapper(lua, which, part)? else {
                continue;
            };
            let facts = facts.clone();
            super::modelframe::with_model(lua, &wrapper, |m| {
                m.position = centre_of(facts.as_deref());
                if tracked {
                    m.facing = facing;
                }
            })?;
        }
    }
    Ok(())
}

/// `Position...("point", "frame" [, relativePoint] [, offsetX, offsetY])` (`0x4a8d20`,
/// `0x4a8f20`): `SetPoint` on the marker and the effect, silent until both exist. A non-string
/// third argument means the same point and zero offsets, with no error; offsets apply only when
/// both are numbers.
fn position(lua: &Lua, which: Arrow, args: [Value; 5]) -> mlua::Result<()> {
    let Some((arrow, effect)) = arrow_pair(lua, which)? else {
        return Ok(());
    };
    let [point, frame, rel, x, y] = args;
    if !is_string(&point) || !is_string(&frame) {
        return Err(mlua::Error::runtime(which.usage_position()));
    }
    let point_name = as_string(&point)?;
    if point_from_str(&point_name).is_none() {
        return Err(mlua::Error::runtime("Unknown frame point"));
    }
    let frame_name = as_string(&frame)?;
    let resolved = if let Some(rest) = frame_name.strip_prefix("$parent") {
        // The reference (`0x76c5b0`) matches `$parent` in any case and takes the first named
        // ancestor's name, else "Top"; here the prefix matches as spelled and only the direct
        // parent's name is tried.
        let parent_name: Option<String> = arrow
            .call_method::<Option<Table>>("GetParent", ())?
            .and_then(|p| {
                p.call_method::<Option<String>>("GetName", ())
                    .ok()
                    .flatten()
            });
        parent_name.map(|p| format!("{p}{rest}"))
    } else {
        Some(frame_name.clone())
    };
    let target: Option<Table> = resolved
        .as_deref()
        .and_then(|n| lua.globals().get::<Option<Table>>(n).ok().flatten())
        .filter(|t| frame_handle_of(lua, t).is_ok());
    let Some(target) = target else {
        return Err(mlua::Error::runtime(format!(
            "Couldn't find frame named '{}'",
            resolved.unwrap_or(frame_name)
        )));
    };
    // The reference compares only the marker (`relative == frame`); the effect may be named.
    if decode_id(&target)? == decode_id(&arrow)? {
        return Err(mlua::Error::runtime(format!(
            "Error: {} is anchored to itself",
            resolved.unwrap_or(frame_name)
        )));
    }
    let (rel_point, ox, oy) = if is_string(&rel) {
        let r = as_string(&rel)?;
        if point_from_str(&r).is_none() {
            return Err(mlua::Error::runtime("Unknown frame point"));
        }
        match (as_number(lua, &x), as_number(lua, &y)) {
            (Some(x), Some(y)) => (r, x, y),
            _ => (r, 0.0, 0.0),
        }
    } else {
        (point_name.clone(), 0.0, 0.0)
    };
    // Both frames take the same point, as `frame->SetPoint(...)` then `effect->SetPoint(...)`.
    for frame in [arrow, effect] {
        frame.call_method::<()>(
            "SetPoint",
            (
                point_name.clone(),
                Value::Table(target.clone()),
                rel_point.clone(),
                ox,
                oy,
            ),
        )?;
    }
    Ok(())
}

/// `Show...([shown])` (`0x4a9120`, `0x4a9170`): show or hide both frames of the pair, silent until
/// both exist; the never-raising boolean coercion, true by default.
fn show(lua: &Lua, which: Arrow, args: MultiValue) -> mlua::Result<()> {
    let Some((arrow, effect)) = arrow_pair(lua, which)? else {
        return Ok(());
    };
    // An absent argument is `LUA_TNONE`, which the binding's nil test does not match, so a bare
    // call shows and an explicit nil hides.
    let on = match args.front() {
        None => true,
        Some(Value::Nil) => false,
        Some(v) => bool_or_default(Some(v), true),
    };
    let method = if on { "Show" } else { "Hide" };
    arrow.call_method::<()>(method, ())?;
    effect.call_method::<()>(method, ())
}

pub(super) fn install(lua: &Lua) -> mlua::Result<()> {
    let g = lua.globals();
    for (which, create_name, position_name, show_name) in [
        (
            Arrow::World,
            "CreateWorldMapArrowFrame",
            "PositionWorldMapArrowFrame",
            "ShowWorldMapArrowFrame",
        ),
        (
            Arrow::Mini,
            "CreateMiniWorldMapArrowFrame",
            "PositionMiniWorldMapArrowFrame",
            "ShowMiniWorldMapArrowFrame",
        ),
    ] {
        g.set(
            create_name,
            lua.create_function(move |lua, parent: Value| create(lua, which, parent))?,
        )?;
        g.set(
            position_name,
            lua.create_function(
                move |lua, (p, f, r, x, y): (Value, Value, Value, Value, Value)| {
                    position(lua, which, [p, f, r, x, y])
                },
            )?,
        )?;
        g.set(
            show_name,
            lua.create_function(move |lua, args: MultiValue| show(lua, which, args))?,
        )?;
    }
    g.set(
        "UpdateWorldMapArrowFrames",
        lua.create_function(|lua, ()| update(lua))?,
    )?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::script::UiScript;

    /// The arrow file's facts as the host hands them over: the header box the rect and the
    /// centring read.
    fn arrow_facts(s: &mut UiScript) {
        s.set_model_facts(
            ARROW_MODEL,
            crate::widget::ModelFileFacts {
                sequences: vec![crate::widget::SequenceFacts {
                    anim_id: 0,
                    duration_ms: 3333,
                    looping: true,
                }],
                bbox: ([-0.0127, -0.0118, 0.0], [0.0135, 0.0145, 0.0]),
                cameras: 0,
            },
        );
    }

    fn vm() -> UiScript {
        let mut s = UiScript::new().unwrap();
        s.set_screen_size(1024.0, 768.0);
        s.run(
            r#"WM = CreateFrame("Frame", "WM") WM:SetWidth(1024); WM:SetHeight(768) WM:SetPoint("CENTER", 0, 0)
               Detail = CreateFrame("Frame", "WorldMapDetailFrame", WM)
               Detail:SetWidth(1002); Detail:SetHeight(668) Detail:SetPoint("CENTER", 0, 0)"#,
        )
        .unwrap();
        s
    }

    #[test]
    fn create_validates_its_parent_and_makes_one_arrow_per_session() {
        let mut s = vm();
        for (call, want) in [
            (
                "CreateWorldMapArrowFrame(5)",
                "Usage: CreateWorldMapArrowFrame(parent)",
            ),
            (
                "CreateWorldMapArrowFrame({})",
                "CreateWorldMapArrowFrame(): Couldn't find 'this' in parent object",
            ),
            (
                "CreateWorldMapArrowFrame(WM:CreateTexture())",
                "CreateWorldMapArrowFrame(): Wrong object type, expected frame",
            ),
        ] {
            let err = s.run(call).unwrap_err().to_string();
            assert!(err.contains(want), "{call}: {err}");
        }
        s.run("CreateWorldMapArrowFrame(WM)").unwrap();
        let before = s
            .eval::<i64>("return table.getn({WM:GetChildren()})")
            .unwrap();
        s.run("CreateWorldMapArrowFrame(Detail)").unwrap();
        let after = s
            .eval::<i64>("return table.getn({WM:GetChildren()})")
            .unwrap();
        assert_eq!(
            before, after,
            "a second create is a no-op, whatever the parent"
        );
        let (kind, shown, w) = s
            .eval::<(String, bool, f32)>(
                "local a = ({WM:GetChildren()})[2] \
                 return a:GetObjectType(), a:IsShown(), a:GetWidth()",
            )
            .unwrap();
        assert_eq!(kind, "Model");
        assert!(shown, "born shown");
        assert_eq!(w, 0.0, "no authored size, no facts yet: no rect");
        // The rect is the box in layout units, 0.0262 x 1280 at 4:3, with the model centred on it.
        arrow_facts(&mut s);
        // Facts that land after the create are picked up by the next update, which re-centres.
        s.run("UpdateWorldMapArrowFrames()").unwrap();
        s.resolve();
        let w: f32 = s.eval("return ({WM:GetChildren()})[2]:GetWidth()").unwrap();
        assert!((w - 0.0262 * 1280.0).abs() < 0.05, "the box at 4:3: {w}");
        let pos = {
            let model = s.lua().app_data_ref::<Model>().expect("model");
            let id = model.worldmap.arrow_world.expect("the singleton");
            let fh = model.id_to_frame.get(&id).copied().expect("live");
            match &model.arena.frame(fh).expect("frame").kind_state {
                crate::widget::KindState::Model(m) => m.position,
                _ => panic!("not a Model"),
            }
        };
        assert!(
            (pos.0 - 0.0131).abs() < 1e-5 && (pos.1 - 0.01315).abs() < 1e-5 && pos.2 == 0.0,
            "centred on the box: {pos:?}"
        );
        assert_eq!(
            s.arity("CreateWorldMapArrowFrame(WM)").unwrap(),
            0,
            "zero values"
        );
    }

    /// The stock `WorldMapButton_OnUpdate` tail (`WorldMapFrame.lua:780-791`): turn and re-centre
    /// both model frames, then seat the pair at the player's UV or hide it off the map.
    const FOLLOW_FEED: &str = r#"
        function TestArrowFollowsFeed()
            UpdateWorldMapArrowFrames()
            local x, y = GetPlayerMapPosition("player")
            if x == 0 and y == 0 then
                ShowWorldMapArrowFrame(nil)
            else
                x = x * WorldMapDetailFrame:GetWidth()
                y = -y * WorldMapDetailFrame:GetHeight()
                PositionWorldMapArrowFrame("CENTER", "WorldMapDetailFrame", "TOPLEFT", x, y)
                ShowWorldMapArrowFrame(1)
            end
        end
    "#;

    /// `CreateArrowFrame` makes the marker and the facing-cone effect together, both on the same
    /// parent, both named for Lua; the stock update then gives them one projected point and one
    /// facing. `PlayerArrowEffectFrame:SetAlpha(0.65)` in OnLoad must find its global.
    #[test]
    fn the_arrow_and_its_effect_share_the_projected_point_and_facing() {
        let mut s = vm();
        arrow_facts(&mut s);
        s.run("CreateWorldMapArrowFrame(WM)").unwrap();

        let (marker, effect) = s
            .eval::<(String, String)>(
                "return PlayerArrowFrame:GetObjectType(), PlayerArrowEffectFrame:GetObjectType()",
            )
            .unwrap();
        assert_eq!((marker.as_str(), effect.as_str()), ("Model", "Model"));
        s.run("PlayerArrowEffectFrame:SetAlpha(0.65)").unwrap();

        // The mini singleton is a pair too.
        s.run("CreateMiniWorldMapArrowFrame(WM)").unwrap();
        let mini = s
            .eval::<(bool, bool)>(
                "return PlayerMiniArrowFrame ~= nil, PlayerMiniArrowEffectFrame ~= nil",
            )
            .unwrap();
        assert_eq!(mini, (true, true));

        s.run(FOLLOW_FEED).unwrap();
        s.set_world_map_feed(None, Some((0.25, 0.75)), 1.25, None, Vec::new(), Vec::new());
        s.run("TestArrowFollowsFeed()").unwrap();
        s.resolve();

        let (ax, ay, ex, ey, af, ef) = s
            .eval::<(f64, f64, f64, f64, f32, f32)>(
                "local ax, ay = PlayerArrowFrame:GetCenter() \
                 local ex, ey = PlayerArrowEffectFrame:GetCenter() \
                 return ax, ay, ex, ey, PlayerArrowFrame:GetFacing(), PlayerArrowEffectFrame:GetFacing()",
            )
            .unwrap();
        assert!(
            (ax - ex).abs() < 0.01 && (ay - ey).abs() < 0.01,
            "the effect is seated on the marker: ({ax}, {ay}) vs ({ex}, {ey})"
        );
        let (dl, dt, dw, dh) = s
            .eval::<(f64, f64, f64, f64)>(
                "return WorldMapDetailFrame:GetLeft(), WorldMapDetailFrame:GetTop(), \
                 WorldMapDetailFrame:GetWidth(), WorldMapDetailFrame:GetHeight()",
            )
            .unwrap();
        assert!(
            (ax - (dl + 0.25 * dw)).abs() < 0.01 && (ay - (dt - 0.75 * dh)).abs() < 0.01,
            "both follow the player UV: ({ax}, {ay}) on {dw}x{dh} at ({dl}, {dt})"
        );
        assert!(
            (af - 1.25).abs() < 1e-6 && (ef - 1.25).abs() < 1e-6,
            "one facing for the pair: {af}, {ef}"
        );
        // `GetAlpha` answers the reference's byte × 1/255 (`0x79b754`), so the stock 0.65 read
        // back through the byte grid is 165/255, not the literal.
        let alpha = s
            .eval::<f32>("return PlayerArrowEffectFrame:GetAlpha()")
            .unwrap();
        assert!(
            (alpha - 165.0 / 255.0).abs() < 1e-6,
            "the stock alpha reaches the effect frame by name: {alpha}"
        );
    }

    /// Off the displayed map the stock update hides the pair; back on it, a new UV and facing move
    /// and turn both frames together.
    #[test]
    fn the_effect_hides_off_the_map_and_follows_a_later_player_update() {
        let mut s = vm();
        arrow_facts(&mut s);
        s.run("CreateWorldMapArrowFrame(WM)").unwrap();
        s.run(FOLLOW_FEED).unwrap();

        s.set_world_map_feed(None, Some((0.25, 0.75)), 1.25, None, Vec::new(), Vec::new());
        s.run("TestArrowFollowsFeed()").unwrap();
        s.resolve();

        s.set_world_map_feed(None, None, 2.0, None, Vec::new(), Vec::new());
        s.run("TestArrowFollowsFeed()").unwrap();
        let (marker, effect) = s
            .eval::<(bool, bool)>(
                "return PlayerArrowFrame:IsShown(), PlayerArrowEffectFrame:IsShown()",
            )
            .unwrap();
        assert!(
            !marker && !effect,
            "off-map hides the marker AND the effect: {marker} {effect}"
        );

        s.set_world_map_feed(None, Some((0.4, 0.6)), 0.5, None, Vec::new(), Vec::new());
        s.run("TestArrowFollowsFeed()").unwrap();
        s.resolve();
        let (shown, ax, ay, ex, ey, af, ef) = s
            .eval::<(bool, f64, f64, f64, f64, f32, f32)>(
                "local ax, ay = PlayerArrowFrame:GetCenter() \
                 local ex, ey = PlayerArrowEffectFrame:GetCenter() \
                 return PlayerArrowFrame:IsShown() and PlayerArrowEffectFrame:IsShown(), \
                        ax, ay, ex, ey, PlayerArrowFrame:GetFacing(), PlayerArrowEffectFrame:GetFacing()",
            )
            .unwrap();
        assert!(shown, "a later on-map feed shows the pair again");
        assert!(
            (ax - ex).abs() < 0.01 && (ay - ey).abs() < 0.01,
            "the updated pair still shares its centre: ({ax}, {ay}) vs ({ex}, {ey})"
        );
        assert!(
            (af - 0.5).abs() < 1e-6 && (ef - 0.5).abs() < 1e-6,
            "the updated facing reaches both frames: {af}, {ef}"
        );
    }

    #[test]
    fn position_show_and_update_drive_the_arrow() {
        let mut s = vm();
        // No arrow yet: every one of the three is silent, whatever the arguments.
        s.run("PositionWorldMapArrowFrame(1, 2) ShowWorldMapArrowFrame('x') UpdateWorldMapArrowFrames()")
            .unwrap();
        s.run("CreateWorldMapArrowFrame(WM)").unwrap();
        // The file's facts: a size-less pane has no rect to anchor until its box is known.
        arrow_facts(&mut s);
        s.run("arrow = ({WM:GetChildren()})[2]").unwrap();
        for (call, want) in [
            (
                "PositionWorldMapArrowFrame(nil, 'WorldMapDetailFrame')",
                "Usage: PositionWorldMapArrowFrame",
            ),
            (
                "PositionWorldMapArrowFrame('NOWHERE', 'WorldMapDetailFrame')",
                "Unknown frame point",
            ),
            (
                "PositionWorldMapArrowFrame('CENTER', 'NoSuchFrame')",
                "Couldn't find frame named 'NoSuchFrame'",
            ),
            (
                "PositionWorldMapArrowFrame('CENTER', 'WorldMapDetailFrame', 'BOGUS', 1, 2)",
                "Unknown frame point",
            ),
        ] {
            let err = s.run(call).unwrap_err().to_string();
            assert!(err.contains(want), "{call}: {err}");
        }
        s.run("PositionWorldMapArrowFrame('CENTER', 'WorldMapDetailFrame', 'TOPLEFT', 100, -50)")
            .unwrap();
        s.resolve();
        let (cx, cy, dl, dt) = s
            .eval::<(f32, f32, f32, f32)>(
                "local cx, cy = arrow:GetCenter() \
                 return cx, cy, WorldMapDetailFrame:GetLeft(), WorldMapDetailFrame:GetTop()",
            )
            .unwrap();
        assert!(
            (cx - (dl + 100.0)).abs() < 0.01 && (cy - (dt - 50.0)).abs() < 0.01,
            "{cx} {cy}"
        );
        // A non-string third argument: the same point, offsets ignored, no error.
        s.run("PositionWorldMapArrowFrame('TOPLEFT', 'WorldMapDetailFrame', nil, 100, -50)")
            .unwrap();
        s.resolve();
        let (l, t, dl, dt) = s
            .eval::<(f32, f32, f32, f32)>(
                "return arrow:GetLeft(), arrow:GetTop(), WorldMapDetailFrame:GetLeft(), WorldMapDetailFrame:GetTop()",
            )
            .unwrap();
        assert!((l - dl).abs() < 0.01 && (t - dt).abs() < 0.01, "{l} {t}");
        // The name resolver expands $parent against the arrow's parent.
        s.run("PositionWorldMapArrowFrame('CENTER', '$parent')")
            .unwrap();

        s.run("ShowWorldMapArrowFrame(nil)").unwrap();
        assert!(
            !s.eval::<bool>("return arrow:IsShown()").unwrap(),
            "nil hides"
        );
        s.run("ShowWorldMapArrowFrame()").unwrap();
        assert!(
            s.eval::<bool>("return arrow:IsShown()").unwrap(),
            "absent shows"
        );
        s.run("ShowWorldMapArrowFrame('off')").unwrap();
        assert!(!s.eval::<bool>("return arrow:IsShown()").unwrap());
        s.run("ShowWorldMapArrowFrame(1)").unwrap();
        assert!(s.eval::<bool>("return arrow:IsShown()").unwrap());

        s.set_world_map_feed(None, Some((0.5, 0.5)), 1.25, None, Vec::new(), Vec::new());
        s.run("UpdateWorldMapArrowFrames()").unwrap();
        let facing = s.eval::<f32>("return arrow:GetFacing()").unwrap();
        assert!(
            (facing - 1.25).abs() < 1e-6,
            "the tracked object's facing: {facing}"
        );
        s.set_world_map_feed(None, None, 2.0, None, Vec::new(), Vec::new());
        s.run("UpdateWorldMapArrowFrames()").unwrap();
        assert!(
            (s.eval::<f32>("return arrow:GetFacing()").unwrap() - 1.25).abs() < 1e-6,
            "no tracked object: the facing does not move"
        );
    }
}
