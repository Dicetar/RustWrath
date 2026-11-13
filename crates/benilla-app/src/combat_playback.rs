//! The runtime half of the Souls action playback: fills the compiler's profile table for the
//! local player, plays a requested action's clip through the existing one-shot driver path, and
//! tracks the compiled profile as a **predicted** action clock.
//!
//! **Presentation-only prediction.** Nothing here sends traffic, grants i-frames or resolves a
//! hit. The clock, the buffered input and the stamina bar are predictions the owner can feel;
//! the authoritative seam (`combat_mode.rs`) keeps refusing with `NotSent` until a server mode
//! channel exists, and `CombatModeView::effective` is never written here.
//!
//! Provisional binds (until the RFC's rebindable experimental controls land): `F10` toggles the
//! client's *requested* mode, `V` requests a dodge, `C` requests a light attack. All are inert
//! with no local player (glue screens); actions only play once Experimental is requested.

use std::collections::HashMap;

use bevy::prelude::*;
use benilla_assets::ModelAnimations;
use benilla_combat_core::{
    dodge_action, ActionId, ActionKind, ActionRevision, ActionSpec, ActionTable,
    ProfilePolicy, StaminaConfig, TickRate,
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
/// The authored predicted-stamina policy: full bar, one point per tick, 100 ticks (one second at
/// the compile rate) before regeneration resumes. All three are Project B policy, not extracted
/// values; the server replaces them with its own once the seam exists.
pub(crate) const PREDICTED_STAMINA: StaminaConfig = StaminaConfig {
    max: 100,
    regen_per_tick: 1,
    regen_delay_ticks: 100,
};

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

/// The one action instance being presented, on the profile's own tick clock.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub(crate) struct PredictedAction {
    pub(crate) spec: ActionSpec,
    /// When the action started, in the playback clock's milliseconds.
    pub(crate) started_ms: u64,
}

impl PredictedAction {
    /// Ticks elapsed at `now_ms`, floored: each tick's window opens at its own start.
    fn elapsed_ticks(&self, now_ms: u64, rate: TickRate) -> u64 {
        let tick_ms = 1000 / u64::from(rate.hz()).max(1);
        now_ms.saturating_sub(self.started_ms) / tick_ms.max(1)
    }

    /// The phase at `now_ms`. The active window is the i-frame window for a dodge and the
    /// contact window for everything else; both are half-open, matching the profile's windows.
    pub(crate) fn phase(&self, now_ms: u64, rate: TickRate) -> ActionPhase {
        let elapsed = self.elapsed_ticks(now_ms, rate);
        if elapsed >= self.spec.total_ticks {
            return ActionPhase::Finished;
        }
        let active = match self.spec.kind {
            ActionKind::Dodge => self.spec.iframes,
            _ => self.spec.contact,
        };
        match active {
            Some(window) if elapsed < window.from => ActionPhase::Windup,
            Some(window) if window.contains(elapsed) => ActionPhase::Active,
            _ => ActionPhase::Recovery,
        }
    }

    /// Whether a buffered action may replace this one at `now_ms`.
    pub(crate) fn cancel_ready(&self, now_ms: u64, rate: TickRate) -> bool {
        self.spec
            .cancel_from_ticks
            .is_some_and(|from| self.elapsed_ticks(now_ms, rate) >= from)
    }

    /// Whether the action has run past its total.
    pub(crate) fn finished(&self, now_ms: u64, rate: TickRate) -> bool {
        self.elapsed_ticks(now_ms, rate) >= self.spec.total_ticks
    }
}

/// The predicted stamina bar: the donor's rules — eligibility is `stamina > 0`, a spend clamps
/// at zero, and regeneration resumes after the policy delay.
#[derive(Clone, Copy, Debug)]
pub(crate) struct PredictedStamina {
    pub(crate) current: u32,
    /// Playback-clock ms of the last spend.
    last_spend_ms: Option<u64>,
    /// Playback-clock ms up to which regeneration has been applied.
    last_regen_ms: u64,
    config: StaminaConfig,
}

impl Default for PredictedStamina {
    fn default() -> Self {
        Self {
            current: PREDICTED_STAMINA.max,
            last_spend_ms: None,
            last_regen_ms: 0,
            config: PREDICTED_STAMINA,
        }
    }
}

impl PredictedStamina {
    /// Whether a spend is even attempted: the donor requires a non-zero bar, not the full cost.
    pub(crate) fn can_spend(&self) -> bool {
        self.current > 0
    }

