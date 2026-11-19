//! The Souls presses: dodge (on release; a hold while moving sprints), light, heavy (charged on
//! hold, the uncharged swing when released inside the charge window), the running attacks, the
//! guard and the guard counter, poise and hyperarmor, the crouch and crouch attack, the jump
//! attacks, the left-hand chain and the parry. Every request goes through the engine
//! ([`submit_and_play`]), so buffering, cancel windows, stamina and refusals are its rules.
//!
//! The raw keys run through the binding dispatch's typing gate ([`UiKeyboardCapture`]): a focused
//! EditBox eats every key and a shown keyboard frame eats the key it consumed.

use benilla_combat_core::{ActionId, ActionKind, ActionRefusal, EngineEvent, Kit};
use bevy::prelude::*;

use super::config::{SoulsBind, SoulsFeatures, SoulsKey};
use super::{
    current_kit, in_window, running_action, souls_on, ChargeHold, ErControls, SoulsConfig,
    SoulsState,
};
use crate::bindings::{BindingsState, Input};
use crate::combat_kit::{resolve_dodge, resolve_press, roll_load, PlayerKit, Press, STAMINA_SCALE};
use crate::combat_mode::CombatModeView;
use crate::combat_playback::{
    play_clip, play_starts, stock_press, submit_and_play, ActionPhase, CombatPlaybackState, Defense,
};
use crate::combat_profiles::CombatProfiles;
use crate::creature_anim::{move_flags, ActionAnim, PlaySeq};
use crate::net::SelfPlayer;
use crate::player::Player;
use crate::ui_script::UiKeyboardCapture;

/// Holding dodge this long while moving is a sprint, not a roll (ER's tap/hold split).
const SPRINT_AFTER_SECONDS: f32 = 0.35;
/// The sprint's drain, kit points a second (authored; the bar is the kit's).
const SPRINT_DRAIN_POINTS_PER_SECOND: f32 = 8.0;
/// The movement keys may all be up this long without ending a sprint (a direction change).
const SPRINT_NEUTRAL_GRACE_SECONDS: f32 = 0.2;
/// A guarded hit's stamina cost in kit points until the server sends the attacker's guard damage
/// (authored).
const GUARD_HIT_POINTS: u32 = 25;
/// A guard broken at zero stamina stays down this long.
const GUARD_BREAK_SECONDS: f32 = 1.0;
/// A guarded hit opens the guard counter for this long.
const GUARD_COUNTER_SECONDS: f32 = 1.0;
/// The bar flashes red this long after a press refused for stamina.
const REFUSAL_FLASH_SECONDS: f32 = 0.4;
/// Full poise, authored points (ER's medium-armour range).
const POISE_MAX: f32 = 51.0;
/// What one landed hit takes off poise until the server sends the attacker's poise damage
/// (authored).
const POISE_DAMAGE_PER_HIT: f32 = 20.0;
/// Inside a heavy's windup and swing a hit takes this share of that (hyperarmor).
const HYPERARMOR_POISE_FACTOR: f32 = 0.5;
/// Poise refills this long after the last hit that took it, as ER's does.
const POISE_RESET_SECONDS: f32 = 30.0;
/// `ShieldBlock`, the stand-in clip for raising the guard and taking a guarded hit.
const ANIM_SHIELD_BLOCK: u16 = 24;

/// The raw key state behind the typing gate, read as the Souls binds read it.
///
/// - With the ER layout on, a mouse bind counts only while the layout claims its button: the
///   primary buttons while the camera orbits (with the cursor up they are the UI's again).
/// - A `Shift+` bind's press needs Shift down; a plain bind whose key also has a `Shift+` bind
///   needs Shift up, for its press and its hold. A `Shift+` bind's hold ignores Shift, so letting
///   go of Shift mid-charge keeps the charge.
pub(super) struct KeyRead<'a> {
    keys: &'a ButtonInput<KeyCode>,
    mouse: &'a ButtonInput<MouseButton>,
    capture: &'a UiKeyboardCapture,
    er: &'a ErControls,
}

