//! The toggleable combat-mode boundary and the presentation contract handed to the UI host.
//!
//! # Why this is baseline-only
//!
//! The installed server/mod set has no mode channel, no authoritative stamina and no hook that
//! can replace a melee/spell hit outcome with an experimental dodge before packets, procs and
//! logging are built (see `.scratch/project-b/checkpoints/2026-10-26-oc-connected-gameplay-campaign.md`).
//! A client that sent an experimental mode request, invented stamina or claimed an avoided hit
//! would be fabricating authority. Therefore:
//!
//! - the effective mode is always [`CombatMode::Baseline`];
//! - an experimental request is recorded as user intent and answered [`CombatModeDecision::NotSent`];
//! - no experimental packet is emitted;
//! - ordinary AzerothCore traffic and the stock combat owners are untouched.
//!
//! # Contract for the UI host (Astra)
//!
//! [`CombatModeView`] is the single read model. It is a projection, not a second authority: every
//! observed field comes from an existing owner and is refreshed once per frame.
//!
//! - `requested`: the last mode the user asked for, `None` before any request.
//! - `effective`: the mode the authority has accepted. Always `Baseline` today; only a server
//!   message may change it once a mode channel exists.
//! - `availability`: `Unavailable { ServerModeChannelMissing }` today. Do not show an enabled
//!   experimental toggle from this; show the reason instead.
//! - `last_refusal`: why the last experimental request was not sent.
//! - `resources`: self health/max health/power type/power from the authoritative observation.
//!   Each field is independently `None` when that field has not been delivered, never zero.
//! - `committed_action`: the cast/channel currently owned by the existing cast owner; `None`
//!   means no observed commitment. A cancellation is observed as this becoming `None`; the cast
//!   owner publishes no separate cancellation reason.
//! - `cooldown`: the spell/category/GCD recovery of the committed action from the existing
//!   cooldown owner. This is ordinary AzerothCore recovery, not an experimental stamina or
//!   i-frame window; those have no server owner and are intentionally absent.
//! - `revision`: increments whenever an observed field changes, for change-driven UI updates.
//!
//! Lifecycle: request -> refusal (today) -> effective stays `Baseline`; world transfer/session
//! end clear `resources`, `committed_action` and `cooldown` through their owning resources. The
//! UI must never write `effective` or synthesise resources/dodge/stamina.

#![allow(dead_code)] // The UI host consumes this contract in its own slice.

use std::time::Instant;

use bevy::prelude::*;
use benilla_world::schedule::WorldStage;

