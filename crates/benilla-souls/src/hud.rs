//! The Souls HUD: the predicted stamina bar (bottom centre), a mode chip at its left end, a
//! marker on the locked-on unit, and a windup bar under each striking enemy's nameplate (under
//! its head when it has none), clear of its name ([`super::server`]). Plain coloured quads above the world (no text yet). Hidden when Souls
//! combat is off, the `hud` switch is off, or there is no local player.
//!
//! The bar is green; red for a moment after a press refused for stamina, blue while guarding,
//! yellow while sprinting. The chip is gold with the kit's table, grey on the stock fallback; it
//! turns teal while the server script answers. The marker is red, gold while a riposte is open. A
//! strike bar is a dark track the length of the windup. A pale zone at its end and a white tick
//! where the zone starts mark the dodge window: a roll pressed once the fill passes the tick covers
//! the strike. The fill is the strike's kind colour (orange, blue for a thrust, purple for a
//! slam) and turns near-white inside the zone; dimmed for a strike on someone else.
//!
//! While crouched, the eye meter sits over the bar ([`super::server::Eye`]): a dark track (red
//! while the stealth is broken) filled with the highest suspicion around, pale while unnoticed,
//! yellow once a creature looks, orange once it comes to investigate, red when it spots us; a
//! dot of the same colour hangs over that creature. A sneak attack flashes the meter white.

use benilla_world::view::WorldCamera;
use bevy::prelude::*;

use super::server::{Eye, EyeMode, SoulsLink, StrikeKind};
use super::{current_kit, souls_on, SoulsConfig, SoulsState};
use crate::combat_kit::PlayerKit;
use crate::combat_mode::CombatModeView;
use crate::combat_playback::CombatPlaybackState;
use crate::combat_profiles::CombatProfiles;
use crate::entities::{overhead_anchor, BoneAttach, OverheadFallback};
use crate::net::SelfPlayer;
use crate::vplates::{gx_px, plate_basis, VPlates, PLATE_H, PLATE_LIFT};

const BAR_WIDTH: f32 = 320.0;
const BAR_HEIGHT: f32 = 10.0;
const BAR_BOTTOM: f32 = 96.0;
const CHIP: f32 = 14.0;
const MARKER: f32 = 10.0;
/// The marker sits this far above the locked unit's feet, yards.
const MARKER_HEIGHT_YARDS: f32 = 1.6;
/// Strike bars: how many can show at once, their full size, and the gap under the plate (or the
/// head), px.
const STRIKE_BARS: u8 = 4;
const STRIKE_WIDTH: f32 = 84.0;
const STRIKE_HEIGHT: f32 = 8.0;
/// The tick at the dodge zone's start: its width, and how far it reaches above and below the
/// track, px.
const STRIKE_TICK: f32 = 2.0;
const STRIKE_TICK_OVERHANG: f32 = 3.0;
const STRIKE_GAP: f32 = 3.0;
/// The creature stance (ER poise) bar: its size, and its gap over the plate, px.
const POISE_WIDTH: f32 = 88.0;
const POISE_HEIGHT: f32 = 5.0;
const POISE_GAP: f32 = 16.0;
/// A stance reading stays up this long after it arrived, seconds.
const POISE_SHOW_SECONDS: f32 = 5.0;
/// The eye meter: its size and its gap over the stamina bar, px.
const EYE_WIDTH: f32 = 96.0;
const EYE_HEIGHT: f32 = 6.0;
const EYE_GAP: f32 = 8.0;
/// The dot over the watching creature: its size, px, and its height over the feet, yards.
const EYE_MARK: f32 = 8.0;
const EYE_MARK_HEIGHT_YARDS: f32 = 2.4;

/// One HUD quad.
#[derive(Component, Clone, Copy, PartialEq, Eq, Debug)]
pub(super) enum HudPart {
    Bar,
    Fill,
    Chip,
    Lock,
    /// The n-th strike bar's track; the four below are its children.
    Strike(u8),
    /// The dodge zone: from [`super::server::StrikeShown::dodge_from`] to the end.
    StrikeZone(u8),
    /// The windup so far.
    StrikeFill(u8),
    /// The tick where the dodge zone starts.
    StrikeTick(u8),
    /// The tick where the parry window starts, inside the dodge zone.
    StrikeParry(u8),
    /// The stance bar over a creature's plate, and its fill.
    Poise,
    PoiseFill,
    /// The eye meter's track; the fill is its child.
    Eye,
    EyeFill,
    /// The dot over the creature holding the suspicion.
    EyeMark,
}