impl<'a> KeyRead<'a> {
    pub(super) fn new(
        keys: &'a ButtonInput<KeyCode>,
        mouse: &'a ButtonInput<MouseButton>,
        capture: &'a UiKeyboardCapture,
        er: &'a ErControls,
    ) -> Self {
        Self {
            keys,
            mouse,
            capture,
            er,
        }
    }

    fn eaten(&self, key: SoulsKey) -> bool {
        let super_down =
            self.keys.pressed(KeyCode::SuperLeft) || self.keys.pressed(KeyCode::SuperRight);
        super_down
            || self.capture.typing
            || match key {
                SoulsKey::Key(code) => self.capture.consumed.contains(&code),
                SoulsKey::Mouse(button) => self.er.active() && !self.er.claims_mouse(button),
            }
    }

    fn shift(&self) -> bool {
        self.keys.pressed(KeyCode::ShiftLeft) || self.keys.pressed(KeyCode::ShiftRight)
    }

    /// Whether Shift is as the bind wants it.
    fn shift_fits(&self, bind: SoulsBind, press: bool) -> bool {
        if bind.shift {
            !press || self.shift()
        } else {
            !self.er.shifted.contains(&bind.key) || !self.shift()
        }
    }

    fn raw_down(&self, key: SoulsKey) -> bool {
        !self.eaten(key)
            && match key {
                SoulsKey::Key(code) => self.keys.pressed(code),
                SoulsKey::Mouse(button) => self.mouse.pressed(button),
            }
    }

    pub(super) fn down(&self, bind: SoulsBind) -> bool {
        self.shift_fits(bind, false) && self.raw_down(bind.key)
    }

    pub(super) fn just_pressed(&self, bind: SoulsBind) -> bool {
        self.shift_fits(bind, true)
            && !self.eaten(bind.key)
            && match bind.key {
                SoulsKey::Key(code) => self.keys.just_pressed(code),
                SoulsKey::Mouse(button) => self.mouse.just_pressed(button),
            }
    }

    /// Whether a hold is still held: only typing eats it. A shown keyboard frame's per-frame
    /// consumption (an OS key repeat landing on it) must not end a sprint mid-hold.
    fn held(&self, bind: SoulsBind) -> bool {
        !self.capture.typing
            && match bind.key {
                SoulsKey::Key(code) => self.keys.pressed(code),
                SoulsKey::Mouse(button) => self.mouse.pressed(button),
            }
    }

    fn just_released(&self, bind: SoulsBind) -> bool {
        !self.eaten(bind.key)
            && match bind.key {
                SoulsKey::Key(code) => self.keys.just_released(code),
                SoulsKey::Mouse(button) => self.mouse.just_released(button),
            }
    }
}

/// The held movement keys as `MOVEMENTFLAGS` bits, the turn keys counting as strafes while they
/// strafe (lock-on). Under the free camera without a lock the body runs where it faces, so any
/// held key is forward.
fn held_flags(bindings: &BindingsState, turn_keys_strafe: bool, free_run: bool) -> u32 {
    if free_run {
        let any = [
            Input::MoveForward,
            Input::MoveBackward,
            Input::StrafeLeft,
            Input::StrafeRight,
            Input::TurnLeft,
            Input::TurnRight,
        ]
        .into_iter()
        .any(|input| bindings.pressed(input));
        return if any { move_flags::FORWARD } else { 0 };
    }
    let mut flags = 0;
    if bindings.pressed(Input::MoveForward) {
        flags |= move_flags::FORWARD;
    }
    if bindings.pressed(Input::MoveBackward) {
        flags |= move_flags::BACKWARD;
    }
    if bindings.pressed(Input::StrafeLeft)
        || (turn_keys_strafe && bindings.pressed(Input::TurnLeft))
    {
        flags |= move_flags::STRAFE_LEFT;
    }
    if bindings.pressed(Input::StrafeRight)
        || (turn_keys_strafe && bindings.pressed(Input::TurnRight))
    {
        flags |= move_flags::STRAFE_RIGHT;
    }
    flags
}

/// The uncharged swing a charged heavy releases into: the same name without `Charge`.
fn uncharged(kit: &Kit, id: ActionId) -> Option<ActionId> {
    let action = kit.action(id)?;
    let base = action.name.strip_suffix("Charge")?;
    kit.action_named(base).map(|action| action.id)
}

