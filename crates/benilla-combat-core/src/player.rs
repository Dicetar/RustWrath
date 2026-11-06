//! Player lifecycle and the one dodge action.
//!
//! Donor mapping: `Ground` becomes [`PlayerPhase::Ready`], `Act` is split into
//! `Windup/Active/Recovery` by the dodge's i-frame window, a non-lethal hit becomes
//! [`PlayerPhase::Interrupted`], and `Dead` stays `Dead`. The donor keeps a separate
//! `invincible()` check; the split lifecycle makes that eligibility explicit instead.

use crate::data::CombatConfig;
use crate::identity::ActionRevision;
use crate::stamina::Stamina;
use crate::time::{TickWindow, Ticks};

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum PlayerPhase {
    Ready,
    Windup,
    Active,
    Recovery,
    Interrupted,
    Dead,
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum HitOutcome {
    /// The player was already dead; no state changed.
    IgnoredDead,
    /// The active dodge's half-open i-frame window covered the hit tick.
    Dodged,
    Hit,
    Killed,
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub(crate) struct ActiveDodge {
    pub(crate) revision: ActionRevision,
    pub(crate) started_at: Ticks,
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct DodgeView {
    pub revision: ActionRevision,
    pub started_at: Ticks,
    pub elapsed_ticks: u64,
    pub iframes: TickWindow,
    pub cancel_dodge_from_tick: u64,
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct PlayerView {
    pub hp: u32,
    pub max_hp: u32,
    pub stamina: u32,
    pub max_stamina: u32,
    pub phase: PlayerPhase,
    pub dodge: Option<DodgeView>,
    pub interrupted_until: Option<Ticks>,
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub(crate) struct Player {
    pub(crate) hp: u32,
    pub(crate) max_hp: u32,
    pub(crate) stamina: Stamina,
    pub(crate) phase: PlayerPhase,
    pub(crate) dodge: Option<ActiveDodge>,
    pub(crate) interrupted_until: Option<Ticks>,
}

impl Player {
    pub(crate) fn new(config: &CombatConfig) -> Self {
        Self {
            hp: config.player_max_hp,
            max_hp: config.player_max_hp,
            stamina: Stamina::new(config.stamina.max),
            phase: PlayerPhase::Ready,
            dodge: None,
            interrupted_until: None,
        }
    }

    pub(crate) fn view(&self, now: Ticks, config: &CombatConfig) -> PlayerView {
        PlayerView {
            hp: self.hp,
            max_hp: self.max_hp,
            stamina: self.stamina.current(),
            max_stamina: self.stamina.max(),
            phase: self.phase,
            dodge: self.dodge.map(|dodge| DodgeView {
                revision: dodge.revision,
                started_at: dodge.started_at,
                elapsed_ticks: now.elapsed_since(dodge.started_at),
                iframes: config.dodge.iframes,
                cancel_dodge_from_tick: config.dodge.cancel_dodge_from_tick,
            }),
            interrupted_until: self.interrupted_until,
        }
    }

    pub(crate) fn is_dead(&self) -> bool {
        self.hp == 0
    }

    pub(crate) fn dodge_started_at(&self) -> Option<Ticks> {
        self.dodge.map(|dodge| dodge.started_at)
    }

    pub(crate) fn active_revision(&self) -> Option<ActionRevision> {
        self.dodge.map(|dodge| dodge.revision)
    }

    pub(crate) fn interrupted_until(&self) -> Option<Ticks> {
        self.interrupted_until
    }

    pub(crate) fn dodge_elapsed(&self, now: Ticks) -> Option<u64> {
        self.dodge.map(|dodge| now.elapsed_since(dodge.started_at))
    }

    /// Recompute the displayed phase and expire a finished action. Returns `true` when the dodge
    /// ended on this call, so the caller can emit exactly one end event.
    pub(crate) fn sync_phase(&mut self, now: Ticks, config: &CombatConfig) -> bool {
        if self.hp == 0 {
            self.phase = PlayerPhase::Dead;
            self.dodge = None;
            self.interrupted_until = None;
            return false;
        }

        if let Some(dodge) = self.dodge {
            let elapsed = now.elapsed_since(dodge.started_at);
            if elapsed >= config.dodge.total_ticks {
                self.dodge = None;
                if let Some(until) = self.interrupted_until {
                    if now < until {
                        self.phase = PlayerPhase::Interrupted;
                        return true;
                    }
                    self.interrupted_until = None;
                }
                self.phase = PlayerPhase::Ready;
                return true;
            }

            let iframes = config.dodge.iframes;
            self.phase = if elapsed < iframes.from {
                PlayerPhase::Windup
            } else if iframes.contains(elapsed) {
                PlayerPhase::Active
            } else {
                PlayerPhase::Recovery
            };
            return false;
        }

        if let Some(until) = self.interrupted_until {
            if now < until {
                self.phase = PlayerPhase::Interrupted;
                return false;
            }
            self.interrupted_until = None;
        }
        self.phase = PlayerPhase::Ready;
        false
    }

    /// Can the current action accept the dodge input at all? The donor only listens after both
    /// its generic and dodge-specific input thresholds.
    pub(crate) fn listening_for_dodge(&self, now: Ticks, config: &CombatConfig) -> bool {
        match self.phase {
            PlayerPhase::Ready => true,
            PlayerPhase::Windup | PlayerPhase::Active | PlayerPhase::Recovery => self
                .dodge_elapsed(now)
                .is_some_and(|elapsed| elapsed >= config.dodge.dodge_input_threshold()),
            PlayerPhase::Interrupted | PlayerPhase::Dead => false,
        }
    }

    /// Is a buffered dodge allowed to replace the current action now?
    pub(crate) fn can_start_dodge(&self, now: Ticks, config: &CombatConfig) -> bool {
        if self.hp == 0 {
            return false;
        }
        match self.phase {
            PlayerPhase::Ready => true,
            PlayerPhase::Windup | PlayerPhase::Active | PlayerPhase::Recovery => self
                .dodge_elapsed(now)
                .is_some_and(|elapsed| elapsed >= config.dodge.cancel_dodge_from_tick),
            PlayerPhase::Interrupted | PlayerPhase::Dead => false,
        }
    }

    pub(crate) fn start_dodge(&mut self, now: Ticks, config: &CombatConfig, revision: ActionRevision) {
        self.dodge = Some(ActiveDodge {
            revision,
            started_at: now,
        });
        self.phase = PlayerPhase::Windup;
        self.sync_phase(now, config);
    }

    pub(crate) fn invincible(&self, now: Ticks, config: &CombatConfig) -> bool {
        if self.phase != PlayerPhase::Active {
            return false;
        }
        self.dodge.is_some_and(|dodge| {
            dodge.revision == config.dodge.revision
                && config
                    .dodge
                    .iframes
                    .contains(now.elapsed_since(dodge.started_at))
        })
    }

    pub(crate) fn receive_hit(
        &mut self,
        now: Ticks,
        config: &CombatConfig,
        damage: u32,
    ) -> HitOutcome {
        if self.hp == 0 {
            return HitOutcome::IgnoredDead;
        }
        if self.invincible(now, config) {
            return HitOutcome::Dodged;
        }

        self.hp = self.hp.saturating_sub(damage);
        if self.hp == 0 {
            self.dodge = None;
            self.interrupted_until = None;
            self.phase = PlayerPhase::Dead;
            return HitOutcome::Killed;
        }

        self.dodge = None;
        self.interrupted_until = Some(now.saturating_add(config.interrupt_recovery_ticks));
        self.phase = PlayerPhase::Interrupted;
        HitOutcome::Hit
    }

    pub(crate) fn reset(&mut self) {
        self.hp = self.max_hp;
        self.stamina.reset();
        self.phase = PlayerPhase::Ready;
        self.dodge = None;
        self.interrupted_until = None;
    }

    pub(crate) fn restore(
        now: Ticks,
        config: &CombatConfig,
        hp: u32,
        current_stamina: u32,
        stamina_max: u32,
        stamina_regen_resume_at: Option<Ticks>,
        dodge_started_at: Option<Ticks>,
        interrupted_until: Option<Ticks>,
        expected_phase: PlayerPhase,
    ) -> Option<Self> {
        if hp > config.player_max_hp || stamina_max != config.stamina.max {
            return None;
        }
        if hp == 0 && (dodge_started_at.is_some() || interrupted_until.is_some()) {
            return None;
        }
        if dodge_started_at.is_some() && interrupted_until.is_some() {
            return None;
        }
        if expected_phase != PlayerPhase::Interrupted && interrupted_until.is_some() {
            return None;
        }
        let stamina = Stamina::restore(current_stamina, stamina_max, stamina_regen_resume_at)?;
        let dodge = match dodge_started_at {
            Some(started_at) if started_at <= now => Some(ActiveDodge {
                revision: config.dodge.revision,
                started_at,
            }),
            Some(_) => return None,
            None => None,
        };
        let mut player = Self {
            hp,
            max_hp: config.player_max_hp,
            stamina,
            phase: PlayerPhase::Ready,
            dodge,
            interrupted_until,
        };
        player.sync_phase(now, config);
        if player.phase != expected_phase {
            return None;
        }
        if dodge_started_at.is_some() && player.dodge.is_none() {
            return None;
        }
        if player.hp == 0 && (player.dodge.is_some() || player.interrupted_until.is_some()) {
            return None;
        }
        Some(player)
    }
}
