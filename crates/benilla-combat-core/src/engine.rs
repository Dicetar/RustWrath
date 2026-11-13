//! The general action engine: any actor, any action table, integer ticks.
//!
//! This is the reusable rules surface the RFC's step 1 asks for: the donor's ordered-input
//! semantics (release versus hold), one-slot newest-wins buffering with a TTL, half-open action
//! windows, stamina spend/clamp/regen-delay, interruption and death — generalized from the
//! single-dodge fixture to every [`ActionSpec`] in a validated [`ActionTable`].
//!
//! **Authority:** the engine owns no authority of its own. Callers in the authoritative server
//! process run it for real; the client runs the same code as a prediction and never treats its
//! output as an outcome. The event stream names exactly what happened and when, so a server
//! receipt can be checked against it.
//!
//! **Time:** caller-supplied monotonic [`Ticks`], no wall clock, no floats. A request is applied
//! at the caller's `now`; the engine itself never picks a tick.

use crate::action::{ActionId, ActionSpec, ActionTable};
use crate::data::StaminaConfig;
use crate::engine::private::*;
use crate::error::{CombatError, CombatResult};
use crate::time::{TickWindow, Ticks};

/// Why an action instance ended.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum ActionEnd {
    /// It ran to its total.
    Completed,
    /// A buffered action replaced it in its cancel window.
    Replaced { by: ActionId },
    /// A hit (or forced control) cut it short.
    Interrupted,
    /// The actor died.
    Died,
}

/// Why a request did not become a running action.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum ActionRefusal {
    /// The table holds no such action (this revision).
    UnknownAction,
    /// The donor's eligibility rule: a zero bar refuses; a partial bar would have clamped.
    InsufficientStamina,
    /// Nothing plays for a dead actor.
    Dead,
}

/// Everything the engine reports, in order.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum EngineEvent {
    Started {
        id: ActionId,
        tick: Ticks,
        /// Whether the start consumed a buffered request rather than a fresh one.
        from_buffer: bool,
    },
    Ended {
        id: ActionId,
        tick: Ticks,
        reason: ActionEnd,
    },
    /// The action's active window opened: the contact window, or a dodge's i-frames.
    ActiveOpened {
        id: ActionId,
        tick: Ticks,
        window: TickWindow,
    },
    /// The active window closed.
    ActiveClosed { id: ActionId, tick: Ticks },
    /// A request was held in the one slot.
    Buffered { id: ActionId, tick: Ticks },
    /// A held request outlived the buffer TTL and was dropped.
    BufferExpired { id: ActionId, tick: Ticks },
    Refused {
        id: ActionId,
        tick: Ticks,
        reason: ActionRefusal,
    },
}

/// The engine's actor state.
#[derive(Clone, Debug)]
pub struct ActionEngine {
    table: ActionTable,
    stamina: Stamina,
    /// Ticks a buffered request stays alive.
    buffer_ttl_ticks: u64,
    active: Option<ActiveAction>,
    buffered: Option<BufferedRequest>,
    dead: bool,
    last: Ticks,
}

mod private {
    use super::*;

    #[derive(Clone, Copy, Debug)]
    pub(super) struct ActiveAction {
        pub(super) spec: ActionSpec,
        pub(super) started: Ticks,
        pub(super) active_opened: bool,
        pub(super) active_closed: bool,
    }

    impl ActiveAction {
        pub(super) fn elapsed(&self, now: Ticks) -> u64 {
            now.elapsed_since(self.started)
        }

        /// The window whose boundaries define the action's active phase: a dodge's i-frames,
        /// everything else's contact window.
        pub(super) fn active_window(&self) -> Option<TickWindow> {
            match self.spec.kind {
                crate::action::ActionKind::Dodge => self.spec.iframes,
                _ => self.spec.contact,
            }
        }
    }

    #[derive(Clone, Copy, Debug)]
    pub(super) struct BufferedRequest {
        pub(super) id: ActionId,
        pub(super) at: Ticks,
    }

    #[derive(Clone, Debug)]
    pub(super) struct Stamina {
        pub(super) current: u32,
        pub(super) config: StaminaConfig,
        last_spend: Option<Ticks>,
        last_regen: Ticks,
    }

    impl Stamina {
        pub(super) fn new(config: &StaminaConfig) -> CombatResult<Self> {
            if config.max == 0 || config.regen_per_tick == 0 || config.regen_delay_ticks == 0 {
                return Err(CombatError::InvalidConfig(
                    "engine stamina config must be non-zero",
                ));
            }
            Ok(Self {
                current: config.max,
                config: *config,
                last_spend: None,
                last_regen: Ticks::ZERO,
            })
        }