    /// Spend `cost`, clamping at zero (the donor's `(stamina - cost).max(0)`).
    pub(crate) fn spend(&mut self, cost: u32, now_ms: u64) {
        self.current = self.current.saturating_sub(cost);
        self.last_spend_ms = Some(now_ms);
        self.last_regen_ms = now_ms;
    }

    /// Apply regeneration for the time elapsed at `now_ms`, honouring the post-spend delay.
    pub(crate) fn regen(&mut self, now_ms: u64, rate: TickRate) {
        let Some(last_spend) = self.last_spend_ms else {
            return;
        };
        let tick_ms = (1000 / u64::from(rate.hz()).max(1)).max(1);
        let delay_ms = self.config.regen_delay_ticks.saturating_mul(tick_ms);
        // Regeneration resumes one delay after the spend; time before that is never banked.
        let from = last_spend.saturating_add(delay_ms).max(self.last_regen_ms);
        if now_ms <= from {
            return;
        }
        let ticks = (now_ms - from) / tick_ms;
        if ticks == 0 {
            return;
        }
        let restored = (ticks as u32).saturating_mul(self.config.regen_per_tick);
        self.current = self.current.saturating_add(restored).min(self.config.max);
        self.last_regen_ms = from + ticks * tick_ms;
    }
}

/// Why a predicted action request did not start.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub(crate) enum PlaybackRefusal {
    /// The mode is not requested (or a glue screen owns the input).
    ModeOff,
    /// The action is not in the compiled table.
    UnknownAction,
    /// The donor's rule: a zero bar refuses the spend; a partial bar clamps.
    InsufficientStamina,
}

/// What one request did.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub(crate) enum PlaybackDecision {
    /// The action started now.
    Started,
    /// An action is running or its cancel window has not opened: held in the one slot.
    Buffered,
    /// Nothing happened; the refusal names why.
    Refused(PlaybackRefusal),
}

/// The playback read model: the predicted action instance, the one buffered request, the
/// predicted bar, and the last refusal. Every field is presentation; none is authoritative.
#[derive(Resource, Debug, Default)]
pub(crate) struct CombatPlaybackState {
    pub(crate) active: Option<PredictedAction>,
    pub(crate) buffered: Option<ActionId>,
    pub(crate) stamina: PredictedStamina,
    pub(crate) last_refusal: Option<PlaybackRefusal>,
    /// The playback clock in milliseconds, advanced from `Time` each frame.
    pub(crate) now_ms: u64,
}

/// One action-instance request: play `action` on `entity`. `instance` identifies this play for
/// later correlation; presentation uses it only to keep the requests distinct.
#[derive(Message, Clone, Copy)]
pub(crate) struct CombatActionRequest {
    pub(crate) entity: Entity,
    pub(crate) action: ActionId,
    pub(crate) instance: u64,
}

/// The compiled spec for one action, if the table holds it.
fn spec_of(profiles: &CombatProfiles, action: ActionId) -> Option<ActionSpec> {
    profiles.table.as_ref()?.get(action).copied()
}

/// Start one predicted action: spend the policy stamina, stamp the clock, and play its clip.
fn start_action(
    state: &mut CombatPlaybackState,
    profiles: &CombatProfiles,
    action: ActionId,
    now_ms: u64,
    play: &mut impl FnMut(u16),
) -> PlaybackDecision {
    let Some(spec) = spec_of(profiles, action) else {
        state.last_refusal = Some(PlaybackRefusal::UnknownAction);
        return PlaybackDecision::Refused(PlaybackRefusal::UnknownAction);
    };
    if !state.stamina.can_spend() {
        state.last_refusal = Some(PlaybackRefusal::InsufficientStamina);
        return PlaybackDecision::Refused(PlaybackRefusal::InsufficientStamina);
    }
    state.stamina.spend(spec.stamina_cost, now_ms);
    state.active = Some(PredictedAction {
        spec,
        started_ms: now_ms,
    });
    if let Some(&anim_id) = profiles.anims.get(&action) {
        play(anim_id);
    }
    PlaybackDecision::Started
}

