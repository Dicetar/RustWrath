//! One telegraphed enemy: wind-up, a single resolved active strike, recovery, death.

use crate::data::CombatConfig;
use crate::time::Ticks;

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum EnemyPhase {
    Ready,
    Windup,
    Active,
    Recovery,
    Dead,
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct EnemyView {
    pub hp: u32,
    pub max_hp: u32,
    pub phase: EnemyPhase,
    pub telegraph_started_at: Option<Ticks>,
    pub active_resolved: bool,
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub(crate) struct Telegraph {
    pub(crate) started_at: Ticks,
    pub(crate) resolved: bool,
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub(crate) struct Enemy {
    pub(crate) hp: u32,
    pub(crate) max_hp: u32,
    pub(crate) phase: EnemyPhase,
    pub(crate) telegraph: Option<Telegraph>,
}

impl Enemy {
    pub(crate) fn new(config: &CombatConfig) -> Self {
        Self {
            hp: config.enemy.max_hp,
            max_hp: config.enemy.max_hp,
            phase: EnemyPhase::Ready,
            telegraph: None,
        }
    }

    pub(crate) fn view(&self) -> EnemyView {
        EnemyView {
            hp: self.hp,
            max_hp: self.max_hp,
            phase: self.phase,
            telegraph_started_at: self.telegraph.map(|telegraph| telegraph.started_at),
            active_resolved: self.telegraph.map_or(false, |telegraph| telegraph.resolved),
        }
    }

    pub(crate) fn is_dead(&self) -> bool {
        self.hp == 0
    }

    pub(crate) fn telegraph_started_at(&self) -> Option<Ticks> {
        self.telegraph.map(|telegraph| telegraph.started_at)
    }

    pub(crate) fn active_resolved(&self) -> bool {
        self.telegraph.map_or(false, |telegraph| telegraph.resolved)
    }

    pub(crate) fn begin_telegraph(&mut self, now: Ticks) -> bool {
        if self.hp == 0 || self.telegraph.is_some() {
            return false;
        }
        self.telegraph = Some(Telegraph {
            started_at: now,
            resolved: false,
        });
        self.phase = EnemyPhase::Windup;
        true
    }

    pub(crate) fn sync_phase(&mut self, now: Ticks, config: &CombatConfig) {
        if self.hp == 0 {
            self.phase = EnemyPhase::Dead;
            self.telegraph = None;
            return;
        }

        let Some(telegraph) = self.telegraph else {
            self.phase = EnemyPhase::Ready;
            return;
        };

        let elapsed = now.elapsed_since(telegraph.started_at);
        let windup = config.enemy.windup_ticks;
        let active_end = windup.saturating_add(config.enemy.active_ticks);
        let total = config
            .enemy
            .total_ticks()
            .unwrap_or(active_end.saturating_add(config.enemy.recovery_ticks));

        if elapsed >= total {
            self.telegraph = None;
            self.phase = EnemyPhase::Ready;
        } else if elapsed < windup {
            self.phase = EnemyPhase::Windup;
        } else if elapsed < active_end {
            self.phase = EnemyPhase::Active;
        } else {
            self.phase = EnemyPhase::Recovery;
        }
    }

    /// True exactly once per swing, on the first tick in the active window.
    pub(crate) fn take_active_resolution(&mut self) -> bool {
        let Some(telegraph) = &mut self.telegraph else {
            return false;
        };
        if self.phase != EnemyPhase::Active || telegraph.resolved {
            return false;
        }
        telegraph.resolved = true;
        true
    }

    pub(crate) fn reset(&mut self) {
        self.hp = self.max_hp;
        self.phase = EnemyPhase::Ready;
        self.telegraph = None;
    }

    pub(crate) fn restore(
        now: Ticks,
        config: &CombatConfig,
        hp: u32,
        max_hp: u32,
        telegraph_started_at: Option<Ticks>,
        active_resolved: bool,
        expected_phase: EnemyPhase,
    ) -> Option<Self> {
        if hp > config.enemy.max_hp || max_hp != config.enemy.max_hp {
            return None;
        }
        if hp == 0 && telegraph_started_at.is_some() {
            return None;
        }
        if telegraph_started_at.is_none() && active_resolved {
            return None;
        }
        let telegraph = match telegraph_started_at {
            Some(started_at) if started_at <= now => Some(Telegraph {
                started_at,
                resolved: active_resolved,
            }),
            Some(_) => return None,
            None => None,
        };
        let mut enemy = Self {
            hp,
            max_hp,
            phase: EnemyPhase::Ready,
            telegraph,
        };
        enemy.sync_phase(now, config);
        if enemy.phase != expected_phase {
            return None;
        }
        if telegraph_started_at.is_some() && enemy.telegraph.is_none() {
            return None;
        }
        if expected_phase == EnemyPhase::Windup && active_resolved {
            return None;
        }
        if expected_phase == EnemyPhase::Active && !active_resolved {
            return None;
        }
        if expected_phase == EnemyPhase::Recovery && !active_resolved {
            return None;
        }
        Some(enemy)
    }
}