        /// The donor's eligibility rule: a non-zero bar, not the full cost.
        pub(super) fn can_spend(&self) -> bool {
            self.current > 0
        }

        /// Spend with the donor's clamp at zero.
        pub(super) fn spend(&mut self, cost: u32, now: Ticks) {
            self.current = self.current.saturating_sub(cost);
            self.last_spend = Some(now);
            self.last_regen = now;
        }

        /// Regenerate whole ticks after the post-spend delay; the delay period is never banked.
        pub(super) fn regen(&mut self, now: Ticks) {
            let Some(last_spend) = self.last_spend else {
                return;
            };
            let from = last_spend
                .saturating_add(self.config.regen_delay_ticks)
                .max(self.last_regen);
            if now <= from {
                return;
            }
            let ticks = now.elapsed_since(from);
            let restored = (ticks as u32).saturating_mul(self.config.regen_per_tick);
            self.current = self.current.saturating_add(restored).min(self.config.max);
            self.last_regen = from.saturating_add(ticks);
        }
    }
}

impl ActionEngine {
    /// Build an engine over a validated table. `table.revision` pins every event this engine
    /// emits; `stamina` is the caller's policy (server-owned in production).
    pub fn new(table: ActionTable, stamina: StaminaConfig, buffer_ttl_ticks: u64) -> CombatResult<Self> {
        table.validate()?;
        if buffer_ttl_ticks == 0 {
            return Err(CombatError::InvalidConfig(
                "engine buffer_ttl_ticks must be non-zero",
            ));
        }
        Ok(Self {
            stamina: Stamina::new(&stamina)?,
            table,
            buffer_ttl_ticks,
            active: None,
            buffered: None,
            dead: false,
            last: Ticks::ZERO,
        })
    }

    /// The pinned table.
    pub fn table(&self) -> &ActionTable {
        &self.table
    }

    /// The running action instance, if any.
    pub fn active(&self) -> Option<ActionId> {
        self.active.map(|active| active.spec.id)
    }

    /// Elapsed ticks of the running action at `now`, if any.
    pub fn active_elapsed(&self, now: Ticks) -> Option<u64> {
        self.active.map(|active| active.elapsed(now))
    }

    /// The held request, if any.
    pub fn buffered(&self) -> Option<ActionId> {
        self.buffered.map(|buffered| buffered.id)
    }

    /// The current predicted/authoritative stamina.
    pub fn stamina(&self) -> u32 {
        self.stamina.current
    }

    /// Whether the actor is dead.
    pub fn is_dead(&self) -> bool {
        self.dead
    }

    /// Advance the engine to `now`: end completed actions, open and close active windows, start
    /// a held request in its cancel window when stamina allows, expire a stale buffer, and
    /// regenerate stamina.
    pub fn advance_to(&mut self, now: Ticks) -> CombatResult<Vec<EngineEvent>> {
        if now < self.last {
            return Err(CombatError::TimeWentBackwards {
                now: self.last,
                requested: now,
            });
        }
        self.last = now;
        let mut events = Vec::new();
        if self.dead {
            return Ok(events);
        }
        self.stamina.regen(now);

        if let Some(active) = self.active {
            let elapsed = active.elapsed(now);
            if let Some(window) = active.active_window() {
                if !active.active_opened && elapsed >= window.from {
                    if let Some(slot) = self.active.as_mut() {
                        slot.active_opened = true;
                    }
                    events.push(EngineEvent::ActiveOpened {
                        id: active.spec.id,
                        tick: now,
                        window,
                    });
                }
                if !active.active_closed && elapsed >= window.to {
                    if let Some(slot) = self.active.as_mut() {
                        slot.active_closed = true;
                    }
                    events.push(EngineEvent::ActiveClosed {
                        id: active.spec.id,
                        tick: now,
                    });
                }
            }
            if elapsed >= active.spec.total_ticks {
                self.active = None;
                events.push(EngineEvent::Ended {
                    id: active.spec.id,
                    tick: now,
                    reason: ActionEnd::Completed,
                });
            }
        }

        // A held request outlives its welcome only so long: the TTL runs from the press,
        // whether or not the running action has a cancel window.
        if let Some(buffered) = self.buffered {
            if now.elapsed_since(buffered.at) >= self.buffer_ttl_ticks {
                self.buffered = None;
                events.push(EngineEvent::BufferExpired {
                    id: buffered.id,
                    tick: now,
                });
            }
        }

        // A held request starts when the slot frees or its cancel window opens — but only with
        // a non-zero bar; otherwise it keeps waiting until stamina returns or the TTL expires.
        let cancel_ready = self.active.is_none_or(|active| {
            active
                .spec
                .cancel_from_ticks
                .is_some_and(|from| active.elapsed(now) >= from)
        });
        if cancel_ready {
            if let Some(buffered) = self.buffered {
                if self.stamina.can_spend() {
                    self.buffered = None;
                    if let Some(active) = self.active.take() {
                        events.push(EngineEvent::Ended {
                            id: active.spec.id,
                            tick: now,
                            reason: ActionEnd::Replaced { by: buffered.id },
                        });
                    }
                    self.start(buffered.id, now, true, &mut events);
                }
            }
        }
        Ok(events)
    }

