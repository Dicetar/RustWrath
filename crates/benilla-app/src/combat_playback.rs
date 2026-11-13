//! The runtime half of the Souls action playback: fills the compiler's profile table for the
//! local player and plays a requested action's clip through the existing one-shot driver path.
//!
//! **Presentation-only prediction.** This module never sends traffic, debits stamina, grants
//! i-frames or resolves a hit. It plays the compiled profile's clip so the owner can feel the
//! action while the authoritative seam (`combat_mode.rs`) keeps refusing with `NotSent` until a
//! server mode channel exists. The `CombatModeView::effective` mode is never written here.
//!
//! Provisional binds (until the RFC's rebindable experimental controls land):
//! `F10` toggles the client's *requested* mode, `V` plays the dodge. Both are inert with no
//! local player (glue screens) and the dodge only plays once Experimental is requested.

use std::collections::HashMap;

use bevy::prelude::*;
use benilla_assets::ModelAnimations;
use benilla_combat_core::{
    dodge_action, ActionId, ActionRevision, ActionTable, ProfilePolicy, TickRate,
};

use crate::combat_mode::{CombatMode, CombatModeView};
use crate::combat_profiles::{
    player_dodge_spec, player_melee_action, CombatProfiles, HOST_DODGE, PLAYER_MELEE_MAIN,
};
use crate::creature_anim::{ActionAnim, AnimData, PlaySeq, Wielded};
use crate::net::SelfPlayer;

/// The first authored action-table revision. The server-negotiated revision replaces it once
/// the mode channel exists; every consumer pins this value per session.
pub(crate) const PROFILE_REVISION: ActionRevision = ActionRevision(1);
/// The compile rate: authored 100 Hz pending the server's authoritative tick rate.
pub(crate) const PROFILE_TICK_RATE_HZ: u16 = 100;

/// One action-instance request: play `action` on `entity`. `instance` identifies this play for
/// later correlation; presentation uses it only to keep the requests distinct.
#[derive(Message, Clone, Copy)]
pub(crate) struct CombatActionRequest {
    pub(crate) entity: Entity,
    pub(crate) action: ActionId,
    pub(crate) instance: u64,
}

pub(crate) struct CombatPlaybackPlugin;

impl Plugin for CombatPlaybackPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<CombatProfiles>()
            .add_message::<CombatActionRequest>()
            .add_systems(
                Update,
                (fill_local_profiles, toggle_and_stage_dodge, play_combat_actions).chain(),
            );
    }
}

/// Compile the local player's profile table when the rig or the main-hand weapon changes. The
/// table carries the dodge and the main-hand melee action; both compile from the rig's own clips
/// (see `combat_profiles.rs`) and refuse rather than invent a profile when a clip is missing.
fn fill_local_profiles(
    me: Query<(Entity, &ModelAnimations, &Wielded), With<SelfPlayer>>,
    anim_data: Option<Res<AnimData>>,
    mut profiles: ResMut<CombatProfiles>,
) {
    let Ok((entity, anims, wielded)) = me.single() else {
        return;
    };
    let Some(anim_data) = anim_data else {
        return;
    };
    let main = wielded.armed_main();
    if profiles.built_for == Some((entity, main)) && profiles.table.is_some() {
        return;
    }
    let Some(rate) = TickRate::new(PROFILE_TICK_RATE_HZ) else {
        return;
    };
    let policy = ProfilePolicy::default();
    let mut table = ActionTable::new(PROFILE_REVISION, rate);
    let mut bindings = HashMap::new();
    if let Ok(Some((dodge, anim_id))) =
        player_dodge_spec(anims, &anim_data.0, &policy, rate, PROFILE_REVISION)
    {
        let action = dodge_action(HOST_DODGE, &dodge);
        if table.insert(action).is_ok() {
            bindings.insert(HOST_DODGE, anim_id);
        }
    }
    if let Ok(Some((spec, anim_id))) =
        player_melee_action(PLAYER_MELEE_MAIN, main, anims, &anim_data.0, &policy, rate)
    {
        if table.insert(spec).is_ok() {
            bindings.insert(PLAYER_MELEE_MAIN, anim_id);
        }
    }
    profiles.table = Some(table);
    profiles.anims = bindings;
    profiles.built_for = Some((entity, main));
}

