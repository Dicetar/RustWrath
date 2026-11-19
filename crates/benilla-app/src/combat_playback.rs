//! The runtime half of the Souls action playback: fills the compiler's profile table for the
//! local player, feeds requests through the core [`ActionEngine`], and plays each started
//! action's clip through the existing one-shot driver path.
//!
//! **Presentation-only prediction.** Nothing here sends traffic or resolves a hit; the i-frames
//! and guard only shield the *prediction* from interruption. The engine is the same pure rules
//! surface the authoritative server will run; the client runs it as a prediction and the authoritative seam (`combat_mode.rs`) keeps refusing with
//! `NotSent` until a server mode channel exists. `CombatModeView::effective` is never written
//! here.
//!
//! The prediction follows the world signals presentation already owns:
//!
//! - **Death.** The local player's own observation (`WrathWorldView::self_observation`, the same
//!   `SelfDeath`/zero-health read `combat_mode.rs` closes its window on) ends the engine once and
//!   clears the refusal, so no new clip starts and no request survives the corpse (a one-shot
//!   already playing ends on its own clock). A revival rebuilds the engine anchored at the
//!   playback clock over the same compiled table (the engine has no revive of its own), with a
//!   full bar.
//! - **Interruption.** A `SwingMessage` that lands on the local player (`victim_state == 1`,
//!   attacker someone else) — the melee stream `creature_anim` publishes per
//!   `SMSG_ATTACKERSTATEUPDATE` — interrupts the running action and drops the held request.
//!   Misses, dodges, parries, blocks and evades change nothing.
//! - **Clock.** The playback clock is a `Duration` accumulated from `Time::delta`; only the tick
//!   conversion truncates, so sub-millisecond frame time is not lost (~4 % slow at 60 fps when
//!   each frame's `as_millis` is summed). Every engine is built with `new_at(ticks_at())`, so its
//!   first advance is anchored at the current tick, and advances past the engine's span bound are
//!   walked in bounded chunks rather than dropped.
//!
//! A profile rebuild (rig load, weapon swap, a new kit) carries the predicted bar across via
//! `ActionEngine::set_stamina` when the stamina config is unchanged, which resumes regeneration
//! after the install delay; a new config and a respawn start with a full bar.
//!
//! **The Souls kit.** With `benilla-config/combat/player.kit` present ([`crate::combat_kit`]),
//! the table is the kit's Elden Ring moveset at its own timings. The clips are stand-ins from the
//! WoW rig until the retarget. Without a kit the stock dodge and main-hand swing play.
//!
//! `F10` toggles the client's *requested* mode and, on the way in, re-reads the kit and
//! `souls.toml`. The presses, root motion, lock-on, guard and hits are [`crate::souls`], which
//! submits through [`submit_and_play`] and writes the [`Defense`] this module's interrupt rule
//! reads (a dodge's i-frames and a raised guard keep a landed hit from interrupting). Off, the
//! defense is reset and every landed hit interrupts as before.

use std::collections::HashMap;
use std::time::Duration;

use benilla_assets::ModelAnimations;
use benilla_combat_core::{
    dodge_action, ActionEngine, ActionId, ActionRefusal, ActionSpec, ActionTable, EngineEvent,
    ProfilePolicy, StaminaConfig, TickRate, Ticks, MAX_ADVANCE_SPAN_TICKS,
};
use benilla_world::schedule::WorldStage;
use bevy::prelude::*;

use crate::combat_kit::{
    clip_bindings, compile_kit, engine_stamina, load_player_kit, PlayerKit, Press,
};
use crate::combat_mode::{CombatMode, CombatModeView};
use crate::combat_profiles::{
    player_dodge_spec, player_melee_action, rig_dodge_clip, rig_swing_clip, CombatProfiles,
    HOST_DODGE, PLAYER_MELEE_MAIN,
};
use crate::creature_anim::{ActionAnim, AnimData, PlaySeq, SwingMessage, Wielded};
use crate::net::SelfPlayer;
use crate::ui_script::{UiInput, UiKeyboardCapture};
use crate::wrath::WrathWorldView;

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
    /// The playback clock. `Time::delta` is a `Duration`; summing whole milliseconds per frame
    /// truncates every frame (about 4 % slow at 60 fps), so the full precision is carried here
    /// and only the tick conversion truncates.
    clock: Duration,
    /// Hit-stop still owed: the playback clock stands still for this much frame time, so the
    /// running action (and its windows) freeze when a Souls hit lands ([`crate::souls`]).
    pub(crate) hit_stop: Duration,
    /// What protects the local player from an incoming hit this frame, written by the Souls
    /// systems; the default (nothing) is the stock rule, every landed hit interrupts.
    pub(crate) defense: Defense,
    /// Landed hits the guard absorbed since the Souls systems last read them.
    pub(crate) guarded_hits: u32,
    /// Landed hits poise absorbed since the Souls systems last read them.
    pub(crate) poised_hits: u32,
}