    /// Submit one request at `now`. A request while an action runs (or while a request is still
    /// held) occupies the one newest-wins slot; with nothing running it starts immediately, or
    /// is refused at a zero bar.
    pub fn submit(&mut self, id: ActionId, now: Ticks) -> CombatResult<Vec<EngineEvent>> {
        let mut events = self.advance_to(now)?;
        if self.dead {
            events.push(EngineEvent::Refused {
                id,
                tick: now,
                reason: ActionRefusal::Dead,
            });
            return Ok(events);
        }
        if self.table.get(id).is_none() {
            events.push(EngineEvent::Refused {
                id,
                tick: now,
                reason: ActionRefusal::UnknownAction,
            });
            return Ok(events);
        }
        if self.active.is_some() || self.buffered.is_some() {
            self.buffered = Some(BufferedRequest { id, at: now });
            events.push(EngineEvent::Buffered { id, tick: now });
            return Ok(events);
        }
        if !self.stamina.can_spend() {
            events.push(EngineEvent::Refused {
                id,
                tick: now,
                reason: ActionRefusal::InsufficientStamina,
            });
            return Ok(events);
        }
        self.start(id, now, false, &mut events);
        Ok(events)
    }

    /// A hit or forced control ends the running action and drops the held request. A non-lethal
    /// hit does not kill; [`Self::die`] does.
    pub fn interrupt(&mut self, now: Ticks) -> CombatResult<Vec<EngineEvent>> {
        let mut events = self.advance_to(now)?;
        self.buffered = None;
        if let Some(active) = self.active.take() {
            events.push(EngineEvent::Ended {
                id: active.spec.id,
                tick: now,
                reason: ActionEnd::Interrupted,
            });
        }
        Ok(events)
    }

    /// The actor died: the running action ends, the buffer clears, and nothing submits again.
    pub fn die(&mut self, now: Ticks) -> CombatResult<Vec<EngineEvent>> {
        let mut events = self.advance_to(now)?;
        self.dead = true;
        self.buffered = None;
        if let Some(active) = self.active.take() {
            events.push(EngineEvent::Ended {
                id: active.spec.id,
                tick: now,
                reason: ActionEnd::Died,
            });
        }
        Ok(events)
    }