/// The kit action a heavy press asks for.
///
/// - Sprinting (with running attacks on): `RunHeavy`.
/// - Inside the guard counter window: `GuardCounter`.
/// - Otherwise the running action's `next_heavy`, else `Heavy1Charge`, else `Heavy1`, else the
///   kit's first heavy. With charging off, a `…Charge` action is swapped for its uncharged swing.
fn heavy_action(
    kit: &Kit,
    running: Option<ActionId>,
    sprinting: bool,
    counter: bool,
    features: &SoulsFeatures,
) -> Option<ActionId> {
    let named = |name: &str| kit.action_named(name).map(|action| action.id);
    if sprinting && features.running_attacks {
        if let Some(id) = named("RunHeavy") {
            return Some(id);
        }
    }
    if counter && features.guard_counter {
        if let Some(id) = named("GuardCounter") {
            return Some(id);
        }
    }
    let chosen = running
        .and_then(|id| kit.action(id))
        .and_then(|action| action.next_heavy)
        .or_else(|| named("Heavy1Charge"))
        .or_else(|| named("Heavy1"))
        .or_else(|| {
            kit.actions
                .iter()
                .find(|action| action.kind == ActionKind::MeleeHeavy)
                .map(|action| action.id)
        })?;
    if features.charge {
        Some(chosen)
    } else {
        Some(uncharged(kit, chosen).unwrap_or(chosen))
    }
}

/// The light press: `RunLight` while sprinting (with running attacks on), else the kit's chain.
fn light_action(
    kit: &Kit,
    running: Option<ActionId>,
    sprinting: bool,
    features: &SoulsFeatures,
) -> Option<ActionId> {
    if sprinting && features.running_attacks {
        if let Some(action) = kit.action_named("RunLight") {
            return Some(action.id);
        }
    }
    resolve_press(kit, running, Press::Light, 0)
}

/// The kit row `name`, for the presses that need a row of their own (jump attacks, the crouch
/// attack, the parry). A kit without it leaves the press doing nothing, said at debug level.
fn kit_row(kit: &Kit, name: &str) -> Option<ActionId> {
    let row = kit.action_named(name).map(|action| action.id);
    if row.is_none() {
        debug!("souls: the kit has no {name} row");
    }
    row
}

/// The left-hand press: the running left-hand chain's next swing, else the chain's first
/// (`LeftLight1`, else `PairedLight1`, as the kit importer names them).
fn left_action(kit: &Kit, running: Option<ActionId>) -> Option<ActionId> {
    let left_chain = |name: &str| name.starts_with("LeftLight") || name.starts_with("PairedLight");
    running
        .and_then(|id| kit.action(id))
        .filter(|action| left_chain(action.name.as_str()))
        .and_then(|action| action.next_light)
        .or_else(|| kit.action_named("LeftLight1").map(|action| action.id))
        .or_else(|| kit_row(kit, "PairedLight1"))
}

/// Whether the running action is a dodge inside its i-frames.
pub(super) fn in_iframes(playback: &CombatPlaybackState, profiles: &CombatProfiles) -> bool {
    let dodge = playback
        .engine
        .as_ref()
        .and_then(|engine| engine.active())
        .and_then(|id| profiles.table.as_ref()?.get(id))
        .is_some_and(|spec| spec.kind == ActionKind::Dodge);
    dodge && playback.phase(profiles) == Some(ActionPhase::Active)
}

/// Whether the running action is the kit's `Parry` inside its window (its `iframes` row): a hit
/// there is deflected, and an enemy strike judged there is a parry ([`super::server`]).
pub(super) fn in_parry_window(kit: &Kit, playback: &CombatPlaybackState) -> bool {
    running_action(kit, playback).is_some_and(|(_, action, frame)| {
        action.name == "Parry"
            && action
                .iframes
                .is_some_and(|window| in_window(frame, window))
    })
}

