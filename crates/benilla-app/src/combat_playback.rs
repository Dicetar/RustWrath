//! The runtime half of the Souls action playback: fills the compiler's profile table for the
//! local player, feeds requests through the core [`ActionEngine`], and plays each started
//! action's clip through the existing one-shot driver path.
//!
//! **Presentation-only prediction.** Nothing here sends traffic, grants i-frames or resolves a
//! hit. The engine is the same pure rules surface the authoritative server will run; the client
//! runs it as a prediction and the authoritative seam (`combat_mode.rs`) keeps refusing with
//! `NotSent` until a server mode channel exists. `CombatModeView::effective` is never written
//! here.
//!
//! Provisional binds (until the RFC's rebindable experimental controls land): `F10` toggles the
//! client's *requested* mode, `V` requests a dodge, `C` requests a light attack. All are inert
//! with no local player (glue screens); actions only play once Experimental is requested.

use std::collections::HashMap;

use bevy::prelude::*;
use benilla_assets::ModelAnimations;
use benilla_combat_core::{
    dodge_action, ActionEngine, ActionId, ActionRefusal, ActionSpec, ActionTable, EngineEvent,
    ProfilePolicy, StaminaConfig, TickRate, Ticks,
};

use crate::combat_mode::{CombatMode, CombatModeView};
use crate::combat_profiles::{
    player_dodge_spec, player_melee_action, CombatProfiles, HOST_DODGE, PLAYER_MELEE_MAIN,
};
use crate::creature_anim::{ActionAnim, AnimData, PlaySeq, Wielded};
use crate::net::SelfPlayer;

/// The first authored action-table revision. The server-negotiated revision replaces it once
/// the mode channel exists; every consumer pins this value per session.
pub(crate) const PROFILE_REVISION: benilla_combat_core::ActionRevision =
    benilla_combat_core::ActionRevision(1);
/// The compile rate: authored 100 Hz pending the server's authoritative tick rate.
pub(crate) const PROFILE_TICK_RATE_HZ: u16 = 100;
/// The authored predicted-stamina policy: full bar, one point per tick, 100 ticks (one second at
/// the compile rate) before regeneration resumes. All three are Project B policy, not extracted
/// values; the server replaces them with its own once the seam exists.
pub(crate) const PREDICTED_STAMINA: StaminaConfig = StaminaConfig {
    max: 100,
    regen_per_tick: 1,
    regen_delay_ticks: 100,
};
/// Ticks a held request stays alive: half a second at the compile rate, authored.
pub(crate) const BUFFER_TTL_TICKS: u64 = 50;

/// Where a predicted action is in its compiled profile.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub(crate) enum ActionPhase {
    /// Before the contact (or i-frame) window opens.
    Windup,
    /// Inside the half-open contact/active window: a melee strike, a spell release, i-frames.
    Active,
    /// After the active window, before the action ends.
    Recovery,
    /// Past the action's total.
    Finished,
}

/// The phase of `spec` at `elapsed` ticks. The active window is the i-frame window for a dodge
/// and the contact window for everything else; both are half-open, matching the profile.
pub(crate) fn phase_of(spec: &ActionSpec, elapsed: u64) -> ActionPhase {
    if elapsed >= spec.total_ticks {
        return ActionPhase::Finished;
    }
    let active = match spec.kind {
        benilla_combat_core::ActionKind::Dodge => spec.iframes,
        _ => spec.contact,
    };
    match active {
        Some(window) if elapsed < window.from => ActionPhase::Windup,
        Some(window) if window.contains(elapsed) => ActionPhase::Active,
        _ => ActionPhase::Recovery,
    }
}

/// The playback read model: the core engine (prediction) plus the playback clock. Every field
/// is presentation; the engine's authority note applies — the server owns the real outcomes.
#[derive(Resource, Debug, Default)]
pub(crate) struct CombatPlaybackState {
    pub(crate) engine: Option<ActionEngine>,
    pub(crate) last_refusal: Option<ActionRefusal>,
    /// The playback clock in milliseconds, advanced from `Time` each frame.
    pub(crate) now_ms: u64,
}

impl CombatPlaybackState {
    /// Whole ticks at `now_ms` on the compile clock.
    fn ticks_at(&self) -> Ticks {
        Ticks::new(self.now_ms * u64::from(PROFILE_TICK_RATE_HZ) / 1000)
    }