/// What stands between an incoming landed hit and the running action.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
pub(crate) struct Defense {
    /// The running dodge is inside its i-frame window: the hit does not interrupt.
    pub(crate) iframes: bool,
    /// The guard is up: the hit costs stamina instead of interrupting ([`Self`] counts it in
    /// `guarded_hits` for the Souls systems to charge).
    pub(crate) guarding: bool,
    /// Poise holds: the hit takes poise instead of interrupting ([`Self`] counts it in
    /// `poised_hits`; the Souls systems interrupt when the poise breaks).
    pub(crate) poised: bool,
}

/// The playback chain (profile fill, engine advance, the mode toggle). The Souls input and hit
/// systems run after it, so they read this frame's engine.
#[derive(SystemSet, Clone, PartialEq, Eq, Hash, Debug)]
pub(crate) struct PlaybackSet;

impl CombatPlaybackState {
    /// Advance the playback clock by one frame's delta, keeping the sub-millisecond remainder.
    /// Owed hit-stop is paid first: that part of the frame does not advance the clock.
    fn advance_clock(&mut self, delta: Duration) {
        let frozen = delta.min(self.hit_stop);
        self.hit_stop -= frozen;
        self.clock = self.clock.saturating_add(delta - frozen);
    }

    /// The playback clock in whole milliseconds.
    fn now_ms(&self) -> u64 {
        u64::try_from(self.clock.as_millis()).unwrap_or(u64::MAX)
    }

    /// Whole ticks at `now_ms` on the compile clock.
    pub(crate) fn ticks_at(&self) -> Ticks {
        Ticks::new(self.now_ms() * u64::from(PROFILE_TICK_RATE_HZ) / 1000)
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
            .init_resource::<PlayerKit>()
            .add_systems(Startup, load_player_kit)
            .add_systems(
                Update,
                (
                    crate::combat_kit::select_player_kit,
                    fill_local_profiles,
                    advance_playback,
                    toggle_and_stage_action,
                )
                    .chain()
                    .in_set(PlaybackSet)
                    // The local player is tagged in the Net stage, and the typing gate below
                    // must read this frame's `UiKeyboardCapture`, written by the UI input pass.
                    .after(WorldStage::Net)
                    .after(UiInput),
            );
    }
}