/// The Souls presses, the guard, the sprint and the charge release. Off, the state resets and the
/// playback's defense goes back to the stock rule.
#[allow(clippy::too_many_arguments)]
/// The grip key: hold a one-handed weapon in both hands, or back in one. As in Elden Ring the
/// switch waits for the hands to be free: nothing running, not guarding. The kit swaps to the
/// weapon's two-handed moveset on the spot (the profiles recompile from the new generation).
#[allow(clippy::too_many_arguments)]
pub(super) fn grip_toggle(
    raw: (
        Res<ButtonInput<KeyCode>>,
        Res<ButtonInput<MouseButton>>,
        Res<UiKeyboardCapture>,
        Res<ErControls>,
    ),
    view: Res<CombatModeView>,
    config: Res<SoulsConfig>,
    playback: Res<CombatPlaybackState>,
    souls: Res<SoulsState>,
    mut kit: ResMut<PlayerKit>,
) {
    let (keys, mouse, capture, er) = raw;
    if !souls_on(&view)
        || !config.features.two_hand
        || !KeyRead::new(&keys, &mouse, &capture, &er).just_pressed(config.binds.grip)
    {
        return;
    }
    let busy = souls.guarding
        || playback
            .engine
            .as_ref()
            .is_some_and(|engine| engine.active().is_some());
    if busy {
        info!("souls: grip switch waits for free hands");
        return;
    }
    if kit.toggle_grip().is_none() {
        info!("souls: a two-handed weapon has only the one grip");
    }
}