use crate::spell::{ActiveChannel, Cooldowns, PendingCast};
use crate::ui_action::Spells;
use crate::wrath::WrathWorldView;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) enum CombatMode {
    #[default]
    Baseline,
    Experimental,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum CombatModeUnavailable {
    /// No installed server/mod owner accepts a mode request, carries stamina, or gates a hit
    /// outcome. Sending a request would be invented traffic.
    ServerModeChannelMissing,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum CombatModeAvailability {
    Available,
    Unavailable(CombatModeUnavailable),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum CombatModeRefusal {
    Unavailable(CombatModeUnavailable),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum CombatModeDecision {
    /// The requested mode is already the effective mode; no traffic was needed.
    AlreadyEffective(CombatMode),
    /// The request was not sent. The effective mode is unchanged.
    NotSent {
        requested: CombatMode,
        reason: CombatModeUnavailable,
    },
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum CommittedAction {
    Cast {
        spell_id: u32,
        item_guid: Option<u64>,
    },
    Channel {
        spell_id: u32,
    },
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct CombatResources {
    pub health: Option<u32>,
    pub max_health: Option<u32>,
    pub power_type: Option<u8>,
    pub power: Option<u32>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct CombatCooldown {
    pub spell_id: u32,
    pub remaining_ms: u32,
    pub duration_ms: u32,
    /// False while a cooldown-on-event leg is parked; the stock cooldown frame hides it.
    pub enabled: bool,
}

#[derive(Resource, Debug, PartialEq, Eq)]
pub(crate) struct CombatModeView {
    pub requested: Option<CombatMode>,
    pub effective: CombatMode,
    pub availability: CombatModeAvailability,
    pub last_refusal: Option<CombatModeRefusal>,
    pub resources: Option<CombatResources>,
    pub committed_action: Option<CommittedAction>,
    pub cooldown: Option<CombatCooldown>,
    pub revision: u64,
}

impl Default for CombatModeView {
    fn default() -> Self {
        Self {
            requested: None,
            effective: CombatMode::Baseline,
            availability: CombatModeAvailability::Unavailable(
                CombatModeUnavailable::ServerModeChannelMissing,
            ),
            last_refusal: None,
            resources: None,
            committed_action: None,
            cooldown: None,
            revision: 0,
        }
    }
}

impl CombatModeView {
    /// Switching to baseline is a local no-op: baseline is already the authoritative mode, so
    /// this sends nothing and leaves all observed fields to the projection.
    pub(crate) fn request_baseline(&mut self) -> CombatModeDecision {
        self.requested = Some(CombatMode::Baseline);
        self.revision = self.revision.wrapping_add(1);
        CombatModeDecision::AlreadyEffective(CombatMode::Baseline)
    }

    /// Ask for the experimental mode. The request is recorded even when it cannot be sent, so the
    /// UI can show "requested, unavailable, reason"; `effective` never changes without a server
    /// acceptance.
    pub(crate) fn request_experimental(&mut self) -> CombatModeDecision {
        self.requested = Some(CombatMode::Experimental);
        let reason = match self.availability {
            CombatModeAvailability::Unavailable(reason) => reason,
            // No implementation can report Available until a server mode channel exists; keep a
            // defensive refusal rather than fabricating a local acceptance.
            CombatModeAvailability::Available => CombatModeUnavailable::ServerModeChannelMissing,
        };
        self.last_refusal = Some(CombatModeRefusal::Unavailable(reason));
        self.revision = self.revision.wrapping_add(1);
        CombatModeDecision::NotSent {
            requested: CombatMode::Experimental,
            reason,
        }
    }
}

pub(crate) struct CombatModePlugin;

impl Plugin for CombatModePlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<CombatModeView>()
            // The local Souls window state machine. It is inert until a server mode channel exists
            // and is closed by the lifecycle system below on death, disconnect or mode exit.
            .init_resource::<CombatWindowState>()
            .add_systems(
                Update,
                project_combat_mode
                    .after(WorldStage::Net)
                    .after(WorldStage::Input),
            )
            .add_systems(
                Update,
                close_combat_window_on_lifecycle.after(project_combat_mode),
            );
    }
}

/// Refresh the observed half of the contract from the existing owners. This never accepts a mode,
/// debits a resource or manufactures a recovery window.
fn project_combat_mode(
    world: Option<Res<WrathWorldView>>,
    pending: Option<Res<PendingCast>>,
    channel: Option<Res<ActiveChannel>>,
    cooldowns: Option<Res<Cooldowns>>,
    spells: Option<Res<Spells>>,
    mut view: ResMut<CombatModeView>,
) {
    let now = Instant::now();

    let resources = world
        .as_ref()
        .and_then(|world| world.self_observation.as_ref())
        .map(|self_state| CombatResources {
            health: self_state.resources.health,
            max_health: self_state.resources.max_health,
            power_type: self_state.resources.power_type,
            power: self_state.resources.power,
        });

    let committed_action = match channel.as_ref().and_then(|channel| channel.current(now)) {
        Some(spell_id) => Some(CommittedAction::Channel { spell_id }),
        None => pending.as_ref().and_then(|pending| {
            pending.current().map(|spell_id| CommittedAction::Cast {
                spell_id,
                item_guid: pending.item(),
            })
        }),
    };

    let cooldown = match (&committed_action, cooldowns.as_ref()) {
        (
            Some(CommittedAction::Cast { spell_id, .. } | CommittedAction::Channel { spell_id }),
            Some(cooldowns),
        ) => {
            let display = spells
                .as_ref()
                .and_then(|spells| spells.catalog.get(*spell_id));
            let info = cooldowns.info(*spell_id, 0, display, now);
            (info.remaining_ms > 0 || !info.enabled).then_some(CombatCooldown {
                spell_id: *spell_id,
                remaining_ms: info.remaining_ms,
                duration_ms: info.duration_ms,
                enabled: info.enabled,
            })
        }
        _ => None,
    };

    let changed = view.resources != resources
        || view.committed_action != committed_action
        || view.cooldown != cooldown;
    view.resources = resources;
    view.committed_action = committed_action;
    view.cooldown = cooldown;
    if changed {
        view.revision = view.revision.wrapping_add(1);
    }
}

// ============================================================================
// Souls combat seam (`CombatModuleV1`)
// ============================================================================
//
// This is the local half of the seamless-combat boundary. It is deliberately *not* a second
// combat authority:
//
// - A module may only propose a window (dodge/roll/block/stamina) and its own advisory numbers.
//   The proposal type has no damage, hit, reward or resource-debit field at all, so the module
//   cannot assert a hit, award loot, or spend stamina through this seam.
// - The server's acknowledgement is the only authority. A proposal becomes a pending window and
//   nothing more; only `apply_authoritative_combat_result` can settle it, and that is the only
//   writer of `authoritative_stamina`.
// - This checkout has **no** server/mod mode channel and no Souls acknowledgement receiver
//   (see the module header and `.scratch/project-b/checkpoints/2026-10-26-oc-connected-gameplay-campaign.md`).
//   Capability therefore defaults to `Missing`, `evaluate_combat_intent` refuses with
//   `NoServerContract`, and the effective mode stays `Baseline`. The `Negotiated` capability and
//   the `NotSent` decision exist so the state machine is typed and testable; no production code
//   path can produce either until a receiver is implemented.

/// The only combat windows this seam understands. A module proposing anything else is refused.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum CombatWindowKind {
    Dodge,
    Roll,
    Block,
    Stamina,
}

/// A `CombatModuleV1` proposal, translated to host-facing values. Stamina, iframes and poise are
/// the module's *view*; none of them is applied without a server result.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct CombatProposal {
    /// Monotonic per-session sequence, echoed by the server acknowledgement. `0` is invalid.
    pub(crate) sequence: u64,
    pub(crate) window: CombatWindowKind,
    pub(crate) trigger_spell_id: u32,
    pub(crate) iframe_active: bool,
    pub(crate) hyperarmor_active: bool,
    /// Advisory cost. Never debited locally; kept only so a refusal can name an out-of-range value.
    pub(crate) stamina_cost: f32,
    pub(crate) window_ms: u32,
}

/// Capability as negotiated by the mode owner. This checkout never negotiates.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) enum CombatCapability {
    /// No installed owner accepts a mode request or carries a Souls window. The default.
    #[default]
    Missing,
    /// A server owner would answer a request carrying this sequence. **Unreachable in this
    /// checkout**: no receiver exists, so no production path constructs this value.
    Negotiated,
}

/// A window that was proposed and is awaiting a server answer.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct CombatPendingWindow {
    pub(crate) sequence: u64,
    pub(crate) kind: CombatWindowKind,
    pub(crate) trigger_spell_id: u32,
    pub(crate) iframe_active: bool,
    pub(crate) hyperarmor_active: bool,
    pub(crate) window_ms: u32,
}

/// Why a proposal did not become a pending window.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum CombatIntentRefusal {
    /// The mode is off (the default): baseline class combat owns the tick.
    Disabled,
    /// No server owner accepts the mode, so a proposal cannot be sent or answered.
    NoServerContract,
    /// The proposal's own values were unusable.
    InvalidProposal,
    /// A window is already awaiting its answer.
    WindowOpen,
    /// The sequence was already proposed.
    DuplicateSequence,
}

