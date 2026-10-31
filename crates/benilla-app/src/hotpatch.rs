//! Application configuration and the optional player's diagnostic marker; adapter is reusable.
use benilla_hotpatch_bevy::{HotpatchPlugin, HotpatchProbeTarget};
use benilla_world::rig_anim::RigPose;
use bevy::prelude::*;

pub(crate) fn plugin(app: &mut App) {
    let Some(home) = crate::local_state::home() else {
        return;
    };
    let probe = std::env::var("WOW_HOTPATCH_PROBE").as_deref() == Ok("1");
    app.add_plugins(HotpatchPlugin::new(home.join("Hotpatch")).with_probe(probe));
    if probe {
        app.add_systems(Update, mark_player_rig);
    }
}

fn mark_player_rig(
    mut commands: Commands,
    rigs: Query<Entity, (With<RigPose>, Without<HotpatchProbeTarget>)>,
    parents: Query<&ChildOf>,
    players: Query<(), With<crate::net::SelfPlayer>>,
) {
    for entity in &rigs {
        let mut ancestor = entity;
        for _ in 0..32 {
            if players.contains(ancestor) {
                commands.entity(entity).insert(HotpatchProbeTarget);
                break;
            }
            let Ok(parent) = parents.get(ancestor) else {
                break;
            };
            ancestor = parent.parent();
        }
    }
}