/// Compile the local player's profile table when the rig, the main-hand weapon or the kit file
/// changes, then rebuild the prediction engine over it.
///
/// With a Souls kit ([`crate::combat_kit`]) the table is the kit's, each action bound to the
/// rig's stand-in clip, with the kit's stamina. Without one, or when the kit does not compile,
/// the table carries the stock dodge and main-hand melee action, both compiled from the rig's own
/// clips (see `combat_profiles.rs`), which refuse rather than invent a profile when a clip is
/// missing.
///
/// A compile failure is a real defect (a bad clip, a kit or policy the compiler rejects), so it
/// is reported once per rebuild with the action it names, never swallowed and never repeated per
/// frame. A missing clip is not a failure: it is the compiler's documented `None` gap.
fn fill_local_profiles(
    me: Query<(Entity, &ModelAnimations, &Wielded), With<SelfPlayer>>,
    anim_data: Option<Res<AnimData>>,
    kit: Res<PlayerKit>,
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
    let key = (entity, main, kit.generation);
    if profiles.built_for == Some(key) && profiles.table.is_some() {
        return;
    }
    let Some(rate) = TickRate::new(PROFILE_TICK_RATE_HZ) else {
        return;
    };
    let mut problems: Vec<String> = Vec::new();
    let compiled = match kit.kit.as_ref() {
        Some(player_kit) => match compile_kit(player_kit, PROFILE_REVISION, rate) {
            Ok(table) => {
                let dodge_clip = rig_dodge_clip(anims, &anim_data.0).map(|clip| clip.anim_id);
                let swing_clip = rig_swing_clip(main, anims, &anim_data.0).map(|clip| clip.anim_id);
                Some((
                    table,
                    clip_bindings(player_kit, dodge_clip, swing_clip),
                    engine_stamina(player_kit, rate, PREDICTED_STAMINA),
                ))
            }
            Err(error) => {
                problems.push(format!(
                    "kit `{}`: {error}; the stock profiles stand in",
                    player_kit.label
                ));
                None
            }
        },
        None => None,
    };
    let from_kit = compiled.is_some();
    let (table, bindings, stamina) = compiled.unwrap_or_else(|| {
        let (table, bindings) = stock_table(anims, &anim_data.0, main, rate, &mut problems);
        (table, bindings, PREDICTED_STAMINA)
    });
    // A rebuild anchors the engine at the playback clock, so the first span stays inside the
    // engine's bounded-advance contract (a fresh engine at tick zero could never reach an
    // app-lifetime clock). The bar carries across a rebuild under the same stamina config and
    // resumes regeneration after the install delay; a new config (a kit loaded or dropped) and a
    // respawn start full.
    let carried_stamina = state
        .engine
        .as_ref()
        .filter(|_| profiles.stamina == Some(stamina))
        .map(ActionEngine::stamina);
    match ActionEngine::new_at(table.clone(), stamina, BUFFER_TTL_TICKS, state.ticks_at()) {
        Ok(mut engine) => {
            if let Some(carried) = carried_stamina {
                engine.set_stamina(carried, state.ticks_at());
            }
            state.engine = Some(engine);
        }
        Err(error) => {
            problems.push(format!("engine: {error}"));
            state.engine = None;
        }
    }
    state.last_refusal = None;
    if !problems.is_empty() {
        bevy::log::warn!(
            "souls playback: profile rebuild for {entity:?} (main {main:?}) failed: {}",
            problems.join("; ")
        );
    }
    profiles.table = Some(table);
    profiles.anims = bindings;
    profiles.built_for = Some(key);
    profiles.from_kit = from_kit;
    profiles.stamina = Some(stamina);
}

/// The stock table: the rig's own Dodge clip as [`HOST_DODGE`] and its main-hand swing as
/// [`PLAYER_MELEE_MAIN`], each bound to its clip. Compile failures go to `problems`.
fn stock_table(
    anims: &ModelAnimations,
    anim_data: &benilla_formats::AnimDataCatalog,
    main: Option<(u8, u8)>,
    rate: TickRate,
    problems: &mut Vec<String>,
) -> (ActionTable, HashMap<ActionId, u16>) {
    let policy = ProfilePolicy::default();
    let mut table = ActionTable::new(PROFILE_REVISION, rate);
    let mut bindings = HashMap::new();
    match player_dodge_spec(anims, anim_data, &policy, rate, PROFILE_REVISION) {
        Ok(Some((dodge, anim_id))) => match table.insert(dodge_action(HOST_DODGE, &dodge)) {
            Ok(()) => {
                bindings.insert(HOST_DODGE, anim_id);
            }
            Err(error) => problems.push(format!("dodge {}: {error}", HOST_DODGE.0)),
        },
        // No Dodge clip on this rig: the compiler's named gap, not a failure.
        Ok(None) => {}
        Err(error) => problems.push(format!("dodge {}: {error}", HOST_DODGE.0)),
    }
    match player_melee_action(PLAYER_MELEE_MAIN, main, anims, anim_data, &policy, rate) {
        Ok(Some((spec, anim_id))) => match table.insert(spec) {
            Ok(()) => {
                bindings.insert(PLAYER_MELEE_MAIN, anim_id);
            }
            Err(error) => problems.push(format!("melee {}: {error}", PLAYER_MELEE_MAIN.0)),
        },
        // The rig resolved no swing clip, or no main hand: the compiler's named gap.
        Ok(None) => {}
        Err(error) => problems.push(format!("melee {}: {error}", PLAYER_MELEE_MAIN.0)),
    }
    (table, bindings)
}