/// The seam's answer for one tick.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) enum CombatIntentDecision {
    /// The mode is disabled; baseline class combat runs unchanged.
    Disabled,
    /// Nothing was staged this tick.
    Idle,
    /// The proposal was refused before any request existed.
    Refused(CombatIntentRefusal),
    /// A request is owed, but no sender/receiver exists in this checkout: the explicit `NotSent`
    /// boundary. The caller must not claim an outcome from it.
    NotSent(CombatPendingWindow),
}

/// The server's answer for one window.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) enum CombatServerResult {
    /// The server accepted the window and echoed its sequence; stamina, when carried, is the
    /// server's own authoritative value.
    Accepted {
        sequence: u64,
        stamina: Option<u32>,
    },
    Rejected {
        sequence: u64,
    },
    /// No answer arrived before the window closed.
    Unknown {
        sequence: u64,
    },
}

impl CombatServerResult {
    pub(crate) fn sequence(&self) -> u64 {
        match self {
            CombatServerResult::Accepted { sequence, .. }
            | CombatServerResult::Rejected { sequence }
            | CombatServerResult::Unknown { sequence } => *sequence,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum CombatSettlementOutcome {
    Accepted,
    Rejected,
    Unknown,
}

/// The result of feeding one server answer into the window state.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) enum CombatSettlement {
    /// This answer settled the open window; apply it once.
    Settled {
        sequence: u64,
        outcome: CombatSettlementOutcome,
        stamina: Option<u32>,
    },
    /// The window was already settled. Nothing changed and nothing rolls back.
    Duplicate { sequence: u64 },
    /// The answer is older than the open or settled window. It cannot roll back a later hit.
    Late { sequence: u64 },
    /// No window ever carried that sequence.
    Unmatched { sequence: u64 },
}

/// Why a window was closed without a server answer.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum CombatCloseReason {
    Death,
    Disconnect,
    ModeExit,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) enum CombatClose {
    /// The pending window was closed as `Unknown`, settled once.
    Unknown {
        sequence: u64,
        reason: CombatCloseReason,
    },
    /// Nothing was pending and nothing was staged.
    Nothing,
}

/// The local Souls window state. At most one window is in flight; the only progression is
/// proposal -> pending -> one server settlement.
#[derive(Resource, Debug, Default, PartialEq)]
pub(crate) struct CombatWindowState {
    capability: CombatCapability,
    staged: Option<CombatProposal>,
    pending: Option<CombatPendingWindow>,
    /// The newest sequence a proposal carried, refused or not, for duplicate detection.
    last_sequence: Option<u64>,
    /// The newest settled window and its outcome. A later answer may not rewrite it.
    settled: Option<(u64, CombatSettlementOutcome)>,
    /// The only stamina value this seam stores: one the server reported.
    authoritative_stamina: Option<u32>,
    /// The last proposal drained without being sent (disabled/death/disconnect/mode exit).
    last_drained: Option<u64>,
}

impl CombatWindowState {
    pub(crate) fn capability(&self) -> CombatCapability {
        self.capability
    }