    /// The running action's phase, if any.
    pub(crate) fn phase(&self, profiles: &CombatProfiles) -> Option<ActionPhase> {
        let engine = self.engine.as_ref()?;
        let id = engine.active()?;
        let elapsed = engine.active_elapsed(self.ticks_at())?;
        let spec = profiles.table.as_ref()?.get(id)?;
        Some(phase_of(spec, elapsed))
    }
}

pub(crate) struct CombatPlaybackPlugin;

impl Plugin for CombatPlaybackPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<CombatProfiles>()
            .init_resource::<CombatPlaybackState>()
            .add_systems(
                Update,
                (
                    fill_local_profiles,
                    advance_playback,
                    toggle_and_stage_action,
                )
                    .chain(),
            );
    }
}

/// Compile the local player's profile table when the rig or the main-hand weapon changes, then
/// rebuild the prediction engine over it. The table carries the dodge and the main-hand melee
/// action; both compile from the rig's own clips (see `combat_profiles.rs`) and refuse rather
/// than invent a profile when a clip is missing.
fn fill_local_profiles(
    me: Query<(Entity, &ModelAnimations, &Wielded), With<SelfPlayer>>,
    anim_data: Option<Res<AnimData>>,
    mut profiles: ResMut<CombatProfiles>,
    mut state: ResMut<CombatPlaybackState>,
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
    state.engine = ActionEngine::new(table.clone(), PREDICTED_STAMINA, BUFFER_TTL_TICKS).ok();
    state.last_refusal = None;
    profiles.table = Some(table);
    profiles.anims = bindings;
    profiles.built_for = Some((entity, main));
}

/// Advance the playback clock and the engine; every action the engine starts this frame plays
/// its compiled clip through the driver.
fn advance_playback(
    time: Res<Time>,
    profiles: Res<CombatProfiles>,
    mut state: ResMut<CombatPlaybackState>,
    mut play_seq: ResMut<PlaySeq>,
    me: Query<Entity, With<SelfPlayer>>,
    mut actions: MessageWriter<ActionAnim>,
) {
    state.now_ms = state
        .now_ms
        .saturating_add(time.delta().as_millis() as u64);
    let Ok(entity) = me.single() else {
        return;
    };
    let now = state.ticks_at();
    let Some(engine) = state.engine.as_mut() else {
        return;
    };
    let Ok(events) = engine.advance_to(now) else {
        return;
    };
    for event in events {
        if let EngineEvent::Started { id, .. } = event {
            if let Some(&anim_id) = profiles.anims.get(&id) {
                actions.write(ActionAnim {
                    entity,
                    anim_id,
                    seq: play_seq.next(),
                });
            }
        }
    }
}