/// Spawn the quads once, parked hidden.
pub(super) fn spawn_hud(mut commands: Commands) {
    commands
        .spawn((
            HudPart::Bar,
            Node {
                position_type: PositionType::Absolute,
                left: Val::Percent(50.0),
                bottom: Val::Px(BAR_BOTTOM),
                width: Val::Px(BAR_WIDTH),
                height: Val::Px(BAR_HEIGHT),
                margin: UiRect {
                    left: Val::Px(-BAR_WIDTH / 2.0),
                    ..default()
                },
                ..default()
            },
            BackgroundColor(Color::srgba(0.0, 0.0, 0.0, 0.6)),
            // Above the world and the UI quads, below the loading cover (1000), like the letterbox.
            GlobalZIndex(900),
            Visibility::Hidden,
        ))
        .with_children(|bar| {
            bar.spawn((
                HudPart::Fill,
                Node {
                    width: Val::Percent(100.0),
                    height: Val::Percent(100.0),
                    ..default()
                },
                BackgroundColor(Color::srgb(0.2, 0.75, 0.25)),
                Visibility::Inherited,
            ));
        });
    commands.spawn((
        HudPart::Chip,
        Node {
            position_type: PositionType::Absolute,
            left: Val::Percent(50.0),
            bottom: Val::Px(BAR_BOTTOM - (CHIP - BAR_HEIGHT) / 2.0),
            width: Val::Px(CHIP),
            height: Val::Px(CHIP),
            margin: UiRect {
                left: Val::Px(-BAR_WIDTH / 2.0 - CHIP - 6.0),
                ..default()
            },
            ..default()
        },
        BackgroundColor(Color::srgb(0.85, 0.7, 0.2)),
        GlobalZIndex(900),
        Visibility::Hidden,
    ));
    commands.spawn((
        HudPart::Lock,
        Node {
            position_type: PositionType::Absolute,
            width: Val::Px(MARKER),
            height: Val::Px(MARKER),
            ..default()
        },
        BackgroundColor(Color::srgb(0.9, 0.15, 0.1)),
        GlobalZIndex(900),
        Visibility::Hidden,
    ));
    commands
        .spawn((
            HudPart::Poise,
            Node {
                position_type: PositionType::Absolute,
                width: Val::Px(POISE_WIDTH),
                height: Val::Px(POISE_HEIGHT),
                ..default()
            },
            BackgroundColor(Color::srgba(0.0, 0.0, 0.0, 0.6)),
            GlobalZIndex(900),
            Visibility::Hidden,
        ))
        .with_children(|bar| {
            bar.spawn((
                HudPart::PoiseFill,
                Node {
                    width: Val::Percent(100.0),
                    height: Val::Percent(100.0),
                    ..default()
                },
                BackgroundColor(Color::srgb(0.45, 0.65, 0.95)),
                Visibility::Inherited,
            ));
        });
    for index in 0..STRIKE_BARS {
        let child = |left: Val, width: Val, top: Val, height: Val| Node {
            position_type: PositionType::Absolute,
            left,
            width,
            top,
            height,
            ..default()
        };
        commands
            .spawn((
                HudPart::Strike(index),
                Node {
                    position_type: PositionType::Absolute,
                    width: Val::Px(STRIKE_WIDTH),
                    height: Val::Px(STRIKE_HEIGHT),
                    ..default()
                },
                BackgroundColor(Color::srgba(0.0, 0.0, 0.0, 0.7)),
                GlobalZIndex(900),
                Visibility::Hidden,
            ))
            .with_children(|track| {
                // Later children draw over earlier ones: zone, then fill, then tick.
                track.spawn((
                    HudPart::StrikeZone(index),
                    child(
                        Val::Percent(100.0),
                        Val::Percent(0.0),
                        Val::Px(0.0),
                        Val::Percent(100.0),
                    ),
                    BackgroundColor(Color::srgba(1.0, 1.0, 1.0, 0.25)),
                ));
                track.spawn((
                    HudPart::StrikeFill(index),
                    child(
                        Val::Px(0.0),
                        Val::Percent(0.0),
                        Val::Px(0.0),
                        Val::Percent(100.0),
                    ),
                    BackgroundColor(Color::srgb(0.95, 0.5, 0.1)),
                ));
                track.spawn((
                    HudPart::StrikeTick(index),
                    child(
                        Val::Percent(100.0),
                        Val::Px(STRIKE_TICK),
                        Val::Px(-STRIKE_TICK_OVERHANG),
                        Val::Px(STRIKE_HEIGHT + 2.0 * STRIKE_TICK_OVERHANG),
                    ),
                    BackgroundColor(Color::WHITE),
                ));
                // The parry window's start, later than the dodge's: gold, so a parry read is
                // told apart from a roll at a glance.
                track.spawn((
                    HudPart::StrikeParry(index),
                    child(
                        Val::Percent(100.0),
                        Val::Px(STRIKE_TICK),
                        Val::Px(-STRIKE_TICK_OVERHANG),
                        Val::Px(STRIKE_HEIGHT + 2.0 * STRIKE_TICK_OVERHANG),
                    ),
                    BackgroundColor(Color::srgb(0.95, 0.8, 0.2)),
                ));
            });
    }
    commands
        .spawn((
            HudPart::Eye,
            Node {
                position_type: PositionType::Absolute,
                left: Val::Percent(50.0),
                bottom: Val::Px(BAR_BOTTOM + BAR_HEIGHT + EYE_GAP),
                width: Val::Px(EYE_WIDTH),
                height: Val::Px(EYE_HEIGHT),
                margin: UiRect {
                    left: Val::Px(-EYE_WIDTH / 2.0),
                    ..default()
                },
                ..default()
            },
            BackgroundColor(Color::srgba(0.05, 0.08, 0.12, 0.75)),
            GlobalZIndex(900),
            Visibility::Hidden,
        ))
        .with_children(|track| {
            track.spawn((
                HudPart::EyeFill,
                Node {
                    width: Val::Percent(0.0),
                    height: Val::Percent(100.0),
                    ..default()
                },
                BackgroundColor(eye_fill(0)),
                Visibility::Inherited,
            ));
        });
    commands.spawn((
        HudPart::EyeMark,
        Node {
            position_type: PositionType::Absolute,
            width: Val::Px(EYE_MARK),
            height: Val::Px(EYE_MARK),
            ..default()
        },
        BackgroundColor(eye_fill(0)),
        GlobalZIndex(900),
        Visibility::Hidden,
    ));
}