    /// Capability is set only by the mode owner that actually negotiated the channel. This
    /// checkout has no such owner; the method exists for the typed state machine and its tests.
    pub(crate) fn set_capability(&mut self, capability: CombatCapability) {
        self.capability = capability;
    }

    pub(crate) fn pending(&self) -> Option<CombatPendingWindow> {
        self.pending
    }

    pub(crate) fn settled(&self) -> Option<(u64, CombatSettlementOutcome)> {
        self.settled
    }

    pub(crate) fn authoritative_stamina(&self) -> Option<u32> {
        self.authoritative_stamina
    }

    pub(crate) fn last_drained(&self) -> Option<u64> {
        self.last_drained
    }

    pub(crate) fn staged(&self) -> Option<CombatProposal> {
        self.staged
    }
}

/// Stage at most one proposal for the next `evaluate_combat_intent`. A second proposal in the
/// same tick replaces nothing: the first is kept and the second dropped.
pub(crate) fn stage_combat_proposal(
    state: &mut CombatWindowState,
    proposal: CombatProposal,
) -> bool {
    if state.staged.is_some() {
        return false;
    }
    state.staged = Some(proposal);
    true
}

/// Turn this tick's staged proposal into a typed decision. Nothing is sent and no resource is
/// debited: under `Missing` capability the answer is a refusal, and even under `Negotiated` the
/// answer is only `NotSent`.
pub(crate) fn evaluate_combat_intent(
    state: &mut CombatWindowState,
    enabled: bool,
) -> CombatIntentDecision {
    let proposal = state.staged.take();
    if !enabled {
        if let Some(proposal) = proposal {
            state.last_drained = Some(proposal.sequence);
        }
        return CombatIntentDecision::Disabled;
    }
    let Some(proposal) = proposal else {
        return CombatIntentDecision::Idle;
    };
    let drain = |state: &mut CombatWindowState| {
        state.last_drained = Some(proposal.sequence);
    };
    if proposal.sequence == 0
        || proposal.window_ms == 0
        || !proposal.stamina_cost.is_finite()
        || proposal.stamina_cost < 0.0
        || (proposal.iframe_active
            && !matches!(
                proposal.window,
                CombatWindowKind::Dodge | CombatWindowKind::Roll
            ))
    {
        drain(state);
        return CombatIntentDecision::Refused(CombatIntentRefusal::InvalidProposal);
    }
    if state.pending.is_some() {
        drain(state);
        return CombatIntentDecision::Refused(CombatIntentRefusal::WindowOpen);
    }
    if state
        .last_sequence
        .is_some_and(|last| proposal.sequence <= last)
    {
        drain(state);
        return CombatIntentDecision::Refused(CombatIntentRefusal::DuplicateSequence);
    }
    state.last_sequence = Some(proposal.sequence);
    let window = CombatPendingWindow {
        sequence: proposal.sequence,
        kind: proposal.window,
        trigger_spell_id: proposal.trigger_spell_id,
        iframe_active: proposal.iframe_active,
        hyperarmor_active: proposal.hyperarmor_active,
        window_ms: proposal.window_ms,
    };
    match state.capability {
        CombatCapability::Missing => {
            drain(state);
            CombatIntentDecision::Refused(CombatIntentRefusal::NoServerContract)
        }
        CombatCapability::Negotiated => {
            // There is still no sender: this is the `NotSent` boundary, not a success.
            state.pending = Some(window);
            CombatIntentDecision::NotSent(window)
        }
    }
}

/// Feed one server acknowledgement into the window state. The first answer for a window settles
/// it; a duplicate settles nothing and a late answer for an older window can never roll back a
/// newer authoritative outcome or the stamina the server reported for it.
pub(crate) fn apply_authoritative_combat_result(
    state: &mut CombatWindowState,
    result: CombatServerResult,
) -> CombatSettlement {
    let sequence = result.sequence();
    if let Some(pending) = state.pending {
        if pending.sequence == sequence {
            state.pending = None;
            let (outcome, stamina) = match result {
                CombatServerResult::Accepted { stamina, .. } => {
                    (CombatSettlementOutcome::Accepted, stamina)
                }
                CombatServerResult::Rejected { .. } => (CombatSettlementOutcome::Rejected, None),
                CombatServerResult::Unknown { .. } => (CombatSettlementOutcome::Unknown, None),
            };
            state.settled = Some((sequence, outcome));
            if outcome == CombatSettlementOutcome::Accepted {
                // The server's own value is the only stamina this seam keeps; a frame that
                // carries none leaves the last authoritative value in place.
                state.authoritative_stamina = stamina.or(state.authoritative_stamina);
            }
            return CombatSettlement::Settled {
                sequence,
                outcome,
                stamina,
            };
        }
        if sequence < pending.sequence {
            return CombatSettlement::Late { sequence };
        }
    }
    if let Some((_settled, outcome)) = state.settled.filter(|(settled, _)| *settled == sequence) {
        // A window this seam closed without an answer (`Unknown`) settles once; a later answer
        // for it is late, not a second settlement. A window the server already settled sees only
        // duplicates.
        return if outcome == CombatSettlementOutcome::Unknown {
            CombatSettlement::Late { sequence }
        } else {
            CombatSettlement::Duplicate { sequence }
        };
    }
    if state.settled.is_some_and(|(settled, _)| sequence < settled) {
        return CombatSettlement::Late { sequence };
    }
    CombatSettlement::Unmatched { sequence }
}

/// Close any in-flight window without a server answer and drain the staged proposal. The close is
/// a settlement (`Unknown`), recorded once, so a later answer for it is `Late` rather than a
/// second settlement.
pub(crate) fn close_combat_window(
    state: &mut CombatWindowState,
    reason: CombatCloseReason,
) -> CombatClose {
    let staged = state.staged.take();
    let Some(pending) = state.pending.take() else {
        if let Some(staged) = staged {
            state.last_drained = Some(staged.sequence);
        }
        return CombatClose::Nothing;
    };
    state.settled = Some((pending.sequence, CombatSettlementOutcome::Unknown));
    state.last_drained = Some(staged.map_or(pending.sequence, |proposal| proposal.sequence));
    CombatClose::Unknown {
        sequence: pending.sequence,
        reason,
    }
}

/// Close the seam's window when the session, the character or the mode can no longer own it.
/// Death and a new session/world generation are the observed signals this checkout actually has;
/// a mode exit is the view leaving `Experimental`, which cannot happen until a mode channel exists.
fn close_combat_window_on_lifecycle(
    world: Option<Res<WrathWorldView>>,
    view: Res<CombatModeView>,
    mut window: ResMut<CombatWindowState>,
    mut last_session: Local<Option<(u64, u64)>>,
    mut was_enabled: Local<bool>,
) {
    let session = world
        .as_ref()
        .map(|w| (w.session_epoch, w.world_generation));
    if (*last_session).is_some_and(|previous| session != Some(previous)) {
        close_combat_window(&mut window, CombatCloseReason::Disconnect);
    }
    *last_session = session;
    let dead = world
        .as_ref()
        .and_then(|world| world.self_observation.as_ref())
        .is_some_and(|observation| {
            matches!(
                observation.death,
                benilla_protocol_wrath::session::SelfDeath::Dead
                    | benilla_protocol_wrath::session::SelfDeath::Ghost
            ) || observation.resources.health == Some(0)
        });
    if dead {
        close_combat_window(&mut window, CombatCloseReason::Death);
    }
    let enabled = view.effective == CombatMode::Experimental;
    if *was_enabled && !enabled {
        close_combat_window(&mut window, CombatCloseReason::ModeExit);
    }
    *was_enabled = enabled;
}

#[cfg(test)]
mod souls_seam_tests {
    use super::*;