/// The provisional binds: `F10` toggles the client's requested mode (never the effective one);
/// `V` requests a dodge and `C` a light attack while Experimental is requested. Requests go
/// through the engine, so buffering, cancel windows, stamina and refusals are the same rules the
/// server will run.
fn toggle_and_stage_action(
    keys: Res<ButtonInput<KeyCode>>,
    me: Query<Entity, With<SelfPlayer>>,
    mut view: ResMut<CombatModeView>,
    profiles: Res<CombatProfiles>,
    mut state: ResMut<CombatPlaybackState>,
    mut play_seq: ResMut<PlaySeq>,
    mut actions: MessageWriter<ActionAnim>,
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
    let action = if keys.just_pressed(KeyCode::KeyV) {
        Some(HOST_DODGE)
    } else if keys.just_pressed(KeyCode::KeyC) {
        Some(PLAYER_MELEE_MAIN)
    } else {
        None
    };
    let Some(action) = action else {
        return;
    };
    let now = state.ticks_at();
    let Some(engine) = state.engine.as_mut() else {
        return;
    };
    let Ok(events) = engine.submit(action, now) else {
        return;
    };
    for event in events {
        match event {
            EngineEvent::Started { id, .. } => {
                state.last_refusal = None;
                if let Some(&anim_id) = profiles.anims.get(&id) {
                    actions.write(ActionAnim {
                        entity,
                        anim_id,
                        seq: play_seq.next(),
                    });
                }
            }
            EngineEvent::Refused { reason, .. } => {
                state.last_refusal = Some(reason);
            }
            _ => {}
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use benilla_assets::AnimClip;
    use benilla_combat_core::{ActionKind, TickWindow};
    use bevy::animation::graph::AnimationNodeIndex;
    use bevy::prelude::{Handle, Vec3};
    use benilla_formats::AnimDataCatalog;

    fn rate() -> TickRate {
        TickRate::new(PROFILE_TICK_RATE_HZ).expect("100 Hz")
    }

    fn dodge_spec() -> ActionSpec {
        dodge_action(
            HOST_DODGE,
            &benilla_combat_core::DodgeSpec {
                revision: PROFILE_REVISION,
                total_ticks: 50,
                input_from_tick: 50,
                dodge_from_tick: 50,
                cancel_dodge_from_tick: 20,
                cancel_move_from_tick: 50,
                iframes: TickWindow { from: 10, to: 40 },
                stamina_cost: 25,
            },
        )
    }

    /// The phase helper follows the profile's half-open windows: windup, active (i-frames for a
    /// dodge, contact otherwise), recovery, finished.
    #[test]
    fn phase_helper_follows_the_profile_windows() {
        let spec = dodge_spec();
        assert_eq!(phase_of(&spec, 0), ActionPhase::Windup);
        assert_eq!(phase_of(&spec, 9), ActionPhase::Windup);
        assert_eq!(phase_of(&spec, 10), ActionPhase::Active);
        assert_eq!(phase_of(&spec, 39), ActionPhase::Active);
        assert_eq!(phase_of(&spec, 40), ActionPhase::Recovery);
        assert_eq!(phase_of(&spec, 49), ActionPhase::Recovery);
        assert_eq!(phase_of(&spec, 50), ActionPhase::Finished);
    }

    /// The playback state drives the core engine: a dodge request starts, the clock reaches the
    /// i-frame window, and the predicted stamina reflects the engine's spend — the same rules
    /// the server-side engine will run.
    #[test]
    fn playback_state_runs_the_core_engine() {
        let mut table = ActionTable::new(PROFILE_REVISION, rate());
        table.insert(dodge_spec()).expect("insert dodge");
        let engine =
            ActionEngine::new(table.clone(), PREDICTED_STAMINA, BUFFER_TTL_TICKS).expect("engine");
        let mut state = CombatPlaybackState {
            engine: Some(engine),
            last_refusal: None,
            now_ms: 0,
        };
        let profiles = CombatProfiles {
            table: Some(table),
            anims: HashMap::new(),
            built_for: None,
        };

        let now = state.ticks_at();
        let engine = state.engine.as_mut().expect("engine");
        let events = engine.submit(HOST_DODGE, now).expect("submit");
        assert!(matches!(
            events.first(),
            Some(EngineEvent::Started { id, .. }) if *id == HOST_DODGE
        ));
        assert_eq!(
            state.engine.as_ref().expect("engine").stamina(),
            75,
            "the dodge spent its 25 from 100"
        );
        assert_eq!(state.phase(&profiles), Some(ActionPhase::Windup));

        state.now_ms = 120;
        let now = state.ticks_at();
        state.engine.as_mut().expect("engine").advance_to(now).expect("advance");
        assert_eq!(state.phase(&profiles), Some(ActionPhase::Active));

        state.now_ms = 500;
        let now = state.ticks_at();
        state.engine.as_mut().expect("engine").advance_to(now).expect("advance");
        assert_eq!(state.phase(&profiles), None, "the dodge completed");
    }

    /// The runtime table compiles the same dodge and melee actions the pure compilers produce,
    /// and binds each to the rig's clip — the pair the playback path hands to the driver.
    #[test]
    fn local_table_binds_compiled_actions_to_rig_clips() {
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
        let rig = ModelAnimations {
            graph: Handle::default(),
            clips: vec![clip(0, 1.0), clip(30, 0.5), clip(17, 1.0)],
            hand_close: [None, None],
            playable_animation_lookup: Vec::new(),
            animation_lookup: Vec::new(),
            global_bones: Vec::new(),
            first_seq: None,
            pose: Default::default(),
        };
        let policy = ProfilePolicy::default();
        let catalog = AnimDataCatalog::from_rows([]);

        let dodge = player_dodge_spec(&rig, &catalog, &policy, rate(), PROFILE_REVISION)
            .expect("compile")
            .expect("the rig dodges");
        assert_eq!(dodge.1, 30);
        dodge_action(HOST_DODGE, &dodge.0)
            .validate()
            .expect("valid dodge");
        assert_eq!(
            dodge_action(HOST_DODGE, &dodge.0).kind,
            ActionKind::Dodge
        );

        let melee = player_melee_action(
            PLAYER_MELEE_MAIN,
            Some((2, 0x7)),
            &rig,
            &catalog,
            &policy,
            rate(),
        )
        .expect("compile")
        .expect("the rig swings");
        assert_eq!(melee.1, 17);
        melee.0.validate().expect("valid melee");
    }
}
