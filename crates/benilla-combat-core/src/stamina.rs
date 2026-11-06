//! Integer stamina with spend clamping and an explicit post-spend regeneration delay.

use crate::time::Ticks;

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub(crate) struct Stamina {
    current: u32,
    max: u32,
    regen_resume_at: Option<Ticks>,
}

impl Stamina {
    pub(crate) fn new(max: u32) -> Self {
        Self {
            current: max,
            max,
            regen_resume_at: None,
        }
    }

    pub(crate) fn restore(
        current: u32,
        max: u32,
        regen_resume_at: Option<Ticks>,
    ) -> Option<Self> {
        // A spend is the only way to leave full, and it always schedules the deadline. Keeping the
        // pair canonical prevents a snapshot from inventing a partial bar that never regenerates.
        let deadline_matches = (current < max) == regen_resume_at.is_some();
        (current <= max && deadline_matches).then_some(Self {
            current,
            max,
            regen_resume_at,
        })
    }

    pub(crate) fn current(self) -> u32 {
        self.current
    }

    pub(crate) fn max(self) -> u32 {
        self.max
    }

    pub(crate) fn regen_resume_at(self) -> Option<Ticks> {
        self.regen_resume_at
    }

    /// Spend through the action's start. Eligibility is `current > 0`, matching the primary
    /// donor; the subtraction clamps instead of going negative.
    pub(crate) fn spend(&mut self, cost: u32, now: Ticks, delay_ticks: u64) -> bool {
        if self.current == 0 {
            return false;
        }
        self.current = self.current.saturating_sub(cost);
        self.regen_resume_at = Some(now.saturating_add(delay_ticks));
        true
    }

    pub(crate) fn regenerate(&mut self, now: Ticks, per_tick: u32) {
        let Some(resume_at) = self.regen_resume_at else {
            return;
        };
        if now < resume_at {
            return;
        }
        self.current = self.current.saturating_add(per_tick).min(self.max);
        if self.current == self.max {
            self.regen_resume_at = None;
        }
    }

    pub(crate) fn reset(&mut self) {
        self.current = self.max;
        self.regen_resume_at = None;
    }
}