pub(super) fn souls_input(
    raw: (
        Res<ButtonInput<KeyCode>>,
        Res<ButtonInput<MouseButton>>,
        Res<UiKeyboardCapture>,
        Res<BindingsState>,
        Res<ErControls>,
    ),
    time: Res<Time>,
    view: Res<CombatModeView>,
    config: Res<SoulsConfig>,
    kit: Res<PlayerKit>,
    profiles: Res<CombatProfiles>,
    player: Res<Player>,
    me: Query<Entity, With<SelfPlayer>>,
    critical: (
        super::targets::Units,
        crate::target::RelationWorld,
        ResMut<super::server::SoulsLink>,
    ),
    mut playback: ResMut<CombatPlaybackState>,
    mut souls: ResMut<SoulsState>,
    mut play_seq: ResMut<PlaySeq>,
    mut actions: MessageWriter<ActionAnim>,
) {
    if !souls_on(&view) {
        if !souls.is_reset() {
            *souls = SoulsState::default();
        }
        if playback.defense != Defense::default()
            || playback.guarded_hits != 0
            || playback.poised_hits != 0
        {
            playback.defense = Defense::default();
            playback.guarded_hits = 0;
            playback.poised_hits = 0;
        }
        return;
    }
    let (keys, mouse, capture, bindings, er) = raw;
    let Ok(entity) = me.single() else {
        return;
    };
    let input = KeyRead::new(&keys, &mouse, &capture, &er);
    let t = time.elapsed_secs();
    let dt = time.delta_secs();
    let features = config.features;
    let binds = config.binds;
    let kit_now = current_kit(&kit, &profiles);
    // Costs authored in kit points are scaled like the kit's own; the stock bar is unscaled.
    let scale = if kit_now.is_some() {
        STAMINA_SCALE.unsigned_abs()
    } else {
        1
    };
    let locked = features.lock_on && souls.lock.is_some();
    let flags = held_flags(&bindings, locked, er.free_camera && !locked);
    let moving = flags != 0;
    let now = playback.ticks_at();

    // Guarded hits since last frame: each costs stamina; an empty bar breaks the guard.
    let guarded = std::mem::take(&mut playback.guarded_hits);
    if guarded > 0 {
        if let Some(engine) = playback.engine.as_mut() {
            let cost = GUARD_HIT_POINTS
                .saturating_mul(scale)
                .saturating_mul(guarded);
            let stamina = engine.stamina();
            if stamina <= cost {
                engine.set_stamina(0, now);
                souls.guarding = false;
                souls.guard_broken_until = t + GUARD_BREAK_SECONDS;
                info!("souls: guard broken");
            } else {
                engine.set_stamina(stamina - cost, now);
                if features.guard_counter {
                    souls.counter_until = t + GUARD_COUNTER_SECONDS;
                }
                // The retarget holds the stance's own guard; the stock one is a shield's.
                if !features.retarget_locomotion {
                    play_clip(ANIM_SHIELD_BLOCK, entity, &mut play_seq, &mut actions);
                }
            }
        }
    }

    // Poise: a hit poise held costs poise (half inside a heavy's windup, the hyperarmor); at zero
    // it breaks, the hit staggers and the poise refills. It also refills a while after the last
    // hit, as ER's does.
    let poised = std::mem::take(&mut playback.poised_hits);
    if !features.poise || (souls.poise.is_some() && t >= souls.poise_reset_at) {
        souls.poise = None;
    }
    if poised > 0 && features.poise {
        let armoured = kit_now.is_some_and(|kit| hyperarmor(kit, &playback));
        let factor = if armoured {
            HYPERARMOR_POISE_FACTOR
        } else {
            1.0
        };
        let left = souls.poise.unwrap_or(POISE_MAX) - POISE_DAMAGE_PER_HIT * factor * poised as f32;
        souls.poise_reset_at = t + POISE_RESET_SECONDS;
        if left <= 0.0 {
            souls.poise = None;
            info!("souls: poise broken");
            stagger(
                &mut playback,
                entity,
                &profiles,
                &mut play_seq,
                &mut actions,
            );
        } else {
            souls.poise = Some(left);
        }
    }

    // The guard: held while nothing runs, not sprinting and not broken.
    let idle = playback
        .engine
        .as_ref()
        .is_some_and(|engine| engine.active().is_none());
    let guard = features.guard
        && input.down(binds.guard)
        && t >= souls.guard_broken_until
        && idle
        && !souls.sprinting;
    if guard && !souls.guarding && !features.retarget_locomotion {
        play_clip(ANIM_SHIELD_BLOCK, entity, &mut play_seq, &mut actions);
    }
    souls.guarding = guard;

    // The dodge key: a tap rolls on release, a hold while moving sprints.
    if input.just_pressed(binds.dodge) {
        souls.dodge_down_at = Some(t);
        souls.dodge_sprinted = false;
    }
    let dodge_held = input.held(binds.dodge);
    let dodge_released =
        input.just_released(binds.dodge) || (!dodge_held && souls.dodge_down_at.is_some());
    let held_for = souls.dodge_down_at.map_or(0.0, |at| t - at);
    let has_stamina = playback
        .engine
        .as_ref()
        .is_some_and(|engine| engine.stamina() > 0);
    if moving {
        souls.last_moving_at = t;
    }
    // A direction change through neutral (or a frame of no keys) keeps the sprint.
    let moving_now = moving || t - souls.last_moving_at < SPRINT_NEUTRAL_GRACE_SECONDS;
    if !souls.sprinting
        && features.sprint
        && dodge_held
        && moving
        && held_for >= SPRINT_AFTER_SECONDS
        && has_stamina
    {
        info!("souls: sprint");
        souls.sprinting = true;
        souls.dodge_sprinted = true;
    }
    if souls.sprinting && (!dodge_held || !moving_now || !features.sprint) {
        info!(
            "souls: sprint ends (dodge held {dodge_held}, moving {moving_now}, on {})",
            features.sprint
        );
        souls.sprinting = false;
        souls.sprint_owed = 0.0;
    }
    if souls.sprinting {
        if let Some(engine) = playback.engine.as_mut() {
            souls.sprint_owed += SPRINT_DRAIN_POINTS_PER_SECOND * scale as f32 * dt;
            let whole = souls.sprint_owed.floor();
            if whole >= 1.0 {
                souls.sprint_owed -= whole;
                let left = engine.stamina().saturating_sub(whole as u32);
                engine.set_stamina(left, now);
                if left == 0 {
                    info!("souls: sprint ends, out of stamina");
                    souls.sprinting = false;
                }
            }
        }
    }
    let dodge_press = if features.sprint {
        dodge_released && souls.dodge_down_at.is_some() && !souls.dodge_sprinted
    } else {
        input.just_pressed(binds.dodge)
    };
    if dodge_released || !dodge_held {
        souls.dodge_down_at = None;
    }

    // The charge release: released inside the charge window, the charged heavy is swapped for
    // its uncharged swing (its cost refunded, the uncharged one's spent).
    let running = kit_now.and_then(|kit| running_action(kit, &playback));
    if let Some(mut hold) = souls.charge {
        if !input.down(binds.heavy) {
            hold.released = true;
        }
        let engine = playback.engine.as_ref();
        match (hold.instance, running) {
            (None, Some((instance, _, _))) if instance.id == hold.id => {
                hold.instance = Some(instance);
            }
            // Neither running nor held any more: refused or dropped.
            (None, _) if engine.and_then(|engine| engine.buffered()) != Some(hold.id) => {
                souls.charge = None;
            }
            _ => {}
        }
        souls.charge = souls.charge.map(|_| hold);
        if let (Some(instance), Some(kit)) = (hold.instance, kit_now) {
            match running {
                Some((now_running, action, frame)) if now_running == instance => {
                    let window = action.charge.filter(|_| features.charge);
                    match window {
                        Some(window) if hold.released && in_window(frame, window) => {
                            souls.charge = None;
                            if let Some(swing) = uncharged(kit, instance.id) {
                                release_charge(
                                    &mut playback,
                                    instance.id,
                                    swing,
                                    entity,
                                    &profiles,
                                    &mut play_seq,
                                    &mut actions,
                                );
                            }
                        }
                        Some((_, to)) if frame < to => {}
                        // Past the window (or no window): the charged swing plays out.
                        _ => souls.charge = None,
                    }
                }
                _ => souls.charge = None,
            }
        }
    }

    // The crouch: its key toggles it; a dodge, a sprint or any accepted action stands back up.
    if !features.crouch || dodge_press || souls.sprinting {
        souls.crouched = false;
    } else if input.just_pressed(binds.crouch) {
        souls.crouched = !souls.crouched;
        info!(
            "souls: {}",
            if souls.crouched { "crouched" } else { "stood" }
        );
    }

    // The jump attack: a light, heavy or (paired) left press in the air is held until the body
    // comes down, then the kit's landing swing plays; ER has no ground swing mid-jump. The air
    // half of the swing is the WoW jump clip until the retarget.
    let airborne = player.airborne();
    let landed = souls.was_airborne && !airborne;
    souls.was_airborne = airborne;
    let jumping = features.jump_attacks && airborne;
    if !features.jump_attacks || (!airborne && !landed) {
        souls.air_attack = None;
    }
    let landing = if landed {
        souls.air_attack.take()
    } else {
        None
    };

    // The presses.
    let active = playback.engine.as_ref().and_then(|engine| engine.active());
    let counter = t < souls.counter_until;
    let mut heavy_pressed = None;
    let (units, relations, mut link) = critical;
    let mut crit = None;
    let request = if let Some(row) = landing {
        kit_now.and_then(|kit| kit_row(kit, row))
    } else if jumping {
        if souls.air_attack.is_none() {
            souls.air_attack = if input.just_pressed(binds.light) {
                Some("JumpLightLand")
            } else if features.heavy && input.just_pressed(binds.heavy) {
                Some("JumpHeavyLand")
            } else if features.left_hand && input.just_pressed(binds.left) {
                Some("PairedJumpLand")
            } else {
                None
            };
        }
        None
    } else if dodge_press {
        match kit_now {
            Some(kit) => resolve_dodge(kit, flags, Some(roll_load(link.load))),
            None => Some(stock_press(Press::Dodge)),
        }
    } else if input.just_pressed(binds.light) {
        crit = kit_now
            .filter(|_| idle && link.reporting())
            .and_then(|kit| {
                let broken = link.broken.and_then(|opening| {
                    super::targets::broken_opening(&units, &relations, player.pos, t, opening)
                });
                let (guid, back) = if let Some(guid) = broken {
                    (guid, false)
                } else {
                    (
                        super::targets::backstab_opening(&units, &relations, player.pos)?,
                        true,
                    )
                };
                let action = kit
                    .action_named("Critical")
                    .or_else(|| kit.action_named("Heavy1"))?;
                Some((guid, back, action.id))
            });
        if let Some((_, _, id)) = crit {
            Some(id)
        } else {
            match kit_now {
                Some(kit) if souls.crouched => kit_row(kit, "CrouchAttack")
                    .or_else(|| light_action(kit, active, false, &features)),
                Some(kit) => light_action(kit, active, souls.sprinting, &features),
                None => Some(stock_press(Press::Light)),
            }
        }
    } else if features.heavy && input.just_pressed(binds.heavy) {
        let id =
            kit_now.and_then(|kit| heavy_action(kit, active, souls.sprinting, counter, &features));
        heavy_pressed = id;
        id
    } else if features.left_hand && input.just_pressed(binds.left) {
        kit_now.and_then(|kit| left_action(kit, active))
    } else if features.parry && input.just_pressed(binds.parry) {
        kit_now.and_then(|kit| kit_row(kit, "Parry"))
    } else {
        None
    };
    if let Some(action) = request {
        let events = submit_and_play(
            &mut playback,
            action,
            entity,
            &profiles,
            &mut play_seq,
            &mut actions,
        );
        if let Some((guid, back, _)) = crit.filter(|_| {
            events
                .iter()
                .any(|e| matches!(e, EngineEvent::Started { id, .. } if *id == action))
        }) {
            if let Some((instance, ..)) = kit_now.and_then(|kit| running_action(kit, &playback)) {
                souls.critical = Some(instance);
                if !back {
                    link.broken = None;
                }
                link.critical(guid, back);
            }
        }
        let refused = events.iter().any(|event| {
            matches!(
                event,
                EngineEvent::Refused {
                    reason: ActionRefusal::InsufficientStamina,
                    ..
                }
            )
        });
        if refused {
            souls.refused_until = t + REFUSAL_FLASH_SECONDS;
        }
        let accepted = events.iter().any(|event| {
            matches!(
                event,
                EngineEvent::Started { id, .. } | EngineEvent::Buffered { id, .. } if *id == action
            )
        });
        if accepted {
            souls.sprinting = false;
            souls.crouched = false;
            if counter && heavy_pressed.is_some() {
                souls.counter_until = 0.0;
            }
        }
        let charges = kit_now
            .and_then(|kit| kit.action(action))
            .is_some_and(|row| row.charge.is_some());
        if accepted && features.charge && heavy_pressed == Some(action) && charges {
            souls.charge = Some(ChargeHold {
                id: action,
                instance: None,
                released: false,
            });
        }
    }

    // What shields the prediction from the next landed hit.
    let defense = Defense {
        iframes: (features.iframes && in_iframes(&playback, &profiles))
            || (features.parry && kit_now.is_some_and(|kit| in_parry_window(kit, &playback))),
        guarding: souls.guarding,
        poised: features.poise,
    };
    if playback.defense != defense {
        playback.defense = defense;
    }
}

