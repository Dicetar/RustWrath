//! Native cast/channel lifecycle contract.
//!
//! The vanilla `CastBarEdge` is deliberately lossy at its terminal edges: `Stop`,
//! `Failed` and `Interrupted` carry no spell or cast identity. This module keeps the
//! 12340 facts needed by `UnitCastingInfo`, `UnitChannelInfo` and the native
//! `UNIT_SPELLCAST_*` payloads without inventing a second gameplay authority:
//!
//! - [`NativeCastState`] is the single authority that resolves wire facts, owned by the
//!   session IO thread. Each edge is resolved exactly once, [here](NativeCastState::apply).
//! - [`WrathResolvedCast`] hands the resolved edge plus the authoritative entries it left
//!   behind to the app, which mirrors them for presentation and never re-resolves.
//! - [`WrathCastEvent`] is the Bevy message carrying the resolved identity through
//!   terminal edges (launch, failure, interruption, channel end, scope loss).
//!
//! The authority has no UI clock and no Bevy dependency in its resolution path: the
//! `*_ui_now_seconds` fields stay `None` here and are stamped by the presentation drain
//! when it consumes a resolved edge. The `GetTime()` conversion is therefore explicit and
//! happens only in presentation.
//!
//! Clock domains are explicit. `observed_at_unix_ms` is the producer's packet-arrival wall
//! clock. `start_remaining_ms` is 12340's `SMSG_SPELL_START` `m_timer`, the time left when the
//! packet was written, not the full cast time. `server_time_ms` is 12340's game-time timestamp
//! on `SMSG_SPELL_GO`, a different clock. No start timestamp, cast id or total cast time is
//! invented here; the UI resolves display/rank and total time from its spell catalog.
//!
//! Interruptibility is `None` unless the wire actually establishes it (it does not in the
//! START/GO payloads). The consumer must use spell metadata for the "not interruptible" bit.

use std::collections::BTreeMap;

use bevy::prelude::*;

use benilla_protocol_wrath::gameplay::{ServerEvent as WrathGameplayEvent, SpellTargets};

/// The authoritative native cast/channel read model, owned by the session IO thread.
///
/// This is the only place wire facts are resolved: an edge updates these maps exactly once,
/// and the resolved event plus the entries it left for that caster are handed to the app as a
/// snapshot. The app-facing mirror never resolves the same fact again, and the WM export reads
/// these maps directly, so it needs no Bevy world, UI entity or presentation clock.
#[derive(Clone, Debug)]
pub struct NativeCastState {
    /// Active casts keyed by caster. Removed by the authoritative terminal edge.
    pub casts: BTreeMap<u64, WrathActiveCast>,
    /// Active channels keyed by caster.
    pub channels: BTreeMap<u64, WrathActiveChannel>,
    /// The last terminal cast edge, retained after its active entry is removed.
    pub last_cast_end: Option<WrathCastEnded>,
    /// Stable per-attempt handle source. Independent of wire `cast_count`.
    pub next_cast_serial: u64,
}

impl Default for NativeCastState {
    fn default() -> Self {
        Self {
            casts: BTreeMap::new(),
            channels: BTreeMap::new(),
            last_cast_end: None,
            next_cast_serial: 1,
        }
    }
}

/// One resolved native fact: the presentation edge and the authoritative entries the edge left
/// for its caster. The app mirrors exactly these values; absence (`None`) is the authority's
/// observed no-cast/no-channel for that caster.
#[derive(Clone, Debug, PartialEq)]
pub struct WrathResolvedCast {
    pub event: WrathCastEvent,
    /// The edge caster's active cast after resolution.
    pub casting: Option<(u64, WrathActiveCast)>,
    /// The edge caster's active channel after resolution.
    pub channeling: Option<(u64, WrathActiveChannel)>,
    /// The authority's retained last terminal cast edge after resolution.
    pub last_cast_end: Option<WrathCastEnded>,
}

/// A session/world/revision scope. Native state from an older scope is never valid.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct WrathScope {
    pub session_epoch: u64,
    pub world_generation: u64,
    pub state_revision: u64,
}

/// The facts that identify one cast attempt across its wire edges.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct WrathCastIdentity {
    pub caster: u64,
    pub spell_id: u32,
    /// 12340's `m_cast_count`; `None` only where the specific wire edge omits it.
    pub cast_count: Option<u8>,
    /// The cast item guid from START/GO when the first guid is not the caster.
    pub cast_item: Option<u64>,
}

/// A unit's observed health and all seven Wrath power pairs, in protocol units. Each field is
/// independently `Some` only when the server delivered that word; `Some(0)` is an observed zero,
/// `None` is unavailable. Display conversions (rage 0-1000, energy/runic 0-100, happiness) belong
/// to the UI owner, not this contract.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct WrathUnitResources {
    pub health: Option<u32>,
    pub max_health: Option<u32>,
    pub powers: [Option<u32>; 7],
    pub max_powers: [Option<u32>; 7],
    /// `UNIT_FIELD_BYTES_0` high byte: the protocol power type, not a displayed value.
    pub power_type: Option<u8>,
}