/// The local player's own observation says they are dead. The same signal `combat_mode.rs`
/// closes its Souls window on: the replicated `SelfDeath` flag, or zero observed health.
fn self_is_dead(world: Option<&WrathWorldView>) -> bool {
    world
        .and_then(|world| world.self_observation.as_ref())
        .is_some_and(|observation| {
            matches!(
                observation.death,
                benilla_protocol_wrath::session::SelfDeath::Dead
                    | benilla_protocol_wrath::session::SelfDeath::Ghost
            ) || observation.resources.health == Some(0)
        })
}

/// Advance the engine to an absolute tick in bounded chunks, so a long gap (a load, a hitch, or
/// a freshly anchored engine after a respawn) never trips the engine's span bound. Errors other
/// than the bound are logged once per failure rather than silently dropped.
pub(crate) fn advance_bounded(engine: &mut ActionEngine, target: Ticks) -> Vec<EngineEvent> {
    let mut events = Vec::new();
    loop {
        match engine.advance_to(target) {
            Ok(mut chunk) => {
                events.append(&mut chunk);
                return events;
            }
            Err(benilla_combat_core::CombatError::AdvanceTooLarge { from, .. }) => {
                let step = Ticks::new(from.0.saturating_add(MAX_ADVANCE_SPAN_TICKS));
                match engine.advance_to(step) {
                    Ok(mut chunk) => events.append(&mut chunk),
                    Err(error) => {
                        bevy::log::warn!("souls playback: bounded advance failed: {error}");
                        return events;
                    }
                }
            }
            Err(error) => {
                bevy::log::warn!("souls playback: advance failed: {error}");
                return events;
            }
        }
    }
}

/// Play the clip of every action the engine started in `events`.
pub(crate) fn play_starts(
    events: &[EngineEvent],
    entity: Entity,
    profiles: &CombatProfiles,
    play_seq: &mut PlaySeq,
    actions: &mut MessageWriter<ActionAnim>,
) {
    for event in events {
        if let EngineEvent::Started { id, .. } = event {
            if let Some(&anim_id) = profiles.anims.get(id) {
                actions.write(ActionAnim {
                    entity,
                    anim_id,
                    seq: play_seq.next(),
                });
            }
        }
    }
}