/// Whether the running action is a heavy before its last hit ends: its windup and swing carry
/// hyperarmor.
fn hyperarmor(kit: &Kit, playback: &CombatPlaybackState) -> bool {
    running_action(kit, playback).is_some_and(|(_, action, frame)| {
        action.kind == ActionKind::MeleeHeavy
            && action.hits.last().is_some_and(|hit| frame < hit.to)
    })
}

/// A broken poise staggers: the running action is interrupted where the engine lets it be (an
/// attack until its first hit, a dodge throughout); past that the action plays out.
fn stagger(
    playback: &mut CombatPlaybackState,
    entity: Entity,
    profiles: &CombatProfiles,
    play_seq: &mut PlaySeq,
    actions: &mut MessageWriter<ActionAnim>,
) {
    let now = playback.ticks_at();
    let Some(engine) = playback.engine.as_mut() else {
        return;
    };
    if engine.active().is_none() {
        return;
    }
    match engine.interrupt(now) {
        Ok(events) => play_starts(&events, entity, profiles, play_seq, actions),
        Err(error) => debug!("souls: poise break did not interrupt: {error}"),
    }
}

/// Swap the running charged heavy `charged` for `swing`: interrupt it (an attack yields until its
/// first hit, and the charge window is before it), refund its cost, and submit the swing.
fn release_charge(
    playback: &mut CombatPlaybackState,
    charged: ActionId,
    swing: ActionId,
    entity: Entity,
    profiles: &CombatProfiles,
    play_seq: &mut PlaySeq,
    actions: &mut MessageWriter<ActionAnim>,
) {
    let now = playback.ticks_at();
    let Some(engine) = playback.engine.as_mut() else {
        return;
    };
    match engine.interrupt(now) {
        Ok(_) if engine.active().is_none() => {
            let refund = engine
                .table()
                .get(charged)
                .map_or(0, |spec| spec.stamina_cost);
            let bar = engine.stamina().saturating_add(refund);
            engine.set_stamina(bar, now);
        }
        Ok(_) => return,
        Err(error) => {
            warn!("souls: charge release failed: {error}");
            return;
        }
    }
    submit_and_play(playback, swing, entity, profiles, play_seq, actions);
}