    fn proposal(sequence: u64, window: CombatWindowKind) -> CombatProposal {
        CombatProposal {
            sequence,
            window,
            trigger_spell_id: 133,
            iframe_active: matches!(window, CombatWindowKind::Dodge | CombatWindowKind::Roll),
            hyperarmor_active: false,
            stamina_cost: 25.0,
            window_ms: 600,
        }
    }

    /// Capability can only be negotiated by a server mode owner; tests set it directly to exercise
    /// the typed state machine. No production path in this checkout reaches this value.
    fn negotiated() -> CombatWindowState {
        let mut state = CombatWindowState::default();
        state.set_capability(CombatCapability::Negotiated);
        state
    }

    fn staged_decision(
        state: &mut CombatWindowState,
        sequence: u64,
        window: CombatWindowKind,
    ) -> CombatIntentDecision {
        assert!(stage_combat_proposal(state, proposal(sequence, window)));
        evaluate_combat_intent(state, true)
    }

    /// The default state refuses: no capability, no stamina, no window. Baseline class combat is
    /// untouched, and a staged proposal is drained rather than sent.
    #[test]
    fn disabled_and_unnegotiated_modes_keep_baseline_combat_playable() {
        let mut state = CombatWindowState::default();
        assert_eq!(state.capability(), CombatCapability::Missing);

        assert!(stage_combat_proposal(
            &mut state,
            proposal(1, CombatWindowKind::Dodge)
        ));
        assert_eq!(
            evaluate_combat_intent(&mut state, false),
            CombatIntentDecision::Disabled
        );
        assert!(
            state.staged().is_none(),
            "a disabled seam drains the proposal"
        );
        assert!(state.pending().is_none());
        assert_eq!(state.authoritative_stamina(), None);
        assert_eq!(state.last_drained(), Some(1));

        // Even with the mode on, the missing server contract refuses before anything is sent.
        assert_eq!(
            staged_decision(&mut state, 2, CombatWindowKind::Block),
            CombatIntentDecision::Refused(CombatIntentRefusal::NoServerContract)
        );
        assert!(state.pending().is_none());
        assert_eq!(state.authoritative_stamina(), None);
    }

