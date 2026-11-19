//! The Souls HUD: the predicted stamina bar (bottom centre), a mode chip at its left end, a
//! marker on the locked-on unit, and a windup bar under each striking enemy's nameplate (under
//! its head when it has none), clear of its name ([`super::server`]). Plain coloured quads above the world (no text yet). Hidden when Souls
//! combat is off, the `hud` switch is off, or there is no local player.
//!
//! The bar is green; red for a moment after a press refused for stamina, blue while guarding,
//! yellow while sprinting. The chip is gold with the kit's table, grey on the stock fallback; it
//! turns teal while the server script answers. The marker is red, gold while a riposte is open. A
//! strike bar fills over its windup: orange for a strike on us (red in its last fifth), grey for
//! one on someone else.

use benilla_world::view::WorldCamera;
use bevy::prelude::*;

use super::server::SoulsLink;
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
const STRIKE_WIDTH: f32 = 56.0;
const STRIKE_HEIGHT: f32 = 6.0;
const STRIKE_GAP: f32 = 3.0;

/// One HUD quad.
#[derive(Component, Clone, Copy, PartialEq, Eq, Debug)]
pub(super) enum HudPart {
    Bar,
    Fill,
    Chip,
    Lock,
    /// The n-th strike bar.
    Strike(u8),
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
    for index in 0..STRIKE_BARS {
        commands.spawn((
            HudPart::Strike(index),
            Node {
                position_type: PositionType::Absolute,
                width: Val::Px(0.0),
                height: Val::Px(STRIKE_HEIGHT),
                ..default()
            },
            BackgroundColor(Color::srgb(0.95, 0.5, 0.1)),
            GlobalZIndex(900),
            Visibility::Hidden,
        ));
    }
}

/// A strike bar's colour.
fn strike_color(mine: bool, progress: f32) -> Color {
    match (mine, progress >= 0.8) {
        (true, true) => Color::srgb(0.95, 0.12, 0.08),
        (true, false) => Color::srgb(0.95, 0.5, 0.1),
        (false, _) => Color::srgb(0.6, 0.6, 0.6),
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
#[allow(clippy::too_many_arguments)]
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
    let strikes: Vec<(Vec2, f32, bool)> = link
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
            Some((at, strike.progress, strike.mine))
        })
        .take(usize::from(STRIKE_BARS))
        .collect();
    let lock_color = if link.riposte.is_some() {
        Color::srgb(0.95, 0.8, 0.2)
    } else {
        Color::srgb(0.9, 0.15, 0.1)
    };
    // The marker: the locked unit projected through the world camera.
    let marker = souls
        .lock
        .filter(|_| on)
        .and_then(|entity| targets.get(entity).ok())
        .and_then(|at| project(at.translation() + Vec3::Y * MARKER_HEIGHT_YARDS));

    for (part, mut node, mut color, mut visibility) in &mut parts {
        let (visible, wanted) = match part {
            HudPart::Bar => (on && engine.is_some(), None),
            HudPart::Fill => (true, Some(fill)),
            HudPart::Chip => (on, Some(chip)),
            HudPart::Lock => (marker.is_some(), Some(lock_color)),
            HudPart::Strike(index) => {
                let strike = strikes.get(usize::from(*index));
                (
                    strike.is_some(),
                    strike.map(|&(_, progress, mine)| strike_color(mine, progress)),
                )
            }
        };
        let visible = if *part == HudPart::Fill {
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
                if let Some(&(at, progress, _)) = strikes.get(usize::from(*index)) {
                    let left = Val::Px(at.x - STRIKE_WIDTH / 2.0);
                    let top = Val::Px(at.y);
                    let width = Val::Px(STRIKE_WIDTH * progress.max(0.05));
                    if node.left != left || node.top != top || node.width != width {
                        node.left = left;
                        node.top = top;
                        node.width = width;
                    }
                }
            }
            HudPart::Bar | HudPart::Chip => {}
        }
    }
}