    /// Start one action: spend its cost and open the instance.
    fn start(&mut self, id: ActionId, now: Ticks, from_buffer: bool, events: &mut Vec<EngineEvent>) {
        let Some(spec) = self.table.get(id).copied() else {
            return;
        };
        self.stamina.spend(spec.stamina_cost, now);
        self.active = Some(ActiveAction {
            spec,
            started: now,
            active_opened: false,
            active_closed: false,
        });
        events.push(EngineEvent::Started {
            id,
            tick: now,
            from_buffer,
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::action::ActionKind;
    use crate::identity::ActionRevision;
    use crate::time::TickRate;

    fn rate() -> TickRate {
        TickRate::new(100).expect("100 Hz")
    }

    fn melee(id: u32, total: u64) -> ActionSpec {
        ActionSpec {
            id: ActionId(id),
            kind: ActionKind::MeleeLight,
            total_ticks: total,
            input_from_ticks: total,
            cancel_from_ticks: Some(40),
            iframes: None,
            contact: Some(TickWindow { from: 20, to: 30 }),
            stamina_cost: 20,
            interruptible_until: Some(20),
            followup: None,
        }
    }

    fn stamina() -> StaminaConfig {
        StaminaConfig {
            max: 100,
            regen_per_tick: 1,
            regen_delay_ticks: 100,
        }
    }

    fn engine_with(actions: Vec<ActionSpec>) -> ActionEngine {
        let mut table = ActionTable::new(ActionRevision(1), rate());
        for spec in actions {
            table.insert(spec).expect("insert");
        }
        ActionEngine::new(table, stamina(), 50).expect("engine")
    }

    fn t(value: u64) -> Ticks {
        Ticks::new(value)
    }

    /// One light swing: starts, opens contact at the window's start, closes it at its end, and
    /// completes at the total — each event once, at the tick the boundary is crossed.
    #[test]
    fn a_swing_reports_its_window_events_once() {
        let mut engine = engine_with(vec![melee(1, 50)]);
        let started = engine.submit(ActionId(1), t(0)).expect("submit");
        assert_eq!(
            started,
            vec![EngineEvent::Started {
                id: ActionId(1),
                tick: t(0),
                from_buffer: false
            }]
        );
        assert_eq!(engine.stamina(), 80, "the swing spent its cost");
        assert_eq!(engine.active(), Some(ActionId(1)));

        let contact = engine.advance_to(t(20)).expect("advance");
        assert_eq!(
            contact,
            vec![EngineEvent::ActiveOpened {
                id: ActionId(1),
                tick: t(20),
                window: TickWindow { from: 20, to: 30 }
            }]
        );
        let closed = engine.advance_to(t(30)).expect("advance");
        assert_eq!(
            closed,
            vec![EngineEvent::ActiveClosed {
                id: ActionId(1),
                tick: t(30)
            }]
        );
        let ended = engine.advance_to(t(50)).expect("advance");
        assert_eq!(
            ended,
            vec![EngineEvent::Ended {
                id: ActionId(1),
                tick: t(50),
                reason: ActionEnd::Completed
            }]
        );
        assert_eq!(engine.active(), None);
    }

    /// A second press while the first swing runs is held; at the cancel window it replaces the
    /// running action (a combo) and the held request is consumed.
    #[test]
    fn a_buffered_press_cancels_into_the_next_swing() {
        let mut engine = engine_with(vec![melee(1, 50), melee(2, 50)]);
        engine.submit(ActionId(1), t(0)).expect("submit");
        let buffered = engine.submit(ActionId(2), t(10)).expect("submit");
        assert!(buffered.contains(&EngineEvent::Buffered {
            id: ActionId(2),
            tick: t(10)
        }));
        assert_eq!(engine.buffered(), Some(ActionId(2)));

        // Before the cancel window (tick 40 of the first action) the request is still held.
        let early = engine.advance_to(t(30)).expect("advance");
        assert!(!early
            .iter()
            .any(|e| matches!(e, EngineEvent::Started { .. })));
        assert_eq!(engine.buffered(), Some(ActionId(2)));

        let cancel = engine.advance_to(t(40)).expect("advance");
        assert!(cancel.contains(&EngineEvent::Ended {
            id: ActionId(1),
            tick: t(40),
            reason: ActionEnd::Replaced { by: ActionId(2) }
        }));
        assert!(cancel.contains(&EngineEvent::Started {
            id: ActionId(2),
            tick: t(40),
            from_buffer: true
        }));
        assert_eq!(engine.active(), Some(ActionId(2)));
        assert_eq!(engine.buffered(), None);
        assert_eq!(engine.stamina(), 60, "both swings spent");
    }

    /// Newest wins: a third press replaces the held second before its cancel window opens.
    #[test]
    fn the_buffer_keeps_only_the_newest_request() {
        let mut engine = engine_with(vec![melee(1, 50), melee(2, 50), melee(3, 50)]);
        engine.submit(ActionId(1), t(0)).expect("submit");
        engine.submit(ActionId(2), t(5)).expect("submit");
        engine.submit(ActionId(3), t(10)).expect("submit");
        assert_eq!(engine.buffered(), Some(ActionId(3)));
    }

    /// A held request whose action the running one never cancels into expires at the TTL.
    #[test]
    fn a_stale_buffer_expires() {
        let mut engine = engine_with(vec![ActionSpec {
            cancel_from_ticks: None,
            ..melee(1, 100)
        }]);
        engine.submit(ActionId(1), t(0)).expect("submit");
        engine.submit(ActionId(1), t(1)).expect("submit");
        let expired = engine.advance_to(t(51)).expect("advance");
        assert!(expired.contains(&EngineEvent::BufferExpired {
            id: ActionId(1),
            tick: t(51)
        }));
        assert_eq!(engine.buffered(), None);
    }

    /// The donor's stamina nuance: an immediate request at a zero bar is refused, but a held
    /// request survives a zero bar and starts once regeneration returns — or expires at the TTL.
    #[test]
    fn a_zero_bar_refuses_immediate_requests_but_waits_for_held_ones() {
        // This engine's TTL outlives the stamina delay (100 ticks), so the held request can be
        // observed waiting and then starting.
        let mut table = ActionTable::new(ActionRevision(1), rate());
        table
            .insert(ActionSpec {
                stamina_cost: 100,
                ..melee(1, 50)
            })
            .expect("insert");
        table.insert(melee(2, 50)).expect("insert");
        let mut engine = ActionEngine::new(table, stamina(), 200).expect("engine");

        engine.submit(ActionId(1), t(0)).expect("submit");
        assert_eq!(engine.stamina(), 0, "the first swing empties the bar");
        engine.submit(ActionId(2), t(10)).expect("submit");
        assert_eq!(engine.buffered(), Some(ActionId(2)));

        // The cancel window opens at tick 40, but the bar is empty: the request keeps waiting.
        let waiting = engine.advance_to(t(40)).expect("advance");
        assert!(!waiting
            .iter()
            .any(|e| matches!(e, EngineEvent::Started { .. })));
        assert_eq!(engine.buffered(), Some(ActionId(2)));

        // The action ends at tick 50; still zero, so it waits.
        engine.advance_to(t(50)).expect("advance");
        assert_eq!(engine.active(), None);
        assert_eq!(engine.buffered(), Some(ActionId(2)));

        // Regeneration resumes at tick 100 (the spend at 0 plus the 100-tick delay): the first
        // regenerated tick lands at 101, and the held request starts at the next advance.
        engine.advance_to(t(100)).expect("advance");
        assert_eq!(engine.stamina(), 0, "the delay period is never banked");
        let started = engine.advance_to(t(101)).expect("advance");
        assert!(started.contains(&EngineEvent::Started {
            id: ActionId(2),
            tick: t(101),
            from_buffer: true
        }));
        assert_eq!(engine.active(), Some(ActionId(2)));
        assert_eq!(
            engine.stamina(),
            0,
            "the held action spent its 20 cost from the single regenerated point and clamped"
        );

        // An immediate request at a zero bar is refused outright, never held.
        let mut engine = engine_with(vec![ActionSpec {
            stamina_cost: 100,
            ..melee(1, 50)
        }]);
        engine.submit(ActionId(1), t(0)).expect("submit");
        engine.advance_to(t(50)).expect("advance");
        let refused = engine.submit(ActionId(1), t(50)).expect("submit");
        assert!(refused.contains(&EngineEvent::Refused {
            id: ActionId(1),
            tick: t(50),
            reason: ActionRefusal::InsufficientStamina
        }));
        assert_eq!(engine.buffered(), None);
    }

    /// A hit interrupts the running action and drops the buffer; death ends everything and
    /// refuses further submissions.
    #[test]
    fn interruption_and_death_end_actions_once() {
        let mut engine = engine_with(vec![melee(1, 50), melee(2, 50)]);
        engine.submit(ActionId(1), t(0)).expect("submit");
        engine.submit(ActionId(2), t(5)).expect("submit");
        let hit = engine.interrupt(t(10)).expect("interrupt");
        assert!(hit.contains(&EngineEvent::Ended {
            id: ActionId(1),
            tick: t(10),
            reason: ActionEnd::Interrupted
        }));
        assert_eq!(engine.buffered(), None);
        assert_eq!(engine.active(), None);

        engine.submit(ActionId(1), t(20)).expect("submit");
        let died = engine.die(t(25)).expect("die");
        assert!(died.contains(&EngineEvent::Ended {
            id: ActionId(1),
            tick: t(25),
            reason: ActionEnd::Died
        }));
        assert!(engine.is_dead());
        let refused = engine.submit(ActionId(1), t(30)).expect("submit");
        assert!(refused.contains(&EngineEvent::Refused {
            id: ActionId(1),
            tick: t(30),
            reason: ActionRefusal::Dead
        }));
        assert!(engine.advance_to(t(100)).expect("advance").is_empty());
    }

    /// Unknown actions are refused, and time cannot move backwards.
    #[test]
    fn unknown_actions_and_time_regression_are_typed_errors() {
        let mut engine = engine_with(vec![melee(1, 50)]);
        let refused = engine.submit(ActionId(9), t(0)).expect("submit");
        assert_eq!(
            refused,
            vec![EngineEvent::Refused {
                id: ActionId(9),
                tick: t(0),
                reason: ActionRefusal::UnknownAction
            }]
        );
        engine.advance_to(t(10)).expect("advance");
        assert!(matches!(
            engine.advance_to(t(9)),
            Err(CombatError::TimeWentBackwards { .. })
        ));
    }
}