/// Request one action against the predicted state: start it when idle, otherwise hold it in the
/// one slot (newest wins). The advance pass starts a held request at the cancel window.
fn request_action(
    state: &mut CombatPlaybackState,
    profiles: &CombatProfiles,
    action: ActionId,
    now_ms: u64,
    play: &mut impl FnMut(u16),
) -> PlaybackDecision {
    if spec_of(profiles, action).is_none() {
        state.last_refusal = Some(PlaybackRefusal::UnknownAction);
        return PlaybackDecision::Refused(PlaybackRefusal::UnknownAction);
    }
    match state.active {
        None => start_action(state, profiles, action, now_ms, play),
        Some(_) => {
            state.buffered = Some(action);
            state.last_refusal = None;
            PlaybackDecision::Buffered
        }
    }
}

/// Advance the predicted clock: end a finished action, start a buffered one once its cancel
/// window opens, and regenerate stamina.
fn advance_playback(
    state: &mut CombatPlaybackState,
    profiles: &CombatProfiles,
    now_ms: u64,
    rate: TickRate,
    play: &mut impl FnMut(u16),
) {
    state.stamina.regen(now_ms, rate);
    if let Some(active) = state.active {
        if active.finished(now_ms, rate) {
            state.active = None;
        } else if !active.cancel_ready(now_ms, rate) {
            return;
        }
    }
    let Some(action) = state.buffered.take() else {
        return;
    };
    // Open window (or no action): the held request replaces the running action.
    start_action(state, profiles, action, now_ms, play);
}

pub(crate) struct CombatPlaybackPlugin;