/// The eye meter's colour for a suspicion: pale, yellow from a look, orange from an
/// investigation, red when spotted.
fn eye_fill(suspicion: u8) -> Color {
    if suspicion >= 100 {
        Color::srgb(0.9, 0.15, 0.1)
    } else if suspicion >= Eye::INVESTIGATE {
        Color::srgb(0.95, 0.5, 0.1)
    } else if suspicion >= Eye::NOTICE {
        Color::srgb(0.95, 0.8, 0.2)
    } else {
        Color::srgb(0.7, 0.75, 0.8)
    }
}

/// One strike as the bars draw it.
#[derive(Clone, Copy, Debug)]
struct StrikeBar {
    /// The track's top centre, px.
    at: Vec2,
    progress: f32,
    dodge_from: f32,
    parry_from: f32,
    mine: bool,
    kind: StrikeKind,
}

impl StrikeBar {
    /// The fill has reached the dodge zone: roll now.
    fn dodge_now(&self) -> bool {
        self.progress >= self.dodge_from
    }

    /// The fill's colour: the kind's colour, near-white inside the dodge zone; dimmed for a
    /// strike on someone else.
    fn fill(&self) -> Color {
        let (r, g, b) = match self.kind {
            StrikeKind::Legacy | StrikeKind::Sweep => (0.95, 0.5, 0.1),
            StrikeKind::Thrust => (0.2, 0.65, 1.0),
            StrikeKind::Slam => (0.85, 0.2, 0.85),
        };
        let (r, g, b) = if self.dodge_now() {
            let lift = |c: f32| c + (1.0 - c) * 0.7;
            (lift(r), lift(g), lift(b))
        } else {
            (r * 0.8, g * 0.8, b * 0.8)
        };
        let brightness = if self.mine { 1.0 } else { 0.55 };
        Color::srgb(r * brightness, g * brightness, b * brightness)
    }