/// One active cast, as the native read model exposes it. Not a gameplay authority: a
/// terminal edge removes the entry, and the underlying owners still decide behavior.
#[derive(Clone, Debug, PartialEq)]
pub struct WrathActiveCast {
    pub scope: WrathScope,
    /// Stable per-attempt handle generated by this read model. It is not the wire `cast_count`
    /// and not a client-global line id; use it to pair the native edges of one attempt.
    pub attempt_serial: u64,
    pub identity: WrathCastIdentity,
    pub target: Option<u64>,
    /// The complete parsed target block: unit/gameobject/item/source/dest/transport/string are
    /// all retained, rather than collapsed into one mutually exclusive summary.
    pub targets: SpellTargets,
    /// 12340 `m_timer` from START: milliseconds left at packet write.
    pub start_remaining_ms: Option<u32>,
    /// 12340 START cast flags, preserved for the consumer.
    pub cast_flags: u32,
    pub ammo_display_id: Option<u32>,
    /// Producer packet-arrival wall clock in ms since the Unix epoch.
    pub observed_at_unix_ms: i64,
    /// `GetTime()` seconds when the app drained the START. This is the cast's timeline origin;
    /// it is not updated by later pushback/channel edges. `None` when no UI clock existed.
    pub start_ui_now_seconds: Option<f64>,
    /// `GetTime()` seconds of the latest resolved edge; updates on pushback, not the origin.
    pub last_update_ui_now_seconds: Option<f64>,
    /// Accumulated `SMSG_SPELL_DELAYED` pushback observed for this cast.
    pub pushback_ms: u32,
    /// 12340's GO game-time timestamp, only once the launch edge arrived.
    pub go_server_time_ms: Option<u32>,
    /// A local cancel command was issued; the authoritative terminal is still awaited.
    pub cancel_requested: bool,
    /// `None` when the wire does not establish interruptibility.
    pub interruptible: Option<bool>,
}

/// A resolved item-text reply (3.3.5 inscription text by item guid). No 1.12 owner exists for
/// this shape: the vanilla mail-text event addresses `(text_id, mail_id)` instead.
#[derive(Message, Clone, Debug, PartialEq, Eq)]
pub struct WrathItemText {
    pub scope: WrathScope,
    pub item_guid: u64,
    pub has_text: bool,
    pub text: String,
}

/// One native inventory failure, including the item identities and reason-specific tail the
/// shared vanilla event cannot carry. `reason` is the 12340 `EQUIP_ERR_*` code.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct WrathInventoryFailure {
    pub reason: u8,
    pub item: Option<u64>,
    pub item2: Option<u64>,
    pub bag_type: u8,
    pub required_level: Option<u32>,
    pub item_limit_category: Option<u32>,
}

/// One native feed item. Kept on its own queue so cast/channel traffic cannot displace lifecycle
/// edges from the bounded session queue.
#[derive(Clone, Debug, PartialEq)]
pub enum WrathNativeFeedEvent {
    /// One resolved cast/channel edge, carrying the authoritative entries it left for its caster.
    Cast(WrathResolvedCast),
    ItemText(WrathItemText),
    InventoryFailure {
        failure: WrathInventoryFailure,
        scope: WrathScope,
    },
}

impl WrathNativeFeedEvent {
    /// The session/world scope this native fact belongs to.
    pub fn scope(&self) -> WrathScope {
        match self {
            Self::Cast(resolved) => resolved.event.scope,
            Self::ItemText(text) => text.scope,
            Self::InventoryFailure { scope, .. } => *scope,
        }
    }
}

/// One active channel, keyed by caster.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct WrathActiveChannel {
    pub scope: WrathScope,
    pub caster: u64,
    pub spell_id: u32,
    pub duration_ms: u32,
    pub remaining_ms: u32,
    pub observed_at_unix_ms: i64,
    pub updated_at_unix_ms: i64,
    /// `GetTime()` seconds when START was drained; survives later updates for UI reload.
    pub start_ui_now_seconds: Option<f64>,
    /// `GetTime()` seconds of the latest channel update.
    pub last_update_ui_now_seconds: Option<f64>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum WrathCastEnd {
    Launched,
    Failed,
    Interrupted,
    ScopeLost,
}