/// Advance the playback clock and the engine; every action the engine starts this frame plays
/// its compiled clip through the driver.
///
/// The prediction hears the two world signals presentation owns: the local player's death (one
/// `die` on the transition, which clears the running action and the held request) and an incoming
/// melee swing that lands on the local player (one `interrupt`; a miss, dodge, parry, block or
/// evade is not a hit and changes nothing). Both are observed outcomes, not authority: nothing
/// here sends or claims a result.
fn advance_playback(
    time: Res<Time>,
    world: Option<Res<WrathWorldView>>,
    profiles: Res<CombatProfiles>,
    mut state: ResMut<CombatPlaybackState>,
    mut play_seq: ResMut<PlaySeq>,
    me: Query<Entity, With<SelfPlayer>>,
    mut swings: MessageReader<SwingMessage>,
    mut actions: MessageWriter<ActionAnim>,
) {
    state.advance_clock(time.delta());
    let Ok(entity) = me.single() else {
        return;
    };
    let now = state.ticks_at();
    if self_is_dead(world.as_deref()) {
        // One `die` per transition: `is_dead` makes the call idempotent. The bounded advance
        // runs first so `die`'s own internal advance never spans more than the engine allows.
        if let Some(engine) = state.engine.as_mut() {
            let _ = advance_bounded(engine, now);
            if !engine.is_dead() {
                let events = match engine.die(now) {
                    Ok(events) => events,
                    Err(error) => {
                        bevy::log::warn!("souls playback: death advance failed: {error}");
                        Vec::new()
                    }
                };
                play_starts(&events, entity, &profiles, &mut play_seq, &mut actions);
            }
        }
        // The refusal outlives the action, so it is cleared here; the running action and the
        // held request are the engine's own and `die` cleared both.
        state.last_refusal = None;
        // This frame's hits land on a corpse and must not interrupt the next life.
        for _ in swings.read() {}
        return;
    }
    // A revival restarts the prediction: the engine has no revive/reset, so a fresh engine
    // anchored at the current tick over the same compiled table keeps playing. The predicted bar
    // refills, which a resurrection owns anyway.
    if state.engine.as_ref().is_none_or(ActionEngine::is_dead) {
        state.engine = None;
        state.last_refusal = None;
        if let Some(table) = profiles.table.clone() {
            let stamina = profiles.stamina.unwrap_or(PREDICTED_STAMINA);
            match ActionEngine::new_at(table, stamina, BUFFER_TTL_TICKS, now) {
                Ok(engine) => state.engine = Some(engine),
                Err(error) => {
                    bevy::log::warn!("souls playback: revive rebuild failed: {error}");
                }
            }
        }
        if state.engine.is_none() {
            return;
        }
    }
    // Only a landed hit interrupts: `victim_state == 1` is the hit outcome; a miss, dodge,
    // parry, block or evade leaves the player's action untouched. Drain every message so no
    // stale swing can interrupt a later action.
    let hits: Vec<SwingMessage> = swings.read().copied().collect();
    let landed = hits
        .iter()
        .filter(|swing| {
            swing.attacker != entity && swing.victim == Some(entity) && swing.victim_state == 1
        })
        .count();
    // The Souls defenses (written last frame, reset whenever Souls combat is off): a dodge's
    // i-frames ignore the hit, a raised guard absorbs it for stamina and holding poise for poise,
    // both charged by the Souls input.
    let defense = state.defense;
    let landed_count = u32::try_from(landed).unwrap_or(u32::MAX);
    let interrupted = landed > 0 && !defense.iframes && !defense.guarding && !defense.poised;
    if landed > 0 && defense.guarding && !defense.iframes {
        state.guarded_hits = state.guarded_hits.saturating_add(landed_count);
    } else if landed > 0 && defense.poised && !defense.iframes {
        state.poised_hits = state.poised_hits.saturating_add(landed_count);
    }
    let Some(engine) = state.engine.as_mut() else {
        return;
    };
    // Interrupt before advancing, so a hit that lands this frame is not played for one tick
    // first. Any action the interruption's own advance started (a free cancel window) still
    // plays its clip.
    if interrupted {
        match engine.interrupt(now) {
            Ok(events) => play_starts(&events, entity, &profiles, &mut play_seq, &mut actions),
            Err(error) => {
                bevy::log::warn!("souls playback: interrupt failed: {error}");
            }
        }
    }
    let events = advance_bounded(engine, now);
    play_starts(&events, entity, &profiles, &mut play_seq, &mut actions);
}

/// The mode toggle: `F10` flips the client's *requested* mode (never the effective one). Entering
/// Souls combat re-reads the kit file and `souls.toml`, so an imported kit or a changed feature
/// switch takes effect without a restart; the next frame's profile build compiles the kit.
/// Leaving it hands everything back to the stock path: every Souls system resets its own state
/// when the requested mode is not Experimental.
///
/// `F10` runs through the binding dispatch's own gate ([`UiKeyboardCapture`]: a focused EditBox
/// eats every key and a shown keyboard frame eats the key it consumed). The action presses live in
/// [`crate::souls`].
fn toggle_and_stage_action(
    keys: Res<ButtonInput<KeyCode>>,
    capture: Res<UiKeyboardCapture>,
    me: Query<(), With<SelfPlayer>>,
    mut view: ResMut<CombatModeView>,
    mut kit: ResMut<PlayerKit>,
    mut souls_config: ResMut<crate::souls::SoulsConfig>,
) {
    let super_down = keys.pressed(KeyCode::SuperLeft) || keys.pressed(KeyCode::SuperRight);
    if me.single().is_err()
        || !keys.just_pressed(KeyCode::F10)
        || super_down
        || capture.typing
        || capture.consumed.contains(&KeyCode::F10)
    {
        return;
    }
    match view.requested {
        Some(CombatMode::Experimental) => {
            view.request_baseline();
            bevy::log::info!("souls combat: off, stock combat restored");
        }
        _ => {
            view.request_experimental();
            kit.reload();
            souls_config.reload();
            bevy::log::info!("souls combat: on");
        }
    }
}

