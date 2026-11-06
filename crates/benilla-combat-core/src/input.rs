//! Dodge gesture, bounded buffering, and typed input results.
//!
//! The donor reads a press, a held state, and a release. Project B keeps the release-time dodge
//! and hold-to-sprint distinction, but makes the buffer's one-slot newest-wins rule and its TTL
//! explicit. An accepted input still only *requests* the server core; it does not grant i-frames.

use crate::error::{CombatError, CombatResult};
use crate::identity::{ActionRevision, PlayerId, Sequence, SessionEpoch};
use crate::time::Ticks;

#[derive(Clone, Copy, Default, PartialEq, Eq, Debug)]
pub struct DodgeButton {
    pub pressed: bool,
    pub held: bool,
    pub released: bool,
}

impl DodgeButton {
    /// The adapter constructs one frame per observed input change. A press is held for that tick;
    /// a release is not held. Press and release cannot share a tick.
    pub fn validate(self) -> CombatResult<()> {
        if self.pressed && !self.held {
            return Err(CombatError::InvalidButtonState);
        }
        if self.pressed && self.released {
            return Err(CombatError::InvalidButtonState);
        }
        if self.released && self.held {
            return Err(CombatError::InvalidButtonState);
        }
        Ok(())
    }
}

/// A server-received input request. It is not proof that the server accepted the action.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct InputFrame {
    pub session: SessionEpoch,
    pub player: PlayerId,
    pub sequence: Sequence,
    pub tick: Ticks,
    pub action_revision: ActionRevision,
    pub dodge: DodgeButton,
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum GestureOutcome {
    Pressed,
    Held,
    /// Short press released: a dodge request was made.
    ShortRelease { held_ticks: u64 },
    /// Long hold released: sprint input, deliberately not a dodge.
    LongRelease { held_ticks: u64 },
    /// Release with no matching press (lost press or idempotent observation).
    IgnoredRelease,
    /// This input is deferred until its future tick.
    Deferred,
    Idle,
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum DodgeRejection {
    Dead,
    Interrupted,
    InsufficientStamina,
    InputWindowClosed,
    NotReady,
}

/// Result inside this offline reducer. `Started` is not a server grant of i-frames and must not be
/// sent or presented as a final combat outcome.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum DodgeResult {
    None,
    Started {
        revision: ActionRevision,
        stamina_remaining: u32,
    },
    Buffered {
        expires_at: Ticks,
    },
    Rejected(DodgeRejection),
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct InputOutcome {
    pub sequence: Sequence,
    pub tick: Ticks,
    pub gesture: GestureOutcome,
    pub dodge: DodgeResult,
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct BufferedDodge {
    pub revision: ActionRevision,
    pub requested_at: Ticks,
    /// Half-open expiry: the request is gone at `now >= expires_at`.
    pub expires_at: Ticks,
}

#[derive(Clone, Copy, Default, Debug)]
pub(crate) struct GestureState {
    armed: bool,
    press_tick: Option<Ticks>,
}

impl GestureState {
    pub(crate) fn reset(&mut self) {
        self.armed = false;
        self.press_tick = None;
    }

    pub(crate) fn update(
        &mut self,
        tick: Ticks,
        button: DodgeButton,
        sprint_hold_ticks: u64,
    ) -> GestureOutcome {
        if button.pressed {
            self.armed = true;
            self.press_tick = Some(tick);
            return GestureOutcome::Pressed;
        }

        if button.released {
            let held_ticks = self
                .press_tick
                .map_or(0, |pressed_at| tick.elapsed_since(pressed_at));
            let was_armed = self.armed;
            self.armed = false;
            self.press_tick = None;
            if !was_armed {
                return GestureOutcome::IgnoredRelease;
            }
            return if held_ticks < sprint_hold_ticks {
                GestureOutcome::ShortRelease { held_ticks }
            } else {
                GestureOutcome::LongRelease { held_ticks }
            };
        }

        if button.held {
            GestureOutcome::Held
        } else {
            GestureOutcome::Idle
        }
    }
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub(crate) struct InputBuffer {
    slot: Option<BufferedDodge>,
}

impl InputBuffer {
    pub(crate) fn new() -> Self {
        Self { slot: None }
    }

    pub(crate) fn current(self) -> Option<BufferedDodge> {
        self.slot
    }

    /// One slot, newest wins. A new request replaces the previous one regardless of age.
    pub(crate) fn push(
        &mut self,
        revision: ActionRevision,
        requested_at: Ticks,
        ttl_ticks: u64,
    ) -> BufferedDodge {
        let buffered = BufferedDodge {
            revision,
            requested_at,
            expires_at: requested_at.saturating_add(ttl_ticks),
        };
        self.slot = Some(buffered);
        buffered
    }

    pub(crate) fn expire(&mut self, now: Ticks) -> Option<BufferedDodge> {
        match self.slot {
            Some(buffered) if now >= buffered.expires_at => {
                self.slot = None;
                Some(buffered)
            }
            _ => None,
        }
    }

    pub(crate) fn clear(&mut self) {
        self.slot = None;
    }
}