    /// A negotiation turns a proposal into a `NotSent` request; only the server's answer settles
    /// it, and only an accepted answer writes stamina.
    #[test]
    fn server_acceptance_and_rejection_settle_once() {
        let mut state = negotiated();
        let decision = staged_decision(&mut state, 1, CombatWindowKind::Dodge);
        let CombatIntentDecision::NotSent(window) = decision else {
            panic!("a negotiated proposal is owed as NotSent, got {decision:?}");
        };
        assert_eq!(window.sequence, 1);
        assert!(window.iframe_active);
        assert!(state.pending().is_some());
        assert_eq!(
            state.authoritative_stamina(),
            None,
            "a proposal never debits stamina locally"
        );

        assert_eq!(
            apply_authoritative_combat_result(
                &mut state,
                CombatServerResult::Accepted {
                    sequence: 1,
                    stamina: Some(80),
                }
            ),
            CombatSettlement::Settled {
                sequence: 1,
                outcome: CombatSettlementOutcome::Accepted,
                stamina: Some(80),
            }
        );
        assert_eq!(state.authoritative_stamina(), Some(80));
        assert_eq!(
            state.settled(),
            Some((1, CombatSettlementOutcome::Accepted))
        );
        assert!(state.pending().is_none());

        // A rejected window settles as rejected and leaves the authoritative stamina alone.
        assert!(matches!(
            staged_decision(&mut state, 2, CombatWindowKind::Block),
            CombatIntentDecision::NotSent(_)
        ));
        assert_eq!(
            apply_authoritative_combat_result(
                &mut state,
                CombatServerResult::Rejected { sequence: 2 }
            ),
            CombatSettlement::Settled {
                sequence: 2,
                outcome: CombatSettlementOutcome::Rejected,
                stamina: None,
            }
        );
        assert_eq!(state.authoritative_stamina(), Some(80));
    }