/// Whether the profile table was compiled from the kit `kit` holds now. Until it is (the frame
/// after a reload) the presses are the stock ones.
pub(crate) fn kit_table_current(kit: &PlayerKit, profiles: &CombatProfiles) -> bool {
    kit.kit.is_some()
        && profiles.from_kit
        && profiles
            .built_for
            .is_some_and(|(_, _, generation)| generation == kit.generation)
}

/// The stock action a press stands for when no kit table is compiled.
pub(crate) fn stock_press(press: Press) -> ActionId {
    match press {
        Press::Dodge => HOST_DODGE,
        Press::Light => PLAYER_MELEE_MAIN,
    }
}

/// Submit `action` to the engine at the playback clock and play the clip of whatever it starts.
/// A refusal is kept as `last_refusal`; the engine's events are returned for the caller.
/// Requests go through the engine, so buffering, cancel windows, stamina and refusals are the
/// same rules the server will run.
pub(crate) fn submit_and_play(
    state: &mut CombatPlaybackState,
    action: ActionId,
    entity: Entity,
    profiles: &CombatProfiles,
    play_seq: &mut PlaySeq,
    actions: &mut MessageWriter<ActionAnim>,
) -> Vec<EngineEvent> {
    let now = state.ticks_at();
    let Some(engine) = state.engine.as_mut() else {
        return Vec::new();
    };
    // Keep the span bounded before submit's own advance (normally a no-op: the frame already
    // advanced this tick).
    let mut events = advance_bounded(engine, now);
    match engine.submit(action, now) {
        Ok(mut submitted) => events.append(&mut submitted),
        Err(error) => {
            bevy::log::warn!("souls playback: submit failed: {error}");
            return events;
        }
    }
    play_starts(&events, entity, profiles, play_seq, actions);
    for event in &events {
        if let EngineEvent::Refused { reason, .. } = event {
            state.last_refusal = Some(*reason);
        }
    }
    events
}

/// Play one clip on `entity` outside the engine (the guard's raise, a guarded hit).
pub(crate) fn play_clip(
    anim_id: u16,
    entity: Entity,
    play_seq: &mut PlaySeq,
    actions: &mut MessageWriter<ActionAnim>,
) {
    actions.write(ActionAnim {
        entity,
        anim_id,
        seq: play_seq.next(),
    });
}

#[cfg(test)]
mod tests {
    use super::*;
    use benilla_assets::AnimClip;
    use benilla_combat_core::{ActionKind, TickWindow};
    use benilla_formats::AnimDataCatalog;
    use bevy::animation::graph::AnimationNodeIndex;
    use bevy::prelude::{Handle, Vec3};

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
            ..Default::default()
        };
        let profiles = CombatProfiles {
            table: Some(table),
            ..Default::default()
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

        state.clock = Duration::from_millis(120);
        let now = state.ticks_at();
        state
            .engine
            .as_mut()
            .expect("engine")
            .advance_to(now)
            .expect("advance");
        assert_eq!(state.phase(&profiles), Some(ActionPhase::Active));

        state.clock = Duration::from_millis(500);
        let now = state.ticks_at();
        state
            .engine
            .as_mut()
            .expect("engine")
            .advance_to(now)
            .expect("advance");
        assert_eq!(state.phase(&profiles), None, "the dodge completed");
    }

    /// The playback clock keeps the sub-millisecond remainder across frames. Summing each
    /// frame's `as_millis` (the old path) loses most of a millisecond per 60 fps frame and runs
    /// the clock seconds short per minute; the `Duration` clock only truncates for the tick
    /// conversion.
    #[test]
    fn the_playback_clock_keeps_submillisecond_time() {
        let mut state = CombatPlaybackState::default();
        let frame = Duration::from_nanos(16_666_667);
        for _ in 0..600 {
            state.advance_clock(frame);
        }
        assert_eq!(
            state.now_ms(),
            10_000,
            "600 frames of 16.666667 ms is 10.0000002 s, not 600 truncated 16 ms"
        );
        assert_eq!(
            state.ticks_at(),
            Ticks::new(1000),
            "ten seconds on the 100 Hz compile clock"
        );
        // Truncating per frame would be 600 × 16 ms = 9600 ms: the drift this replaces.
        assert_ne!(state.now_ms(), 600 * 16);
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
        assert_eq!(dodge_action(HOST_DODGE, &dodge.0).kind, ActionKind::Dodge);

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
