//! Authored action data. Every timing and cost here is caller-supplied and validated.
//!
//! No preset in this crate is an extracted Elden Ring value. The donor's generated table is
//! intentionally absent. A caller that wants faithful numbers must supply and label its own data.

use crate::error::{CombatError, CombatResult};
use crate::identity::ActionRevision;
use crate::time::{TickRate, TickWindow};

/// The one dodge action the v1 model understands.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct DodgeSpec {
    /// Identifies the authored table this action came from. Inputs and snapshots must match it.
    pub revision: ActionRevision,
    /// Total action length in server ticks.
    pub total_ticks: u64,
    /// First offset at which any queued input is heard (donor `input_from`).
    pub input_from_tick: u64,
    /// First offset at which a dodge input is heard (donor `input_dodge_from`).
    ///
    /// The effective dodge threshold is `max(input_from_tick, dodge_from_tick)`.
    pub dodge_from_tick: u64,
    /// First offset at which a buffered dodge may replace the current action (donor
    /// `cancel_dodge`).
    pub cancel_dodge_from_tick: u64,
    /// First offset at which movement may replace the current action. Recorded for integration;
    /// the v1 core does not simulate locomotion.
    pub cancel_move_from_tick: u64,
    /// Invincibility offsets, half-open `[from, to)`. Must lie inside the action.
    pub iframes: TickWindow,
    /// Resource cost. Eligibility is `stamina > 0`, matching the donor; the spend clamps at zero.
    pub stamina_cost: u32,
}

impl DodgeSpec {
    /// The offset at which the donor starts hearing dodge input for this action.
    pub fn dodge_input_threshold(self) -> u64 {
        self.input_from_tick.max(self.dodge_from_tick)
    }
}

/// Stamina capacity and authored regeneration policy.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct StaminaConfig {
    pub max: u32,
    /// Restored per server tick once the delay has elapsed.
    pub regen_per_tick: u32,
    /// Ticks after a spend before regeneration resumes. This is an original integration policy;
    /// the primary donor has no post-spend delay.
    pub regen_delay_ticks: u64,
}

/// One telegraphed enemy attack: wind-up, active tick range, recovery, and damage.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct EnemySpec {
    pub max_hp: u32,
    pub windup_ticks: u64,
    pub active_ticks: u64,
    pub recovery_ticks: u64,
    pub damage: u32,
}

impl EnemySpec {
    pub fn total_ticks(self) -> Option<u64> {
        self.windup_ticks
            .checked_add(self.active_ticks)?
            .checked_add(self.recovery_ticks)
    }
}

/// Everything the offline core needs. Values are tick counts, never seconds or floats.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct CombatConfig {
    pub tick_rate: TickRate,
    pub player_max_hp: u32,
    pub dodge: DodgeSpec,
    pub stamina: StaminaConfig,
    pub enemy: EnemySpec,
    /// Hold time at or above which release is sprint rather than dodge. The donor's value is an
    /// ESTIMATE; this field makes the caller's choice explicit.
    pub sprint_hold_ticks: u64,
    /// Lifetime of the one-slot buffer. The donor's buffer has no TTL; this is an added bound.
    pub input_buffer_ttl_ticks: u64,
    /// How long a non-lethal hit keeps the player out of Ready.
    pub interrupt_recovery_ticks: u64,
    /// Largest accepted difference between an input's tick and the current server tick.
    pub max_input_lead_ticks: u64,
    /// Largest accepted sequence gap. A larger gap is treated as missing/out-of-order input.
    pub max_sequence_gap: u64,
}

impl CombatConfig {
    pub fn validate(&self) -> CombatResult<()> {
        if self.player_max_hp == 0 {
            return Err(CombatError::InvalidConfig("player_max_hp must be non-zero"));
        }
        if self.dodge.total_ticks == 0 {
            return Err(CombatError::InvalidConfig("dodge.total_ticks must be non-zero"));
        }
        if self.dodge.input_from_tick > self.dodge.total_ticks {
            return Err(CombatError::InvalidConfig(
                "dodge.input_from_tick must not exceed total_ticks",
            ));
        }
        if self.dodge.dodge_from_tick > self.dodge.total_ticks {
            return Err(CombatError::InvalidConfig(
                "dodge.dodge_from_tick must not exceed total_ticks",
            ));
        }
        if self.dodge.cancel_dodge_from_tick > self.dodge.total_ticks {
            return Err(CombatError::InvalidConfig(
                "dodge.cancel_dodge_from_tick must not exceed total_ticks",
            ));
        }
        if self.dodge.cancel_move_from_tick > self.dodge.total_ticks {
            return Err(CombatError::InvalidConfig(
                "dodge.cancel_move_from_tick must not exceed total_ticks",
            ));
        }
        if self.dodge.iframes.from >= self.dodge.iframes.to {
            return Err(CombatError::InvalidConfig(
                "dodge.iframes must be a non-empty half-open window",
            ));
        }
        if self.dodge.iframes.to > self.dodge.total_ticks {
            return Err(CombatError::InvalidConfig(
                "dodge.iframes must end within total_ticks",
            ));
        }
        if self.dodge.stamina_cost == 0 {
            return Err(CombatError::InvalidConfig("dodge.stamina_cost must be non-zero"));
        }
        if self.dodge.stamina_cost > self.stamina.max {
            return Err(CombatError::InvalidConfig(
                "dodge.stamina_cost must not exceed stamina.max",
            ));
        }
        if self.stamina.max == 0 {
            return Err(CombatError::InvalidConfig("stamina.max must be non-zero"));
        }
        if self.stamina.regen_per_tick == 0 {
            return Err(CombatError::InvalidConfig(
                "stamina.regen_per_tick must be non-zero",
            ));
        }
        if self.stamina.regen_delay_ticks == 0 {
            return Err(CombatError::InvalidConfig(
                "stamina.regen_delay_ticks must be non-zero",
            ));
        }
        if self.enemy.max_hp == 0 {
            return Err(CombatError::InvalidConfig("enemy.max_hp must be non-zero"));
        }
        if self.enemy.windup_ticks == 0 {
            return Err(CombatError::InvalidConfig(
                "enemy.windup_ticks must be non-zero",
            ));
        }
        if self.enemy.active_ticks == 0 {
            return Err(CombatError::InvalidConfig(
                "enemy.active_ticks must be non-zero",
            ));
        }
        if self.enemy.recovery_ticks == 0 {
            return Err(CombatError::InvalidConfig(
                "enemy.recovery_ticks must be non-zero",
            ));
        }
        if self.enemy.total_ticks().is_none() {
            return Err(CombatError::InvalidConfig("enemy timings overflow u64"));
        }
        if self.enemy.damage == 0 {
            return Err(CombatError::InvalidConfig("enemy.damage must be non-zero"));
        }
        if self.sprint_hold_ticks == 0 {
            return Err(CombatError::InvalidConfig(
                "sprint_hold_ticks must be non-zero",
            ));
        }
        if self.input_buffer_ttl_ticks == 0 {
            return Err(CombatError::InvalidConfig(
                "input_buffer_ttl_ticks must be non-zero",
            ));
        }
        if self.interrupt_recovery_ticks == 0 {
            return Err(CombatError::InvalidConfig(
                "interrupt_recovery_ticks must be non-zero",
            ));
        }
        if self.max_sequence_gap == 0 {
            return Err(CombatError::InvalidConfig(
                "max_sequence_gap must be non-zero",
            ));
        }
        Ok(())
    }
}