impl Plugin for CombatPlaybackPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<CombatProfiles>()
            .init_resource::<CombatPlaybackState>()
            .add_message::<CombatActionRequest>()
            .add_systems(
                Update,
                (
                    fill_local_profiles,
                    advance_playback_system,
                    toggle_and_stage_dodge,
                    play_combat_actions,
                )
                    .chain(),
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

/// Advance the playback clock and the predicted state; started actions play through the driver.
fn advance_playback_system(
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
    let Some(rate) = TickRate::new(PROFILE_TICK_RATE_HZ) else {
        return;
    };
    let now = state.now_ms;
    let mut starts: Vec<u16> = Vec::new();
    advance_playback(&mut state, &profiles, now, rate, &mut |anim_id| {
        starts.push(anim_id)
    });
    for anim_id in starts {
        actions.write(ActionAnim {
            entity,
            anim_id,
            seq: play_seq.next(),
        });
    }
}

/// The provisional binds: `F10` toggles the client's requested mode (never the effective one)
/// and `V` requests a dodge while Experimental is requested.
fn toggle_and_stage_dodge(
    keys: Res<ButtonInput<KeyCode>>,
    me: Query<Entity, With<SelfPlayer>>,
    mut view: ResMut<CombatModeView>,
    profiles: Res<CombatProfiles>,
    mut state: ResMut<CombatPlaybackState>,
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
    if !keys.just_pressed(KeyCode::KeyV) && !keys.just_pressed(KeyCode::KeyC) {
        return;
    }
    let action = if keys.just_pressed(KeyCode::KeyV) {
        HOST_DODGE
    } else {
        PLAYER_MELEE_MAIN
    };
    let now = state.now_ms;
    let decision = request_action(&mut state, &profiles, action, now, &mut |_| {});
    if decision == PlaybackDecision::Started {
        *instances += 1;
        requests.write(CombatActionRequest {
            entity,
            action,
            instance: *instances,
        });
    }
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
                iframes: benilla_combat_core::TickWindow { from: 10, to: 40 },
                stamina_cost: 25,
            },
        )
    }

    fn table_with(spec: ActionSpec) -> CombatProfiles {
        let mut table = ActionTable::new(PROFILE_REVISION, rate());
        table.insert(spec).expect("insert");
        let mut anims = HashMap::new();
        anims.insert(spec.id, 30u16);
        CombatProfiles {
            table: Some(table),
            anims,
            built_for: None,
        }
    }

    /// The phase follows the half-open i-frame window: windup before 10, active 10..40,
    /// recovery 40..50, finished at 50 — the profile's own boundaries, in wall time.
    #[test]
    fn predicted_phase_follows_the_iframe_window() {
        let action = PredictedAction {
            spec: dodge_spec(),
            started_ms: 1_000,
        };
        // 100 Hz => one tick is 10 ms.
        assert_eq!(action.phase(1_000, rate()), ActionPhase::Windup);
        assert_eq!(action.phase(1_099, rate()), ActionPhase::Windup);
        assert_eq!(action.phase(1_100, rate()), ActionPhase::Active);
        assert_eq!(action.phase(1_399, rate()), ActionPhase::Active);
        assert_eq!(action.phase(1_400, rate()), ActionPhase::Recovery);
        assert_eq!(action.phase(1_499, rate()), ActionPhase::Recovery);
        assert_eq!(action.phase(1_500, rate()), ActionPhase::Finished);
        assert!(action.finished(1_500, rate()));
        assert!(!action.finished(1_499, rate()));
        assert!(!action.cancel_ready(1_199, rate()), "cancel opens at tick 20");
        assert!(action.cancel_ready(1_200, rate()));
    }

    /// The donor's stamina rules: eligibility needs a non-zero bar, a spend clamps at zero, and
    /// regeneration waits for the policy delay.
    #[test]
    fn predicted_stamina_spends_clamps_and_delays_regen() {
        let mut bar = PredictedStamina::default();
        assert_eq!(bar.current, PREDICTED_STAMINA.max);
        bar.spend(30, 0);
        assert_eq!(bar.current, 70);
        bar.spend(200, 100);
        assert_eq!(bar.current, 0, "the spend clamps at zero");
        assert!(!bar.can_spend());

        // The delay: 100 ticks at 10 ms = 1000 ms from the last spend; the delay period itself is
        // never banked, so the first tick of regeneration lands one tick after it ends.
        bar.regen(1_099, rate());
        assert_eq!(bar.current, 0, "regeneration has not started");
        bar.regen(1_100, rate());
        assert_eq!(bar.current, 0, "the first tick has not elapsed yet");
        bar.regen(1_110, rate());
        assert_eq!(bar.current, 1, "one tick of regeneration");
        bar.regen(2_100, rate());
        assert_eq!(bar.current, 100, "regeneration caps at the maximum");
    }

    /// A request starts when idle, buffers when busy (newest wins), and a buffered request
    /// starts only once the running action's cancel window opens.
    #[test]
    fn requests_start_buffer_and_consume_at_the_cancel_window() {
        let spec = dodge_spec();
        let profiles = table_with(spec);
        let mut state = CombatPlaybackState::default();
        let played = std::cell::Cell::new(0usize);
        let mut play = |_: u16| played.set(played.get() + 1);

        assert_eq!(
            request_action(&mut state, &profiles, HOST_DODGE, 0, &mut play),
            PlaybackDecision::Started
        );
        assert_eq!(played.get(), 1);
        assert_eq!(state.stamina.current, 75, "the dodge spent its cost");

        // Busy: the second request is held, not played.
        assert_eq!(
            request_action(&mut state, &profiles, HOST_DODGE, 100, &mut play),
            PlaybackDecision::Buffered
        );
        assert_eq!(played.get(), 1);

        // Before the cancel window (tick 20) the buffer waits.
        advance_playback(&mut state, &profiles, 150, rate(), &mut play);
        assert_eq!(played.get(), 1);
        assert!(state.buffered.is_some());

        // At the cancel window the buffered dodge replaces the running one.
        advance_playback(&mut state, &profiles, 200, rate(), &mut play);
        assert_eq!(played.get(), 2);
        assert!(state.buffered.is_none());
        assert_eq!(state.stamina.current, 50);
    }

    /// A finished action clears the clock, and a zero bar refuses a new start with the typed
    /// reason rather than inventing stamina.
    #[test]
    fn finished_actions_clear_and_an_empty_bar_refuses() {
        let profiles = table_with(dodge_spec());
        let mut state = CombatPlaybackState::default();
        let mut play = |_: u16| {};

        start_action(&mut state, &profiles, HOST_DODGE, 0, &mut play);
        assert!(state.active.is_some());
        advance_playback(&mut state, &profiles, 500, rate(), &mut play);
        assert!(state.active.is_none(), "the action ran past its total");

        state.stamina.spend(200, 600);
        assert_eq!(state.stamina.current, 0);
        assert_eq!(
            request_action(&mut state, &profiles, HOST_DODGE, 600, &mut play),
            PlaybackDecision::Refused(PlaybackRefusal::InsufficientStamina)
        );
        assert_eq!(state.last_refusal, Some(PlaybackRefusal::InsufficientStamina));

        // An action the table does not hold is refused, never invented.
        assert_eq!(
            request_action(&mut state, &profiles, PLAYER_MELEE_MAIN, 600, &mut play),
            PlaybackDecision::Refused(PlaybackRefusal::UnknownAction)
        );
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
        dodge_action(HOST_DODGE, &dodge.0).validate().expect("valid dodge");

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
