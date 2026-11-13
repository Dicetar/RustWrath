//! Offline, server-authoritative combat core for Project B.
//!
//! This crate is intentionally small and dependency-free. It models one player, one telegraphed
//! enemy, one dodge, stamina, interruption, death/reset, bounded input, and reconciliation. It
//! does **not** render, animate, collide, transport network messages, or ship server code. It is
//! an integration seam, not deployed authority and not proof of a live dodge.
//!
//! **Authority:** callers in the authoritative server process own stamina, eligibility, and hit
//! outcomes. A client may predict and present a dodge, but an input frame is only a request. The
//! core grants i-frames only when it accepts a dodge; damage is produced only by the enemy hit
//! resolution inside [`CombatCore`].
//!
//! **Time:** all simulation time is caller-supplied monotonic [`Ticks`]. There is no wall clock,
//! no floating point, and no hidden timer. [`TickRate`] exists only to quantise adapter timestamps
//! upwards to the next server tick.
//!
//! **Data:** all action timings and costs come from a caller-supplied [`CombatConfig`]. No preset
//! here is an extracted Elden Ring value, and the donor's generated action table is not present.
//!
//! **Donor attribution:** mechanisms adapted from Funny-Bones/ELDEN-RING-Combat-Rewrite at
//! `177a3b5ec14a9bde39b9ab458a3400b4038224e3` (MIT): release-time dodge versus hold-to-sprint,
//! one-slot newest-wins input buffering, input/cancel/i-frame windows, explicit stamina cost and
//! clamp, and a `Windup/Active/Recovery` telegraph whose strike resolves once. The integer-tick
//! authority, session/sequence envelope, buffer TTL, regeneration delay, and reconciliation are
//! Project B integration code, not donor code. See
//! `docs/opencode/combat-donor-assessment.md`.

mod action;
mod core;
mod data;
mod enemy;
mod error;
mod identity;
mod input;
mod player;
mod policy;
mod profile;
mod stamina;
mod time;

pub use action::{dodge_action, ActionId, ActionKind, ActionSpec, ActionTable};
pub use crate::core::{
    AdvanceReport, CombatCore, CombatEvent, CombatSnapshot, MAX_ADVANCE_TICKS, MAX_PENDING_INPUTS,
    ReconcileReport, SubmitOutcome,
};
pub use data::{CombatConfig, DodgeSpec, EnemySpec, StaminaConfig};
pub use enemy::{EnemyPhase, EnemyView};
pub use error::{CombatError, CombatResult};
pub use identity::{ActionRevision, PlayerId, Sequence, SessionEpoch};
pub use input::{
    BufferedDodge, DodgeButton, DodgeRejection, DodgeResult, GestureOutcome, InputFrame, InputOutcome,
};
pub use policy::ProfilePolicy;
pub use profile::{
    dodge_from_clip, enemy_action, melee_action, spell_action, ClipSummary, SpellSummary,
};
pub use player::{DodgeView, HitOutcome, PlayerPhase, PlayerView};
pub use time::{TickRate, TickWindow, Ticks};