#[cfg(test)]
mod tests {
    use super::*;
    use benilla_combat_core::{KitAction, KitCancels, Milli};

    fn action(id: u32, name: &str, kind: ActionKind, next_heavy: Option<u32>) -> KitAction {
        KitAction {
            id: ActionId(id),
            name: name.into(),
            source: String::new(),
            kind,
            total: Milli(30_000),
            input_from: None,
            dodge_input_from: None,
            cancel: KitCancels::default(),
            iframes: None,
            airborne: false,
            stamina: Milli::ZERO,
            hits: Vec::new(),
            charge: name
                .ends_with("Charge")
                .then_some((Milli(5_000), Milli(24_000))),
            no_turn: Vec::new(),
            turn: Vec::new(),
            motion: Vec::new(),
            next_light: None,
            next_heavy: next_heavy.map(ActionId),
        }
    }

    fn kit() -> Kit {
        Kit {
            label: "test".into(),
            frame_rate_hz: 30,
            stamina_max: None,
            stance: None,
            actions: vec![
                action(110, "RunLight", ActionKind::MeleeLight, None),
                action(111, "RunHeavy", ActionKind::MeleeHeavy, None),
                action(115, "Heavy1Charge", ActionKind::MeleeHeavy, Some(117)),
                action(116, "Heavy1", ActionKind::MeleeHeavy, Some(117)),
                action(117, "Heavy2Charge", ActionKind::MeleeHeavy, None),
                action(118, "Heavy2", ActionKind::MeleeHeavy, None),
                action(119, "GuardCounter", ActionKind::MeleeHeavy, None),
            ],
        }
    }