/// The provisional binds: `F10` toggles the client's requested mode (never the effective one)
/// and `V` stages one dodge request while Experimental is requested.
fn toggle_and_stage_dodge(
    keys: Res<ButtonInput<KeyCode>>,
    me: Query<Entity, With<SelfPlayer>>,
    mut view: ResMut<CombatModeView>,
    mut requests: MessageWriter<CombatActionRequest>,
    mut instances: Local<u64>,
) {
    let Ok(entity) = me.single() else {
        return;
    };
    if keys.just_pressed(KeyCode::F10) {
        match view.requested {
            Some(CombatMode::Experimental) => {
                view.request_baseline();
            }
            _ => {
                view.request_experimental();
            }
        }
    }
    if view.requested != Some(CombatMode::Experimental) {
        return;
    }
    if !keys.just_pressed(KeyCode::KeyV) {
        return;
    }
    *instances += 1;
    requests.write(CombatActionRequest {
        entity,
        action: HOST_DODGE,
        instance: *instances,
    });
}

/// Resolve each request against the compiled table and hand its clip to the animation driver as
/// a one-shot. A request whose action is not in the table (or has no rig binding) plays nothing.
fn play_combat_actions(
    mut requests: MessageReader<CombatActionRequest>,
    profiles: Res<CombatProfiles>,
    mut play_seq: ResMut<PlaySeq>,
    mut actions: MessageWriter<ActionAnim>,
) {
    let Some(table) = profiles.table.as_ref() else {
        return;
    };
    for request in requests.read() {
        if table.get(request.action).is_none() {
            continue;
        }
        let Some(&anim_id) = profiles.anims.get(&request.action) else {
            continue;
        };
        actions.write(ActionAnim {
            entity: request.entity,
            anim_id,
            seq: play_seq.next(),
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use benilla_assets::AnimClip;
    use bevy::animation::graph::AnimationNodeIndex;
    use bevy::prelude::{Handle, Vec3};
    use benilla_formats::AnimDataCatalog;

    /// A test rig carrying Stand + Dodge (30) + Attack1H (17); `duration` in seconds.
    fn rig() -> ModelAnimations {
        let clip = |anim_id: u16, duration: f32| AnimClip {
            anim_id,
            seq_index: 0,
            node: AnimationNodeIndex::new(0),
            blend_node: None,
            looping: false,
            duration,
            move_speed: 0.0,
            blend_time: 0.0,
            bounds_center: Vec3::ZERO,
            bounds_radius: 0.0,
            bounds_min: Vec3::ZERO,
            bounds_max: Vec3::ZERO,
            events: Vec::new().into(),
            arm_nodes: None,
            upper_node: None,
            frequency: 0,
            replay: (0, 0),
            poses_bones: true,
        };
        ModelAnimations {
            graph: Handle::default(),
            clips: vec![clip(0, 1.0), clip(30, 0.5), clip(17, 1.0)],
            hand_close: [None, None],
            playable_animation_lookup: Vec::new(),
            animation_lookup: Vec::new(),
            global_bones: Vec::new(),
            first_seq: None,
            pose: Default::default(),
        }
    }

    /// The runtime table compiles the same dodge and melee actions the pure compilers produce,
    /// and binds each to the rig's clip — the pair the playback path hands to the driver.
    #[test]
    fn local_table_binds_compiled_actions_to_rig_clips() {
        let rig = rig();
        let rate = TickRate::new(PROFILE_TICK_RATE_HZ).expect("100 Hz");
        let policy = ProfilePolicy::default();
        let catalog = AnimDataCatalog::from_rows([]);

        let dodge = player_dodge_spec(&rig, &catalog, &policy, rate, PROFILE_REVISION)
            .expect("compile")
            .expect("the rig dodges");
        assert_eq!(dodge.1, 30);
        let action = dodge_action(HOST_DODGE, &dodge.0);
        action.validate().expect("valid dodge");

        let melee =
            player_melee_action(PLAYER_MELEE_MAIN, Some((2, 0x7)), &rig, &catalog, &policy, rate)
                .expect("compile")
                .expect("the rig swings");
        assert_eq!(melee.1, 17);
        melee.0.validate().expect("valid melee");
    }
}