    /// A duplicate settles nothing and a late answer for an older window cannot roll back a later
    /// authoritative hit or its stamina.
    #[test]
    fn duplicate_and_late_results_do_not_roll_back_a_later_hit() {
        let mut state = negotiated();
        assert!(matches!(
            staged_decision(&mut state, 1, CombatWindowKind::Dodge),
            CombatIntentDecision::NotSent(_)
        ));
        assert!(matches!(
            apply_authoritative_combat_result(
                &mut state,
                CombatServerResult::Accepted {
                    sequence: 1,
                    stamina: Some(60),
                }
            ),
            CombatSettlement::Settled { .. }
        ));
        assert!(matches!(
            staged_decision(&mut state, 2, CombatWindowKind::Roll),
            CombatIntentDecision::NotSent(_)
        ));
        assert!(matches!(
            apply_authoritative_combat_result(
                &mut state,
                CombatServerResult::Accepted {
                    sequence: 2,
                    stamina: Some(40),
                }
            ),
            CombatSettlement::Settled { .. }
        ));

        assert_eq!(
            apply_authoritative_combat_result(
                &mut state,
                CombatServerResult::Accepted {
                    sequence: 2,
                    stamina: Some(40),
                }
            ),
            CombatSettlement::Duplicate { sequence: 2 }
        );
        assert_eq!(
            apply_authoritative_combat_result(
                &mut state,
                CombatServerResult::Rejected { sequence: 1 }
            ),
            CombatSettlement::Late { sequence: 1 }
        );
        assert_eq!(
            state.authoritative_stamina(),
            Some(40),
            "the later authoritative hit stands"
        );
        assert_eq!(
            state.settled(),
            Some((2, CombatSettlementOutcome::Accepted))
        );
        assert_eq!(
            apply_authoritative_combat_result(
                &mut state,
                CombatServerResult::Unknown { sequence: 99 }
            ),
            CombatSettlement::Unmatched { sequence: 99 }
        );
    }

    /// Nothing on this seam debits stamina, deals damage, asserts a hit or owns a reward. Only a
    /// server-carried stamina value is stored, and only on an acceptance.
    #[test]
    fn stamina_and_damage_are_never_debited_locally() {
        let mut state = negotiated();
        assert!(stage_combat_proposal(
            &mut state,
            CombatProposal {
                stamina_cost: 100.0,
                ..proposal(1, CombatWindowKind::Stamina)
            }
        ));
        assert!(matches!(
            evaluate_combat_intent(&mut state, true),
            CombatIntentDecision::NotSent(_)
        ));
        assert_eq!(
            state.authoritative_stamina(),
            None,
            "the advisory cost must not become a local debit"
        );

        // An acceptance without a server stamina field leaves the observed value untouched.
        assert!(matches!(
            apply_authoritative_combat_result(
                &mut state,
                CombatServerResult::Accepted {
                    sequence: 1,
                    stamina: None,
                }
            ),
            CombatSettlement::Settled { .. }
        ));
        assert_eq!(state.authoritative_stamina(), None);
    }