    /// The heavy press follows the ER rules: running heavy while sprinting, the guard counter in
    /// its window, the chain's next heavy, the charged opener; with charging off, the uncharged
    /// swing of the same step.
    #[test]
    fn heavy_press_resolves_like_elden_ring() {
        let kit = kit();
        let on = SoulsFeatures::default();
        assert_eq!(
            heavy_action(&kit, None, false, false, &on),
            Some(ActionId(115))
        );
        assert_eq!(
            heavy_action(&kit, Some(ActionId(115)), false, false, &on),
            Some(ActionId(117))
        );
        assert_eq!(
            heavy_action(&kit, None, true, false, &on),
            Some(ActionId(111))
        );
        assert_eq!(
            heavy_action(&kit, None, false, true, &on),
            Some(ActionId(119))
        );
        let no_charge = SoulsFeatures {
            charge: false,
            ..on
        };
        assert_eq!(
            heavy_action(&kit, None, false, false, &no_charge),
            Some(ActionId(116))
        );
        assert_eq!(
            heavy_action(&kit, Some(ActionId(116)), false, false, &no_charge),
            Some(ActionId(118))
        );
        let no_run = SoulsFeatures {
            running_attacks: false,
            guard_counter: false,
            ..on
        };
        assert_eq!(
            heavy_action(&kit, None, true, true, &no_run),
            Some(ActionId(115))
        );
        assert_eq!(uncharged(&kit, ActionId(117)), Some(ActionId(118)));
        assert_eq!(uncharged(&kit, ActionId(118)), None);
    }
}