/// The last terminal cast edge, retaining the identity after the active map entry is gone.
#[derive(Clone, Debug, PartialEq)]
pub struct WrathCastEnded {
    pub scope: WrathScope,
    pub attempt_serial: Option<u64>,
    pub identity: WrathCastIdentity,
    pub target: Option<u64>,
    pub targets: Option<SpellTargets>,
    pub start_ui_now_seconds: Option<f64>,
    pub ended_at_unix_ms: i64,
    pub outcome: WrathCastEnd,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum WrathScopeLoss {
    WorldTransfer,
    SessionEnded,
    SessionReplaced,
    /// The bounded presentation feed dropped resolved edges: the mirror is no longer a complete
    /// copy of the authority and is cleared until the next resolved edges repopulate it.
    FeedOverflow,
}

/// One wire fact from the IO thread, before identity resolution against the active map.
#[derive(Clone, Debug, PartialEq)]
pub enum WrathNativeUpdate {
    CastStart {
        scope: WrathScope,
        caster: u64,
        spell_id: u32,
        cast_count: u8,
        cast_item: Option<u64>,
        target: Option<u64>,
        targets: SpellTargets,
        remaining_ms: Option<u32>,
        cast_flags: u32,
        ammo_display_id: Option<u32>,
        observed_at_unix_ms: i64,
    },
    CastDelayed {
        scope: WrathScope,
        caster: u64,
        delay_ms: u32,
        observed_at_unix_ms: i64,
    },
    CastLaunched {
        scope: WrathScope,
        caster: u64,
        spell_id: u32,
        cast_count: u8,
        server_time_ms: u32,
        observed_at_unix_ms: i64,
    },
    CastFailed {
        scope: WrathScope,
        caster: u64,
        spell_id: u32,
        cast_count: u8,
        reason: u8,
        observed_at_unix_ms: i64,
    },
    CastInterrupted {
        scope: WrathScope,
        caster: u64,
        spell_id: u32,
        cast_count: u8,
        reason: u8,
        observed_at_unix_ms: i64,
    },
    ChannelStart {
        scope: WrathScope,
        caster: u64,
        spell_id: u32,
        duration_ms: u32,
        observed_at_unix_ms: i64,
    },
    ChannelUpdate {
        scope: WrathScope,
        caster: u64,
        remaining_ms: u32,
        observed_at_unix_ms: i64,
    },
}

/// The resolved native cast/channel edge for UI and WM consumers.
#[derive(Message, Clone, Debug, PartialEq)]
pub struct WrathCastEvent {
    pub scope: WrathScope,
    /// This model's stable attempt handle; `None` for an edge with no matching START.
    pub attempt_serial: Option<u64>,
    /// Resolved from the active map where the wire edge omits spell/identity.
    pub identity: Option<WrathCastIdentity>,
    pub target: Option<u64>,
    /// The full START target block where one is available; channel/scope edges carry `None`.
    pub targets: Option<SpellTargets>,
    pub observed_at_unix_ms: i64,
    /// `GetTime()` seconds when the attempt's START was drained; `None` if unavailable.
    pub start_ui_now_seconds: Option<f64>,
    /// `GetTime()` seconds when this edge was drained; `None` if unavailable. Delayed/terminal
    /// edges carry both the start and this edge clock, so pushback never rewrites the origin.
    pub ui_now_seconds: Option<f64>,
    pub kind: WrathCastEventKind,
}

#[derive(Clone, Debug, PartialEq)]
pub enum WrathCastEventKind {
    Started {
        remaining_ms: Option<u32>,
    },
    Delayed {
        delay_ms: u32,
        accumulated_ms: u32,
    },
    Launched {
        server_time_ms: u32,
    },
    Failed {
        reason: u8,
    },
    Interrupted {
        reason: u8,
    },
    ChannelStarted {
        duration_ms: u32,
    },
    ChannelUpdated {
        remaining_ms: u32,
    },
    ChannelEnded,
    ScopeInvalidated {
        reason: WrathScopeLoss,
    },
}

/// The caster whose entry an update can change.
impl WrathNativeUpdate {
    pub fn caster(&self) -> u64 {
        match self {
            Self::CastStart { caster, .. }
            | Self::CastDelayed { caster, .. }
            | Self::CastLaunched { caster, .. }
            | Self::CastFailed { caster, .. }
            | Self::CastInterrupted { caster, .. }
            | Self::ChannelStart { caster, .. }
            | Self::ChannelUpdate { caster, .. } => *caster,
        }
    }
}

impl NativeCastState {
    /// Resolve one wire update exactly once and return the edge with the authority's entries
    /// for that caster. `None` means the fact carried no resolvable state (e.g. a delay or a
    /// channel update with no active entry); nothing changed and no export is owed.
    pub fn apply(&mut self, update: WrathNativeUpdate) -> Option<WrathResolvedCast> {
        let caster = update.caster();
        let event = self.apply_event(update)?;
        Some(WrathResolvedCast {
            event,
            casting: self.casts.get(&caster).map(|cast| (caster, cast.clone())),
            channeling: self.channels.get(&caster).map(|channel| (caster, *channel)),
            last_cast_end: self.last_cast_end.clone(),
        })
    }

    fn take_matching_cast(
        &mut self,
        caster: u64,
        spell_id: u32,
        cast_count: u8,
    ) -> Option<WrathActiveCast> {
        let matches = self.casts.get(&caster).is_some_and(|cast| {
            cast.identity.spell_id == spell_id && cast.identity.cast_count == Some(cast_count)
        });
        if matches {
            self.casts.remove(&caster)
        } else {
            None
        }
    }

    /// Record that the local cancel command for `spell_id` was sent. This is not a terminal
    /// state: the authoritative failure/interruption/launch edge still decides the outcome.
    /// Returns whether an active attempt matched, so the caller can advance the revision.
    pub fn note_cancel_requested(&mut self, caster: u64, spell_id: u32) -> bool {
        if let Some(cast) = self.casts.get_mut(&caster) {
            if cast.identity.spell_id == spell_id {
                cast.cancel_requested = true;
                return true;
            }
        }
        false
    }

    /// Drop all native state for a scope loss. Silent: the app closes its own presentation from
    /// the same lifecycle trigger, and no wire fact is being resolved here.
    pub fn invalidate_scope(&mut self) {
        self.casts.clear();
        self.channels.clear();
        self.last_cast_end = None;
    }