    /// An unload/exit mid-window closes it as `Unknown` once, drains the staged proposal, and a
    /// late answer for it cannot settle a second time.
    #[test]
    fn a_mid_window_unload_settles_once() {
        let mut state = negotiated();
        assert!(matches!(
            staged_decision(&mut state, 1, CombatWindowKind::Dodge),
            CombatIntentDecision::NotSent(_)
        ));
        assert_eq!(
            close_combat_window(&mut state, CombatCloseReason::ModeExit),
            CombatClose::Unknown {
                sequence: 1,
                reason: CombatCloseReason::ModeExit,
            }
        );
        assert!(state.pending().is_none());
        assert_eq!(state.settled(), Some((1, CombatSettlementOutcome::Unknown)));
        assert_eq!(
            close_combat_window(&mut state, CombatCloseReason::ModeExit),
            CombatClose::Nothing,
            "the window settles once"
        );
        assert_eq!(
            apply_authoritative_combat_result(
                &mut state,
                CombatServerResult::Accepted {
                    sequence: 1,
                    stamina: Some(10),
                }
            ),
            CombatSettlement::Late { sequence: 1 }
        );
        assert_eq!(
            state.authoritative_stamina(),
            None,
            "a late acceptance cannot reopen the window"
        );

        // A proposal staged but never evaluated is drained too.
        assert!(stage_combat_proposal(
            &mut state,
            proposal(3, CombatWindowKind::Block)
        ));
        assert_eq!(
            close_combat_window(&mut state, CombatCloseReason::Death),
            CombatClose::Nothing
        );
        assert!(state.staged().is_none());
        assert_eq!(state.last_drained(), Some(3));
    }

    /// Death and disconnect close a pending window exactly once.
    #[test]
    fn death_and_disconnect_close_the_window_once() {
        for reason in [CombatCloseReason::Death, CombatCloseReason::Disconnect] {
            let mut state = negotiated();
            assert!(matches!(
                staged_decision(&mut state, 7, CombatWindowKind::Roll),
                CombatIntentDecision::NotSent(_)
            ));
            assert!(matches!(
                close_combat_window(&mut state, reason),
                CombatClose::Unknown { sequence: 7, .. }
            ));
            assert_eq!(
                close_combat_window(&mut state, reason),
                CombatClose::Nothing,
                "{reason:?} settles once"
            );
            assert_eq!(state.last_drained(), Some(7));
        }
    }

    /// Invalid proposals and protocol-order violations are refused rather than repaired.
    #[test]
    fn invalid_and_disordered_proposals_are_refused() {
        let mut state = negotiated();
        for invalid in [
            CombatProposal {
                sequence: 0,
                ..proposal(1, CombatWindowKind::Dodge)
            },
            CombatProposal {
                window_ms: 0,
                ..proposal(1, CombatWindowKind::Dodge)
            },
            CombatProposal {
                stamina_cost: f32::NAN,
                ..proposal(1, CombatWindowKind::Dodge)
            },
            CombatProposal {
                stamina_cost: -1.0,
                ..proposal(1, CombatWindowKind::Dodge)
            },
            // An i-frame belongs to a dodge or a roll alone.
            CombatProposal {
                iframe_active: true,
                ..proposal(1, CombatWindowKind::Block)
            },
        ] {
            let mut probe = CombatWindowState::default();
            probe.set_capability(CombatCapability::Negotiated);
            assert!(stage_combat_proposal(&mut probe, invalid));
            assert_eq!(
                evaluate_combat_intent(&mut probe, true),
                CombatIntentDecision::Refused(CombatIntentRefusal::InvalidProposal)
            );
            assert!(probe.pending().is_none());
        }

        assert!(matches!(
            staged_decision(&mut state, 1, CombatWindowKind::Dodge),
            CombatIntentDecision::NotSent(_)
        ));
        // One window in flight: a second proposal is refused, not queued.
        assert_eq!(
            staged_decision(&mut state, 2, CombatWindowKind::Block),
            CombatIntentDecision::Refused(CombatIntentRefusal::WindowOpen)
        );
        assert!(matches!(
            apply_authoritative_combat_result(
                &mut state,
                CombatServerResult::Rejected { sequence: 1 }
            ),
            CombatSettlement::Settled { .. }
        ));
        // A replayed sequence is a duplicate, even with no window open.
        assert_eq!(
            staged_decision(&mut state, 1, CombatWindowKind::Dodge),
            CombatIntentDecision::Refused(CombatIntentRefusal::DuplicateSequence)
        );
        // Only one proposal is staged per tick.
        assert!(stage_combat_proposal(
            &mut state,
            proposal(3, CombatWindowKind::Dodge)
        ));
        assert!(!stage_combat_proposal(
            &mut state,
            proposal(4, CombatWindowKind::Dodge)
        ));
    }
}