    /// The zone's colour: pale, brighter once the fill is in it.
    fn zone(&self) -> Color {
        let alpha = match (self.mine, self.dodge_now()) {
            (false, _) => 0.12,
            (true, false) => 0.3,
            (true, true) => 0.55,
        };
        Color::srgba(1.0, 0.95, 0.6, alpha)
    }

    fn tick(&self) -> Color {
        if self.mine {
            Color::WHITE
        } else {
            Color::srgb(0.55, 0.55, 0.55)
        }
    }

    /// The parry tick: gold, dimmed on someone else's strike.
    fn parry_tick(&self) -> Color {
        if self.mine {
            Color::srgb(0.95, 0.8, 0.2)
        } else {
            Color::srgb(0.55, 0.48, 0.2)
        }
    }
}

fn shown(visible: bool) -> Visibility {
    if visible {
        Visibility::Visible
    } else {
        Visibility::Hidden
    }
}

/// Update the quads from the prediction and the Souls state.
#[allow(clippy::too_many_arguments, clippy::type_complexity)]
pub(super) fn drive_hud(
    time: Res<Time>,
    view: Res<CombatModeView>,
    config: Res<SoulsConfig>,
    kit: Res<PlayerKit>,
    profiles: Res<CombatProfiles>,
    playback: Res<CombatPlaybackState>,
    souls: Res<SoulsState>,
    link: Res<SoulsLink>,
    me: Query<(), With<SelfPlayer>>,
    camera: Query<(&Camera, &GlobalTransform), With<WorldCamera>>,
    targets: Query<&GlobalTransform, Without<HudPart>>,
    plates: Res<VPlates>,
    index: Res<crate::net::GuidIndex>,
    heads: (
        Query<&Transform>,
        Query<&BoneAttach>,
        Query<&benilla_world::rig_anim::RigPose>,
        Query<&OverheadFallback>,
        Query<(), With<crate::entities::mount::MountChild>>,
    ),
    mut parts: Query<(&HudPart, &mut Node, &mut BackgroundColor, &mut Visibility)>,
) {
    let on = souls_on(&view) && config.features.hud && me.single().is_ok();
    let engine = playback.engine.as_ref().filter(|_| on);
    let max = profiles.stamina.map_or(0, |stamina| stamina.max);
    let fraction = match (engine, max) {
        (Some(engine), max) if max > 0 => (engine.stamina() as f32 / max as f32).clamp(0.0, 1.0),
        _ => 0.0,
    };
    let t = time.elapsed_secs();
    let fill = if t < souls.refused_until {
        Color::srgb(0.85, 0.15, 0.1)
    } else if souls.guarding {
        Color::srgb(0.25, 0.5, 0.95)
    } else if souls.sprinting {
        Color::srgb(0.95, 0.8, 0.2)
    } else {
        Color::srgb(0.2, 0.75, 0.25)
    };
    let chip = if link.up {
        Color::srgb(0.2, 0.75, 0.75)
    } else if current_kit(&kit, &profiles).is_some() {
        Color::srgb(0.85, 0.7, 0.2)
    } else {
        Color::srgb(0.5, 0.5, 0.5)
    };
    let project = |at: Vec3| {
        let (camera, view_at) = camera.single().ok()?;
        camera.world_to_viewport(view_at, at).ok()
    };
    // The strike bars: each telegraphed strike under its unit's plate, which hangs from its top
    // centre 2/3 yd over the overhead anchor; with no plate, under the anchor (the name sits over
    // it). `at` is the bar's top centre.
    let plate_h = camera
        .single()
        .ok()
        .and_then(|(camera, _)| camera.logical_viewport_size())
        .map_or(0.0, |viewport| gx_px(PLATE_H, plate_basis(viewport)));
    let (units, attach, poses, fallbacks, mounts) = &heads;
    let strikes: Vec<StrikeBar> = link
        .shown
        .iter()
        .filter(|_| on)
        .filter_map(|strike| {
            let tf = units.get(strike.unit).ok()?;
            let head = overhead_anchor(strike.unit, tf, attach, poses, fallbacks, &targets, mounts);
            let at = if plates.0.contains(&strike.unit) {
                project(head + Vec3::Y * PLATE_LIFT)? + Vec2::Y * (plate_h + STRIKE_GAP)
            } else {
                project(head)? + Vec2::Y * STRIKE_GAP
            };
            Some(StrikeBar {
                at,
                progress: strike.progress,
                dodge_from: strike.dodge_from,
                parry_from: strike.parry_from,
                mine: strike.mine,
                kind: strike.kind,
            })
        })
        .take(usize::from(STRIKE_BARS))
        .collect();
    // The stance (poise) bar: over the plate of the creature whose stance last moved, while it
    // is below full and the reading is fresh.
    let poise = link
        .poise
        .filter(|poise| on && poise.current < poise.max && t - poise.at < POISE_SHOW_SECONDS)
        .and_then(|poise| {
            let entity = index.0.get(&poise.creature).copied()?;
            let tf = units.get(entity).ok()?;
            let head = overhead_anchor(entity, tf, attach, poses, fallbacks, &targets, mounts);
            let at = if plates.0.contains(&entity) {
                project(head + Vec3::Y * PLATE_LIFT)? + Vec2::Y * (plate_h + POISE_GAP)
            } else {
                project(head)? + Vec2::Y * POISE_GAP
            };
            Some((at, poise.ratio()))
        });
    let broken = link
        .broken
        .and_then(|(guid, _)| index.0.get(&guid).copied());
    let lock_color = if broken.is_some() || link.riposte.is_some() {
        Color::srgb(0.95, 0.8, 0.2)
    } else {
        Color::srgb(0.9, 0.15, 0.1)
    };
    // The marker: the locked unit projected through the world camera.
    let marker = broken
        .or(souls.lock)
        .filter(|_| on)
        .and_then(|entity| targets.get(entity).ok())
        .and_then(|at| project(at.translation() + Vec3::Y * MARKER_HEIGHT_YARDS));

    // The eye meter, while crouched: the track's colour by the stealth (white for a sneak
    // attack's flash), the fill by the suspicion, and the dot over the creature holding it.
    let eye = link.eye.filter(|eye| on && eye.mode != EyeMode::Standing);
    let flash = on && link.sneak.is_some();
    let eye_track = if flash {
        Color::WHITE
    } else if eye.is_some_and(|eye| eye.mode == EyeMode::Seen) {
        Color::srgba(0.35, 0.05, 0.05, 0.75)
    } else {
        Color::srgba(0.05, 0.08, 0.12, 0.75)
    };
    let suspicion = eye.map_or(0, |eye| eye.suspicion);
    let eye_mark = eye
        .filter(|eye| eye.suspicion > 0 && eye.creature != 0)
        .and_then(|eye| index.0.get(&eye.creature).copied())
        .and_then(|entity| targets.get(entity).ok())
        .and_then(|at| project(at.translation() + Vec3::Y * EYE_MARK_HEIGHT_YARDS));

    for (part, mut node, mut color, mut visibility) in &mut parts {
        let (visible, wanted) = match part {
            HudPart::Bar => (on && engine.is_some(), None),
            HudPart::Fill => (true, Some(fill)),
            HudPart::Chip => (on, Some(chip)),
            HudPart::Lock => (marker.is_some(), Some(lock_color)),
            HudPart::Strike(index) => (strikes.get(usize::from(*index)).is_some(), None),
            HudPart::StrikeZone(index) => {
                (true, strikes.get(usize::from(*index)).map(StrikeBar::zone))
            }
            HudPart::StrikeFill(index) => {
                (true, strikes.get(usize::from(*index)).map(StrikeBar::fill))
            }
            HudPart::StrikeTick(index) => {
                (true, strikes.get(usize::from(*index)).map(StrikeBar::tick))
            }
            HudPart::StrikeParry(index) => (
                true,
                strikes.get(usize::from(*index)).map(StrikeBar::parry_tick),
            ),
            HudPart::Poise => (poise.is_some(), None),
            HudPart::PoiseFill => (true, None),
            HudPart::Eye => (eye.is_some() || flash, Some(eye_track)),
            HudPart::EyeFill => (true, Some(eye_fill(suspicion))),
            HudPart::EyeMark => (eye_mark.is_some(), Some(eye_fill(suspicion))),
        };
        let child = matches!(
            part,
            HudPart::Fill
                | HudPart::StrikeZone(_)
                | HudPart::StrikeFill(_)
                | HudPart::StrikeTick(_)
                | HudPart::StrikeParry(_)
                | HudPart::PoiseFill
                | HudPart::EyeFill
        );
        let visible = if child {
            Visibility::Inherited
        } else {
            shown(visible)
        };
        if *visibility != visible {
            *visibility = visible;
        }
        if let Some(wanted) = wanted {
            if color.0 != wanted {
                color.0 = wanted;
            }
        }
        match part {
            HudPart::Fill => {
                let width = Val::Percent(fraction * 100.0);
                if node.width != width {
                    node.width = width;
                }
            }
            HudPart::Lock => {
                if let Some(at) = marker {
                    let (left, top) = (Val::Px(at.x - MARKER / 2.0), Val::Px(at.y - MARKER / 2.0));
                    if node.left != left || node.top != top {
                        node.left = left;
                        node.top = top;
                    }
                }
            }
            HudPart::Strike(index) => {
                if let Some(strike) = strikes.get(usize::from(*index)) {
                    let left = Val::Px(strike.at.x - STRIKE_WIDTH / 2.0);
                    let top = Val::Px(strike.at.y);
                    if node.left != left || node.top != top {
                        node.left = left;
                        node.top = top;
                    }
                }
            }
            HudPart::StrikeZone(index) => {
                if let Some(strike) = strikes.get(usize::from(*index)) {
                    let from = strike.dodge_from.clamp(0.0, 1.0) * 100.0;
                    let (left, width) = (Val::Percent(from), Val::Percent(100.0 - from));
                    if node.left != left || node.width != width {
                        node.left = left;
                        node.width = width;
                    }
                }
            }
            HudPart::StrikeFill(index) => {
                if let Some(strike) = strikes.get(usize::from(*index)) {
                    let width = Val::Percent(strike.progress.clamp(0.0, 1.0) * 100.0);
                    if node.width != width {
                        node.width = width;
                    }
                }
            }
            HudPart::StrikeTick(index) => {
                if let Some(strike) = strikes.get(usize::from(*index)) {
                    let at = strike.dodge_from.clamp(0.0, 1.0) * STRIKE_WIDTH;
                    let left =
                        Val::Px((at - STRIKE_TICK / 2.0).clamp(0.0, STRIKE_WIDTH - STRIKE_TICK));
                    if node.left != left {
                        node.left = left;
                    }
                }
            }
            HudPart::StrikeParry(index) => {
                if let Some(strike) = strikes.get(usize::from(*index)) {
                    let at = strike.parry_from.clamp(0.0, 1.0) * STRIKE_WIDTH;
                    let left =
                        Val::Px((at - STRIKE_TICK / 2.0).clamp(0.0, STRIKE_WIDTH - STRIKE_TICK));
                    if node.left != left {
                        node.left = left;
                    }
                }
            }
            HudPart::Poise => {
                if let Some((at, _)) = poise {
                    let left = Val::Px(at.x - POISE_WIDTH / 2.0);
                    let top = Val::Px(at.y);
                    if node.left != left || node.top != top {
                        node.left = left;
                        node.top = top;
                    }
                }
            }
            HudPart::PoiseFill => {
                if let Some((_, ratio)) = poise {
                    let width = Val::Percent(ratio.clamp(0.0, 1.0) * 100.0);
                    if node.width != width {
                        node.width = width;
                    }
                }
            }
            HudPart::EyeFill => {
                let width = Val::Percent(f32::from(suspicion.min(100)));
                if node.width != width {
                    node.width = width;
                }
            }
            HudPart::EyeMark => {
                if let Some(at) = eye_mark {
                    let (left, top) = (
                        Val::Px(at.x - EYE_MARK / 2.0),
                        Val::Px(at.y - EYE_MARK / 2.0),
                    );
                    if node.left != left || node.top != top {
                        node.left = left;
                        node.top = top;
                    }
                }
            }
            HudPart::Bar | HudPart::Chip | HudPart::Eye => {}
        }
    }
}