    fn apply_event(&mut self, update: WrathNativeUpdate) -> Option<WrathCastEvent> {
        match update {
            WrathNativeUpdate::CastStart {
                scope,
                caster,
                spell_id,
                cast_count,
                cast_item,
                target,
                targets,
                remaining_ms,
                cast_flags,
                ammo_display_id,
                observed_at_unix_ms,
            } => {
                let attempt_serial = self.next_cast_serial;
                self.next_cast_serial = self.next_cast_serial.wrapping_add(1).max(1);
                let identity = WrathCastIdentity {
                    caster,
                    spell_id,
                    cast_count: Some(cast_count),
                    cast_item,
                };
                self.casts.insert(
                    caster,
                    WrathActiveCast {
                        scope,
                        attempt_serial,
                        identity,
                        target,
                        targets: targets.clone(),
                        start_remaining_ms: remaining_ms,
                        cast_flags,
                        ammo_display_id,
                        observed_at_unix_ms,
                        start_ui_now_seconds: None,
                        last_update_ui_now_seconds: None,
                        pushback_ms: 0,
                        go_server_time_ms: None,
                        cancel_requested: false,
                        interruptible: None,
                    },
                );
                Some(WrathCastEvent {
                    scope,
                    attempt_serial: Some(attempt_serial),
                    identity: Some(identity),
                    target,
                    targets: Some(targets),
                    observed_at_unix_ms,
                    start_ui_now_seconds: None,
                    ui_now_seconds: None,
                    kind: WrathCastEventKind::Started { remaining_ms },
                })
            }
            WrathNativeUpdate::CastDelayed {
                scope,
                caster,
                delay_ms,
                observed_at_unix_ms,
            } => {
                if let Some(cast) = self.casts.get_mut(&caster) {
                    cast.pushback_ms = cast.pushback_ms.saturating_add(delay_ms);
                    Some(WrathCastEvent {
                        scope,
                        attempt_serial: Some(cast.attempt_serial),
                        identity: Some(cast.identity),
                        target: cast.target,
                        targets: Some(cast.targets.clone()),
                        observed_at_unix_ms,
                        start_ui_now_seconds: cast.start_ui_now_seconds,
                        ui_now_seconds: None,
                        kind: WrathCastEventKind::Delayed {
                            delay_ms,
                            accumulated_ms: cast.pushback_ms,
                        },
                    })
                } else {
                    // A delay without an active START cannot be attached to an identity; do
                    // not manufacture one for the UI.
                    None
                }
            }
            WrathNativeUpdate::CastLaunched {
                scope,
                caster,
                spell_id,
                cast_count,
                server_time_ms,
                observed_at_unix_ms,
            } => {
                let ended = self.take_matching_cast(caster, spell_id, cast_count);
                let (identity, target, targets, attempt_serial, start_ui_now_seconds) = match ended
                {
                    Some(cast) => (
                        cast.identity,
                        cast.target,
                        Some(cast.targets),
                        Some(cast.attempt_serial),
                        cast.start_ui_now_seconds,
                    ),
                    None => (
                        WrathCastIdentity {
                            caster,
                            spell_id,
                            cast_count: Some(cast_count),
                            cast_item: None,
                        },
                        None,
                        None,
                        None,
                        None,
                    ),
                };
                self.last_cast_end = Some(WrathCastEnded {
                    scope,
                    attempt_serial,
                    identity,
                    target,
                    targets: targets.clone(),
                    start_ui_now_seconds,
                    ended_at_unix_ms: observed_at_unix_ms,
                    outcome: WrathCastEnd::Launched,
                });
                Some(WrathCastEvent {
                    scope,
                    attempt_serial,
                    identity: Some(identity),
                    target,
                    targets,
                    observed_at_unix_ms,
                    start_ui_now_seconds,
                    ui_now_seconds: None,
                    kind: WrathCastEventKind::Launched { server_time_ms },
                })
            }
            WrathNativeUpdate::CastFailed {
                scope,
                caster,
                spell_id,
                cast_count,
                reason,
                observed_at_unix_ms,
            } => {
                let ended = self.take_matching_cast(caster, spell_id, cast_count);
                let (identity, target, targets, attempt_serial, start_ui_now_seconds) = match ended
                {
                    Some(cast) => (
                        cast.identity,
                        cast.target,
                        Some(cast.targets),
                        Some(cast.attempt_serial),
                        cast.start_ui_now_seconds,
                    ),
                    None => (
                        WrathCastIdentity {
                            caster,
                            spell_id,
                            cast_count: Some(cast_count),
                            cast_item: None,
                        },
                        None,
                        None,
                        None,
                        None,
                    ),
                };
                self.last_cast_end = Some(WrathCastEnded {
                    scope,
                    attempt_serial,
                    identity,
                    target,
                    targets: targets.clone(),
                    start_ui_now_seconds,
                    ended_at_unix_ms: observed_at_unix_ms,
                    outcome: WrathCastEnd::Failed,
                });
                Some(WrathCastEvent {
                    scope,
                    attempt_serial,
                    identity: Some(identity),
                    target,
                    targets,
                    observed_at_unix_ms,
                    start_ui_now_seconds,
                    ui_now_seconds: None,
                    kind: WrathCastEventKind::Failed { reason },
                })
            }
            WrathNativeUpdate::CastInterrupted {
                scope,
                caster,
                spell_id,
                cast_count,
                reason,
                observed_at_unix_ms,
            } => {
                let ended = self.take_matching_cast(caster, spell_id, cast_count);
                let (identity, target, targets, attempt_serial, start_ui_now_seconds) = match ended
                {
                    Some(cast) => (
                        cast.identity,
                        cast.target,
                        Some(cast.targets),
                        Some(cast.attempt_serial),
                        cast.start_ui_now_seconds,
                    ),
                    None => (
                        WrathCastIdentity {
                            caster,
                            spell_id,
                            cast_count: Some(cast_count),
                            cast_item: None,
                        },
                        None,
                        None,
                        None,
                        None,
                    ),
                };
                self.last_cast_end = Some(WrathCastEnded {
                    scope,
                    attempt_serial,
                    identity,
                    target,
                    targets: targets.clone(),
                    start_ui_now_seconds,
                    ended_at_unix_ms: observed_at_unix_ms,
                    outcome: WrathCastEnd::Interrupted,
                });
                Some(WrathCastEvent {
                    scope,
                    attempt_serial,
                    identity: Some(identity),
                    target,
                    targets,
                    observed_at_unix_ms,
                    start_ui_now_seconds,
                    ui_now_seconds: None,
                    kind: WrathCastEventKind::Interrupted { reason },
                })
            }
            WrathNativeUpdate::ChannelStart {
                scope,
                caster,
                spell_id,
                duration_ms,
                observed_at_unix_ms,
            } => {
                self.channels.insert(
                    caster,
                    WrathActiveChannel {
                        scope,
                        caster,
                        spell_id,
                        duration_ms,
                        remaining_ms: duration_ms,
                        observed_at_unix_ms,
                        updated_at_unix_ms: observed_at_unix_ms,
                        start_ui_now_seconds: None,
                        last_update_ui_now_seconds: None,
                    },
                );
                Some(WrathCastEvent {
                    scope,
                    attempt_serial: None,
                    identity: Some(WrathCastIdentity {
                        caster,
                        spell_id,
                        cast_count: None,
                        cast_item: None,
                    }),
                    target: None,
                    targets: None,
                    observed_at_unix_ms,
                    start_ui_now_seconds: None,
                    ui_now_seconds: None,
                    kind: WrathCastEventKind::ChannelStarted { duration_ms },
                })
            }
            WrathNativeUpdate::ChannelUpdate {
                scope,
                caster,
                remaining_ms,
                observed_at_unix_ms,
            } => {
                let ended = if remaining_ms == 0 {
                    self.channels.remove(&caster)
                } else {
                    self.channels.get_mut(&caster).map(|channel| {
                        channel.remaining_ms = remaining_ms;
                        channel.updated_at_unix_ms = observed_at_unix_ms;
                        *channel
                    })
                };
                ended.map(|channel| WrathCastEvent {
                    scope,
                    attempt_serial: None,
                    identity: Some(WrathCastIdentity {
                        caster,
                        spell_id: channel.spell_id,
                        cast_count: None,
                        cast_item: None,
                    }),
                    target: None,
                    targets: None,
                    observed_at_unix_ms,
                    start_ui_now_seconds: channel.start_ui_now_seconds,
                    ui_now_seconds: None,
                    kind: if remaining_ms == 0 {
                        WrathCastEventKind::ChannelEnded
                    } else {
                        WrathCastEventKind::ChannelUpdated { remaining_ms }
                    },
                })
            }
        }
    }
}

/// Drop the app's mirrored native state for a scope loss, returning the presentation events the
/// UI needs to close its bars. Presentation only: the authority cleared itself from the same
/// lifecycle trigger and does not invent success or failure (`ScopeLost`).
pub(crate) fn invalidate_presentation(
    view: &mut crate::wrath::WrathWorldView,
    reason: WrathScopeLoss,
    observed_at_unix_ms: i64,
    ui_now_seconds: Option<f64>,
) -> Vec<WrathCastEvent> {
    let mut events = Vec::new();
    for (_, cast) in std::mem::take(&mut view.casts) {
        view.last_cast_end = Some(WrathCastEnded {
            scope: cast.scope,
            attempt_serial: Some(cast.attempt_serial),
            identity: cast.identity,
            target: cast.target,
            targets: Some(cast.targets.clone()),
            start_ui_now_seconds: cast.start_ui_now_seconds,
            ended_at_unix_ms: observed_at_unix_ms,
            outcome: WrathCastEnd::ScopeLost,
        });
        events.push(WrathCastEvent {
            scope: cast.scope,
            attempt_serial: Some(cast.attempt_serial),
            identity: Some(cast.identity),
            target: cast.target,
            targets: Some(cast.targets),
            observed_at_unix_ms,
            start_ui_now_seconds: cast.start_ui_now_seconds,
            ui_now_seconds,
            kind: WrathCastEventKind::ScopeInvalidated { reason },
        });
    }
    for (_, channel) in std::mem::take(&mut view.channels) {
        events.push(WrathCastEvent {
            scope: channel.scope,
            attempt_serial: None,
            identity: Some(WrathCastIdentity {
                caster: channel.caster,
                spell_id: channel.spell_id,
                cast_count: None,
                cast_item: None,
            }),
            target: None,
            targets: None,
            observed_at_unix_ms,
            start_ui_now_seconds: channel.start_ui_now_seconds,
            ui_now_seconds,
            kind: WrathCastEventKind::ScopeInvalidated { reason },
        });
    }
    view.last_cast_end = None;
    events
}

/// Translate one parsed gameplay event into a wire-fact update. `scope` and the arrival time
/// come from the session loop, not from a UI clock.
pub(crate) fn update_from_gameplay(
    event: &WrathGameplayEvent,
    scope: WrathScope,
    observed_at_unix_ms: i64,
    self_guid: u64,
) -> Option<WrathNativeUpdate> {
    Some(match event {
        WrathGameplayEvent::SpellStart {
            caster,
            spell_id,
            cast_flags,
            cast_count,
            cast_item,
            cast_time_ms,
            targets,
            ammo_display_id,
        } => WrathNativeUpdate::CastStart {
            scope,
            caster: *caster,
            spell_id: *spell_id,
            cast_count: *cast_count,
            cast_item: *cast_item,
            target: targets.unit,
            targets: (*targets).clone(),
            remaining_ms: (*cast_time_ms >= 0).then_some(*cast_time_ms as u32),
            cast_flags: *cast_flags,
            ammo_display_id: *ammo_display_id,
            observed_at_unix_ms,
        },
        WrathGameplayEvent::SpellDelayed { caster, delay_ms } => {
            WrathNativeUpdate::CastDelayed {
                scope,
                caster: *caster,
                delay_ms: *delay_ms,
                observed_at_unix_ms,
            }
        }
        WrathGameplayEvent::SpellGo {
            caster,
            spell_id,
            cast_count,
            server_time_ms,
            ..
        } => WrathNativeUpdate::CastLaunched {
            scope,
            caster: *caster,
            spell_id: *spell_id,
            cast_count: *cast_count,
            server_time_ms: *server_time_ms,
            observed_at_unix_ms,
        },
        WrathGameplayEvent::CastFailed {
            spell_id,
            cast_count,
            raw_reason,
            ..
        } => WrathNativeUpdate::CastFailed {
            scope,
            // `SMSG_CAST_FAILED` names no caster: it is the local client's own attempt.
            caster: self_guid,
            spell_id: *spell_id,
            cast_count: *cast_count,
            reason: *raw_reason,
            observed_at_unix_ms,
        },
        WrathGameplayEvent::SpellFailure {
            caster,
            spell_id,
            cast_count,
            raw_reason,
            ..
        } => WrathNativeUpdate::CastInterrupted {
            scope,
            caster: *caster,
            spell_id: *spell_id,
            cast_count: *cast_count,
            reason: *raw_reason,
            observed_at_unix_ms,
        },
        WrathGameplayEvent::ChannelStart {
            caster,
            spell_id,
            duration_ms,
        } => WrathNativeUpdate::ChannelStart {
            scope,
            caster: *caster,
            spell_id: *spell_id,
            duration_ms: *duration_ms,
            observed_at_unix_ms,
        },
        WrathGameplayEvent::ChannelUpdate {
            caster,
            remaining_ms,
        } => WrathNativeUpdate::ChannelUpdate {
            scope,
            caster: *caster,
            remaining_ms: *remaining_ms,
            observed_at_unix_ms,
        },
        _ => return None,
    })
}

/// The WM observation block for the self unit's active cast, read from the authority. The UI
/// clocks stay `None`: the authority has no client-process clock, and presentation stamps its
/// own mirror when it drains the resolved edge.
pub(crate) fn cast_observation(
    state: &NativeCastState,
    player_guid: u64,
) -> benilla_wm_native::CastObservation {
    let Some(cast) = state.casts.get(&player_guid) else {
        return benilla_wm_native::CastObservation::none();
    };
    benilla_wm_native::CastObservation {
        state: benilla_wm_native::ActivityState::Active,
        attempt_serial: cast.attempt_serial,
        spell_id: cast.identity.spell_id,
        cast_count: cast.identity.cast_count,
        cast_item: cast.identity.cast_item,
        targets: Some(spell_targets_observation(&cast.targets)),
        cast_flags: Some(cast.cast_flags),
        ammo_display_id: cast.ammo_display_id,
        start_remaining_ms: cast.start_remaining_ms,
        pushback_ms: cast.pushback_ms,
        cancel_requested: cast.cancel_requested,
        interruptible: cast.interruptible,
        go_server_time_ms: cast.go_server_time_ms,
        start_observed_at_unix_ms: cast.observed_at_unix_ms,
        start_ui_now_seconds: None,
        last_update_ui_now_seconds: None,
    }
}

/// The WM observation block for the self unit's active channel, read from the authority.
pub(crate) fn channel_observation(
    state: &NativeCastState,
    player_guid: u64,
) -> benilla_wm_native::ChannelObservation {
    let Some(channel) = state.channels.get(&player_guid) else {
        return benilla_wm_native::ChannelObservation::none();
    };
    benilla_wm_native::ChannelObservation {
        state: benilla_wm_native::ActivityState::Active,
        spell_id: Some(channel.spell_id),
        duration_ms: Some(channel.duration_ms),
        remaining_ms: Some(channel.remaining_ms),
        start_observed_at_unix_ms: Some(channel.observed_at_unix_ms),
        update_observed_at_unix_ms: Some(channel.updated_at_unix_ms),
        start_ui_now_seconds: None,
        last_update_ui_now_seconds: None,
    }
}

/// The WM observation block for the self unit's last terminal cast edge. A retained edge from an
/// older scope is never presented as current.
pub(crate) fn last_cast_end_observation(
    state: &NativeCastState,
    player_guid: u64,
    session_epoch: u64,
    world_generation: u64,
) -> benilla_wm_native::LastCastEndObservation {
    let ended = state.last_cast_end.as_ref().filter(|ended| {
        ended.identity.caster == player_guid
            && ended.scope.session_epoch == session_epoch
            && ended.scope.world_generation == world_generation
    });
    let Some(ended) = ended else {
        return benilla_wm_native::LastCastEndObservation::unknown();
    };
    benilla_wm_native::LastCastEndObservation {
        state: benilla_wm_native::TerminalState::Present,
        outcome: match ended.outcome {
            WrathCastEnd::Launched => benilla_wm_native::CastOutcome::Launched,
            WrathCastEnd::Failed => benilla_wm_native::CastOutcome::Failed,
            WrathCastEnd::Interrupted => benilla_wm_native::CastOutcome::Interrupted,
            WrathCastEnd::ScopeLost => benilla_wm_native::CastOutcome::ScopeLost,
        },
        attempt_serial: ended.attempt_serial,
        spell_id: Some(ended.identity.spell_id),
        cast_count: ended.identity.cast_count,
        cast_item: ended.identity.cast_item,
        target: ended.target,
        targets: ended.targets.as_ref().map(spell_targets_observation),
        ended_observed_at_unix_ms: Some(ended.ended_at_unix_ms),
    }
}

/// The WM observation block for the self unit's observed aura slots: raw wire values with the
/// own-aura marker preserved, sorted by slot and explicitly bounded (never silently truncated).
/// `observed` is the owner's aura-packet coverage for this object generation: until the server
/// has delivered an aura packet for the unit, the empty slot vector is unknown, not observed
/// empty (the forced empty `SMSG_AURA_UPDATE_ALL` is what turns it into a known empty set).
pub(crate) fn auras_observation(
    slots: &[benilla_protocol_wrath::reducer::AuraSlot],
    observed: bool,
) -> benilla_wm_native::AurasObservation {
    if !observed {
        return benilla_wm_native::AurasObservation::unknown();
    }
    let mut by_slot = BTreeMap::new();
    for slot in slots {
        by_slot.insert(
            slot.slot,
            benilla_wm_native::AuraObservation {
                slot: slot.slot,
                spell_id: slot.spell_id,
                flags: slot.flags,
                level: slot.level,
                charges: slot.charges,
                caster_guid: slot.caster,
                max_duration_ms: slot.max_duration_ms,
                duration_ms: slot.duration_ms,
            },
        );
    }
    let total = by_slot.len();
    let slots: Vec<_> = by_slot
        .into_values()
        .take(benilla_wm_native::MAX_AURA_SLOTS)
        .collect();
    benilla_wm_native::AurasObservation {
        state: benilla_wm_native::AuraKnowledge::Known,
        complete: total <= benilla_wm_native::MAX_AURA_SLOTS,
        slots,
    }
}

fn spell_targets_observation(targets: &SpellTargets) -> benilla_wm_native::SpellTargetsObservation {
    benilla_wm_native::SpellTargetsObservation {
        mask: targets.mask,
        unit: targets.unit,
        gameobject: targets.gameobject,
        item: targets.item,
        source: targets.source,
        source_transport: targets.source_transport,
        dest: targets.dest,
        dest_transport: targets.dest_transport,
        string: targets.string.clone(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use benilla_protocol_wrath::reducer::AuraSlot;

    fn scope() -> WrathScope {
        WrathScope {
            session_epoch: 7,
            world_generation: 3,
            state_revision: 11,
        }
    }

    fn empty_targets() -> SpellTargets {
        SpellTargets {
            mask: 0,
            unit: None,
            gameobject: None,
            item: None,
            source: None,
            source_transport: None,
            dest: None,
            dest_transport: None,
            string: None,
        }
    }

    fn start(guid: u64, spell_id: u32, count: u8, target: u64) -> WrathNativeUpdate {
        let mut targets = empty_targets();
        targets.mask = 0x2;
        targets.unit = Some(target);
        WrathNativeUpdate::CastStart {
            scope: scope(),
            caster: guid,
            spell_id,
            cast_count: count,
            cast_item: None,
            target: Some(target),
            targets,
            remaining_ms: Some(1_500),
            cast_flags: 0x100,
            ammo_display_id: None,
            observed_at_unix_ms: 10_000,
        }
    }

    #[test]
    fn cast_edges_resolve_once_in_the_authority() {
        let guid = 0xA11CE;
        let mut state = NativeCastState::default();

        let started = state.apply(start(guid, 133, 4, 0xBEEF)).expect("start resolves");
        assert_eq!(started.event.kind, WrathCastEventKind::Started { remaining_ms: Some(1_500) });
        let (caster, cast) = started.casting.expect("authority holds the active cast");
        assert_eq!(caster, guid);
        assert_ne!(cast.attempt_serial, 0);
        assert_eq!(cast.identity.cast_count, Some(4));
        assert_eq!(cast.targets.unit, Some(0xBEEF), "the complete target block survives");
        assert_eq!(cast.start_ui_now_seconds, None, "the authority has no UI clock");
        assert_eq!(started.last_cast_end, None);

        let block = cast_observation(&state, guid);
        assert_eq!(block.state, benilla_wm_native::ActivityState::Active);
        assert_eq!(block.attempt_serial, cast.attempt_serial);
        assert_eq!(block.start_ui_now_seconds, None);
        assert_eq!(block.targets.as_ref().and_then(|t| t.unit), Some(0xBEEF));
        assert!(!block.cancel_requested);
        block.validate().expect("the active block is schema-valid");

        let delayed = state
            .apply(WrathNativeUpdate::CastDelayed {
                scope: scope(),
                caster: guid,
                delay_ms: 250,
                observed_at_unix_ms: 10_100,
            })
            .expect("the delay attaches to the live attempt");
        assert_eq!(
            delayed.event.kind,
            WrathCastEventKind::Delayed {
                delay_ms: 250,
                accumulated_ms: 250
            }
        );
        let (_, cast) = delayed.casting.expect("still casting");
        assert_eq!(cast.attempt_serial, started.event.attempt_serial.unwrap());
        assert_eq!(cast.observed_at_unix_ms, 10_000, "pushback never rewrites the origin");
        assert_eq!(cast_observation(&state, guid).pushback_ms, 250);

        let launched = state
            .apply(WrathNativeUpdate::CastLaunched {
                scope: scope(),
                caster: guid,
                spell_id: 133,
                cast_count: 4,
                server_time_ms: 86_399_000,
                observed_at_unix_ms: 10_200,
            })
            .expect("launch resolves");
        assert_eq!(launched.casting, None, "the terminal edge removes the active entry");
        let ended = launched.last_cast_end.expect("the terminal edge is retained");
        assert_eq!(ended.outcome, WrathCastEnd::Launched);
        assert_eq!(ended.attempt_serial, started.event.attempt_serial);
        assert_eq!(ended.targets.as_ref().and_then(|t| t.unit), Some(0xBEEF));

        let channel = channel_observation(&state, guid);
        assert_eq!(channel.state, benilla_wm_native::ActivityState::None);
        channel.validate().expect("an observed no-channel is schema-valid");
        let block = cast_observation(&state, guid);
        assert_eq!(block.state, benilla_wm_native::ActivityState::None);
        block.validate().expect("an observed no-cast is schema-valid");
        let terminal = last_cast_end_observation(&state, guid, 7, 3);
        assert_eq!(terminal.state, benilla_wm_native::TerminalState::Present);
        assert_eq!(terminal.outcome, benilla_wm_native::CastOutcome::Launched);
        assert_eq!(terminal.attempt_serial, started.event.attempt_serial);
        terminal.validate().expect("the terminal block is schema-valid");
        assert_eq!(
            last_cast_end_observation(&state, guid, 7, 4).state,
            benilla_wm_native::TerminalState::Unknown,
            "an edge from a retired world scope is never presented as current"
        );
    }

    #[test]
    fn terminal_edge_without_start_never_invents_an_attempt() {
        let guid = 0xA11CE;
        let mut state = NativeCastState::default();
        let resolved = state
            .apply(WrathNativeUpdate::CastFailed {
                scope: scope(),
                caster: guid,
                spell_id: 116,
                cast_count: 9,
                reason: 100,
                observed_at_unix_ms: 20_000,
            })
            .expect("the failure resolves");
        assert_eq!(resolved.event.kind, WrathCastEventKind::Failed { reason: 100 });
        assert_eq!(resolved.casting, None);
        let ended = resolved.last_cast_end.expect("retained");
        assert_eq!(ended.outcome, WrathCastEnd::Failed);
        assert_eq!(ended.attempt_serial, None);
        assert_eq!(ended.identity.spell_id, 116);
        let terminal = last_cast_end_observation(&state, guid, 7, 3);
        assert_eq!(terminal.outcome, benilla_wm_native::CastOutcome::Failed);
        terminal.validate().expect("schema-valid");
    }

    #[test]
    fn channel_edges_carry_both_clock_anchors() {
        let guid = 0xA11CE;
        let mut state = NativeCastState::default();
        let started = state
            .apply(WrathNativeUpdate::ChannelStart {
                scope: scope(),
                caster: guid,
                spell_id: 5143,
                duration_ms: 4_000,
                observed_at_unix_ms: 30_000,
            })
            .expect("channel start resolves");
        assert_eq!(
            started.event.kind,
            WrathCastEventKind::ChannelStarted { duration_ms: 4_000 }
        );
        let (_, channel) = started.channeling.expect("active channel");
        assert_eq!(channel.start_ui_now_seconds, None);
        let block = channel_observation(&state, guid);
        assert_eq!(block.state, benilla_wm_native::ActivityState::Active);
        assert_eq!(block.start_observed_at_unix_ms, Some(30_000));
        assert_eq!(block.update_observed_at_unix_ms, Some(30_000));
        assert_eq!(block.start_ui_now_seconds, None);
        block.validate().expect("schema-valid");

        let updated = state
            .apply(WrathNativeUpdate::ChannelUpdate {
                scope: scope(),
                caster: guid,
                remaining_ms: 2_500,
                observed_at_unix_ms: 31_500,
            })
            .expect("channel update resolves");
        assert_eq!(
            updated.event.kind,
            WrathCastEventKind::ChannelUpdated { remaining_ms: 2_500 }
        );
        let block = channel_observation(&state, guid);
        assert_eq!(block.start_observed_at_unix_ms, Some(30_000), "the anchor survives");
        assert_eq!(block.update_observed_at_unix_ms, Some(31_500));
        assert_eq!(block.remaining_ms, Some(2_500));

        let ended = state
            .apply(WrathNativeUpdate::ChannelUpdate {
                scope: scope(),
                caster: guid,
                remaining_ms: 0,
                observed_at_unix_ms: 32_000,
            })
            .expect("the completing update resolves");
        assert_eq!(ended.event.kind, WrathCastEventKind::ChannelEnded);
        assert_eq!(ended.channeling, None);
        let block = channel_observation(&state, guid);
        assert_eq!(block.state, benilla_wm_native::ActivityState::None);
        block.validate().expect("schema-valid");
    }

    #[test]
    fn cancel_is_authority_state_not_an_outcome() {
        let guid = 0xA11CE;
        let mut state = NativeCastState::default();
        state.apply(start(guid, 133, 4, 0xBEEF)).expect("start");
        assert!(!state.note_cancel_requested(guid, 999), "another spell does not match");
        assert!(state.note_cancel_requested(guid, 133));
        let block = cast_observation(&state, guid);
        assert!(block.cancel_requested);
        assert_eq!(block.state, benilla_wm_native::ActivityState::Active, "not an outcome");
        block.validate().expect("schema-valid");
        assert_eq!(state.last_cast_end.as_ref().map(|e| e.outcome), None);
    }

    #[test]
    fn scope_loss_clears_every_authority_entry() {
        let guid = 0xA11CE;
        let mut state = NativeCastState::default();
        state.apply(start(guid, 133, 4, 0xBEEF)).expect("start");
        state
            .apply(WrathNativeUpdate::ChannelStart {
                scope: scope(),
                caster: guid,
                spell_id: 5143,
                duration_ms: 4_000,
                observed_at_unix_ms: 30_000,
            })
            .expect("channel");
        state.invalidate_scope();
        assert!(state.casts.is_empty());
        assert!(state.channels.is_empty());
        assert!(state.last_cast_end.is_none());
        assert_eq!(
            cast_observation(&state, guid).state,
            benilla_wm_native::ActivityState::None,
            "the fresh scope observes no active cast"
        );
        assert_eq!(
            last_cast_end_observation(&state, guid, 8, 4).state,
            benilla_wm_native::TerminalState::Unknown
        );
    }

    #[test]
    fn aura_export_is_sorted_unique_and_bounded() {
        let mut slots = Vec::new();
        for i in 0..130u8 {
            slots.push(AuraSlot {
                slot: 255 - i,
                spell_id: u32::from(i) + 1,
                flags: 0x08,
                level: 80,
                charges: 1,
                caster: 0,
                max_duration_ms: 1_000,
                duration_ms: 500,
            });
        }
        slots[0].slot = slots[1].slot; // a duplicate slot keeps the last row only
        let export = auras_observation(&slots, true);
        assert_eq!(export.state, benilla_wm_native::AuraKnowledge::Known);
        assert!(!export.complete, "the bound cut the set; never silently complete");
        assert_eq!(export.slots.len(), benilla_wm_native::MAX_AURA_SLOTS);
        assert!(
            export.slots.windows(2).all(|pair| pair[0].slot < pair[1].slot),
            "slots sorted ascending and unique"
        );
        export.validate().expect("schema-valid");

        let empty = auras_observation(&[], true);
        assert_eq!(empty.state, benilla_wm_native::AuraKnowledge::Known);
        assert!(empty.slots.is_empty(), "an observed zero-aura state is not unknown");
        assert!(empty.complete);
    }
}

