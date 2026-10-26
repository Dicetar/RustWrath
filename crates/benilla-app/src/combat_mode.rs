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
        app.init_resource::<CombatModeView>().add_systems(
            Update,
            project_combat_mode
                .after(WorldStage::Net)
                .after(WorldStage::Input),
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
