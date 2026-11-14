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
//!
//! **Step invariance:** [`ActionEngine::advance_to`] walks every tick in order instead of
//! evaluating the target instant once. A frame jump therefore produces exactly the events — and
//! the same final state — that advancing one tick at a time would, with each event stamped at the
//! tick where it happened rather than at the jump's destination. A span wider than
//! [`MAX_ADVANCE_SPAN_TICKS`] is refused with [`CombatError::AdvanceTooLarge`]; callers with a
//! larger gap advance in bounded chunks.

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
    /// The running action has not reached its `input_from_ticks` offset yet, so it is not
    /// listening. The request is refused outright and never enters the held slot.
    InputWindowClosed,
}

/// Largest tick span one [`ActionEngine::advance_to`] call simulates. A wider gap is refused with
/// [`CombatError::AdvanceTooLarge`] so no call performs unbounded catch-up work; a caller with a
/// larger jump advances in bounded chunks.
pub const MAX_ADVANCE_SPAN_TICKS: u64 = 600;

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

    /// Advance the engine to `now` one tick at a time: end completed actions, open and close
    /// active windows, start a held request in its cancel window when stamina and follow-up
    /// policy allow, expire a stale buffer, and regenerate stamina.
    ///
    /// Every boundary is evaluated on the tick it occurs, so one call to `now` reports the same
    /// events (same order, same ticks) and leaves the same state as calling this once per
    /// intermediate tick. A span wider than [`MAX_ADVANCE_SPAN_TICKS`] is refused with
    /// [`CombatError::AdvanceTooLarge`] and leaves the engine untouched.
    pub fn advance_to(&mut self, now: Ticks) -> CombatResult<Vec<EngineEvent>> {
        let from = self.last;
        if now < from {
            return Err(CombatError::TimeWentBackwards {
                now: from,
                requested: now,
            });
        }
        let mut events = Vec::new();
        if self.dead {
            self.last = now;
            return Ok(events);
        }
        let span = now.elapsed_since(from);
        if span > MAX_ADVANCE_SPAN_TICKS {
            return Err(CombatError::AdvanceTooLarge {
                from,
                to: now,
                limit: MAX_ADVANCE_SPAN_TICKS,
            });
        }
        let mut tick = from;
        while tick < now {
            tick = tick
                .checked_add(1)
                .ok_or(CombatError::TickArithmeticOverflow)?;
            self.last = tick;
            self.step(tick, &mut events);
        }
        Ok(events)
    }

    /// One tick of the rules: the body `advance_to` repeats for every tick in its span.
    fn step(&mut self, now: Ticks, events: &mut Vec<EngineEvent>) {
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
        // A running action with a declared follow-up only yields to that follow-up; any other
        // held request keeps waiting (and is still subject to the TTL).
        let Some(buffered) = self.buffered else {
            return;
        };
        let replace_ready = self.active.is_none_or(|active| {
            let elapsed = active.elapsed(now);
            let cancel_ready = active
                .spec
                .cancel_from_ticks
                .is_some_and(|from| elapsed >= from);
            let followup_ready = active
                .spec
                .followup
                .is_none_or(|followup| followup == buffered.id);
            cancel_ready && followup_ready
        });
        if !replace_ready || !self.stamina.can_spend() {
            return;
        }
        self.buffered = None;
        if let Some(active) = self.active.take() {
            self.close_active_early(&active, now, events);
            events.push(EngineEvent::Ended {
                id: active.spec.id,
                tick: now,
                reason: ActionEnd::Replaced { by: buffered.id },
            });
        }
        self.start(buffered.id, now, true, events);
    }

    /// Submit one request at `now`. A request while an action runs (or while a request is still
    /// held) occupies the one newest-wins slot; with nothing running it starts immediately, or
    /// is refused at a zero bar.
    ///
    /// While an action runs, it hears input only from its `input_from_ticks` offset: an earlier
    /// request is refused with [`ActionRefusal::InputWindowClosed`] and never touches the held
    /// slot. A submit with no running action is unaffected by that window.
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
        if self
            .active
            .is_some_and(|active| active.elapsed(now) < active.spec.input_from_ticks)
        {
            events.push(EngineEvent::Refused {
                id,
                tick: now,
                reason: ActionRefusal::InputWindowClosed,
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

    /// A hit or forced control ends the running action and drops the held request.
    ///
    /// The running action yields only up to and including its `interruptible_until` offset; past
    /// it, or when the spec declares `None` (full hyperarmor), the hit changes nothing — no
    /// `Ended`, and the held request stays. With no running action the held request is dropped.
    pub fn interrupt(&mut self, now: Ticks) -> CombatResult<Vec<EngineEvent>> {
        let mut events = self.advance_to(now)?;
        let Some(active) = self.active else {
            self.buffered = None;
            return Ok(events);
        };
        let interruptible = active
            .spec
            .interruptible_until
            .is_some_and(|until| active.elapsed(now) <= until);
        if !interruptible {
            return Ok(events);
        }
        self.buffered = None;
        self.active = None;
        self.close_active_early(&active, now, &mut events);
        events.push(EngineEvent::Ended {
            id: active.spec.id,
            tick: now,
            reason: ActionEnd::Interrupted,
        });
        Ok(events)
    }

    /// The actor died: the running action ends, the buffer clears, and nothing submits again.
    /// Death ignores `interruptible_until`.
    pub fn die(&mut self, now: Ticks) -> CombatResult<Vec<EngineEvent>> {
        let mut events = self.advance_to(now)?;
        self.dead = true;
        self.buffered = None;
        if let Some(active) = self.active.take() {
            self.close_active_early(&active, now, &mut events);
            events.push(EngineEvent::Ended {
                id: active.spec.id,
                tick: now,
                reason: ActionEnd::Died,
            });
        }
        Ok(events)
    }

    /// Emit `ActiveClosed` when an action leaves early from inside its active window. Windows are
    /// half-open, so the window is active for `from <= elapsed < to`; an action already past `to`
    /// closed it during the tick walk.
    fn close_active_early(&self, active: &ActiveAction, now: Ticks, events: &mut Vec<EngineEvent>) {
        let elapsed = active.elapsed(now);
        if active
            .active_window()
            .is_some_and(|window| elapsed >= window.from && elapsed < window.to)
        {
            events.push(EngineEvent::ActiveClosed {
                id: active.spec.id,
                tick: now,
            });
        }
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

    /// A light swing that listens for input from its first tick; the input-window rule has its
    /// own test, so the shared trace helper keeps later presses admissible.
    fn melee(id: u32, total: u64) -> ActionSpec {
        ActionSpec {
            id: ActionId(id),
            kind: ActionKind::MeleeLight,
            total_ticks: total,
            input_from_ticks: 0,
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

    /// Advance one tick at a time, collecting every event: the reference a single jump must match.
    fn step_by_tick(
        engine: &mut ActionEngine,
        from: Ticks,
        to: Ticks,
        events: &mut Vec<EngineEvent>,
    ) {
        let mut tick = from;
        while tick < to {
            tick = t(tick.get() + 1);
            events.extend(engine.advance_to(tick).expect("advance"));
        }
    }

    fn event_tick(event: EngineEvent) -> Ticks {
        match event {
            EngineEvent::Started { tick, .. }
            | EngineEvent::Ended { tick, .. }
            | EngineEvent::ActiveOpened { tick, .. }
            | EngineEvent::ActiveClosed { tick, .. }
            | EngineEvent::Buffered { tick, .. }
            | EngineEvent::BufferExpired { tick, .. }
            | EngineEvent::Refused { tick, .. } => tick,
        }
    }

    /// A frame jump must equal advancing tick by tick: same events (including the tick each one
    /// happened at) and same final state. The traces cross a buffer start, both contact edges,
    /// completion and a TTL expiry inside the jumped span.
    #[test]
    fn advance_to_is_step_invariant() {
        // (actions, buffer TTL, requests at absolute ticks, final tick, starts from a buffer?)
        let traces: Vec<(Vec<ActionSpec>, u64, Vec<(ActionId, u64)>, u64, bool)> = vec![
            // A combo replaces inside the jump, and the next action's contact opens afterwards.
            (
                vec![melee(1, 50), melee(2, 50)],
                50,
                vec![(ActionId(1), 0), (ActionId(2), 10)],
                60,
                true,
            ),
            // The held request outlives its TTL inside the jump and is dropped.
            (
                vec![
                    ActionSpec {
                        cancel_from_ticks: None,
                        ..melee(1, 200)
                    },
                    melee(2, 50),
                ],
                50,
                vec![(ActionId(1), 0), (ActionId(2), 10)],
                100,
                false,
            ),
            // A zero bar defers the held start past completion; the jump crosses the completion
            // and the first regenerated tick.
            (
                vec![
                    ActionSpec {
                        stamina_cost: 100,
                        ..melee(1, 50)
                    },
                    melee(2, 50),
                ],
                200,
                vec![(ActionId(1), 0), (ActionId(2), 10)],
                120,
                true,
            ),
        ];

        for (actions, ttl, requests, final_tick, starts_from_buffer) in traces {
            let mut table = ActionTable::new(ActionRevision(1), rate());
            for spec in &actions {
                table.insert(*spec).expect("insert");
            }
            let mut jumped = ActionEngine::new(table, stamina(), ttl).expect("engine");
            let mut stepped = jumped.clone();

            let mut jump_events = Vec::new();
            let mut step_events = Vec::new();
            let mut stepped_at = t(0);
            for (id, at) in requests {
                jump_events.extend(jumped.submit(id, t(at)).expect("submit"));
                step_by_tick(&mut stepped, stepped_at, t(at), &mut step_events);
                stepped_at = t(at);
                step_events.extend(stepped.submit(id, t(at)).expect("submit"));
            }
            jump_events.extend(jumped.advance_to(t(final_tick)).expect("advance"));
            step_by_tick(&mut stepped, stepped_at, t(final_tick), &mut step_events);

            assert_eq!(
                jump_events, step_events,
                "one jump must report the same events, at the same ticks, as tick-by-tick stepping"
            );
            assert_eq!(jumped.active(), stepped.active());
            assert_eq!(jumped.buffered(), stepped.buffered());
            assert_eq!(jumped.stamina(), stepped.stamina());
            assert_eq!(jumped.is_dead(), stepped.is_dead());
            assert_eq!(
                jumped.active_elapsed(t(final_tick)),
                stepped.active_elapsed(t(final_tick))
            );

            assert!(
                jump_events
                    .iter()
                    .copied()
                    .any(|event| event_tick(event) < t(final_tick)),
                "a boundary inside the jump must be stamped where it happened, not at `now`"
            );
            assert!(
                jump_events
                    .iter()
                    .any(|event| matches!(event, EngineEvent::ActiveClosed { .. })),
                "the contact window closes inside every trace"
            );
            assert_eq!(
                jump_events
                    .iter()
                    .any(|event| matches!(event, EngineEvent::Started { from_buffer: true, .. })),
                starts_from_buffer,
                "the trace's buffer either starts or expires"
            );
        }
    }

    /// A running action hears input only from its `input_from_ticks` offset. Earlier presses are
    /// refused, never held, and do not displace a request already waiting; with no running action
    /// the window does not apply.
    #[test]
    fn input_before_the_window_is_refused_not_buffered() {
        let mut engine = engine_with(vec![
            ActionSpec {
                input_from_ticks: 20,
                ..melee(1, 50)
            },
            melee(2, 50),
        ]);
        engine.submit(ActionId(1), t(0)).expect("submit");

        let early = engine.submit(ActionId(2), t(10)).expect("submit");
        assert_eq!(
            early,
            vec![EngineEvent::Refused {
                id: ActionId(2),
                tick: t(10),
                reason: ActionRefusal::InputWindowClosed
            }]
        );
        assert_eq!(engine.buffered(), None);
        assert_eq!(engine.active(), Some(ActionId(1)));

        // The offset is inclusive: from tick 20 the action listens and the request is held.
        let open = engine.submit(ActionId(2), t(20)).expect("submit");
        assert!(open.contains(&EngineEvent::Buffered {
            id: ActionId(2),
            tick: t(20)
        }));
        assert_eq!(engine.buffered(), Some(ActionId(2)));

        // Nothing running: the input window belongs to the active spec, so this starts at once.
        let mut idle = engine_with(vec![ActionSpec {
            input_from_ticks: 20,
            ..melee(1, 50)
        }]);
        let started = idle.submit(ActionId(1), t(0)).expect("submit");
        assert_eq!(
            started,
            vec![EngineEvent::Started {
                id: ActionId(1),
                tick: t(0),
                from_buffer: false
            }]
        );
    }

    /// Interruption applies up to and including `interruptible_until`. Past it, or with `None`
    /// (full hyperarmor), a hit changes nothing and keeps the held request. Death stays
    /// unconditional.
    #[test]
    fn interruption_respects_the_commitment_offset() {
        // Contact commits the swing at 20: a hit at 20 still interrupts, at 21 it does not.
        let committed = |id: u32| ActionSpec {
            interruptible_until: Some(20),
            ..melee(id, 50)
        };
        let mut engine = engine_with(vec![committed(1)]);
        engine.submit(ActionId(1), t(0)).expect("submit");
        let inside = engine.interrupt(t(20)).expect("interrupt");
        assert_eq!(
            inside,
            vec![
                EngineEvent::ActiveOpened {
                    id: ActionId(1),
                    tick: t(20),
                    window: TickWindow { from: 20, to: 30 }
                },
                EngineEvent::ActiveClosed {
                    id: ActionId(1),
                    tick: t(20)
                },
                EngineEvent::Ended {
                    id: ActionId(1),
                    tick: t(20),
                    reason: ActionEnd::Interrupted
                }
            ],
            "a hit at the commitment offset ends the action and closes its window"
        );
        assert_eq!(engine.active(), None);
        assert_eq!(engine.buffered(), None);

        let mut engine = engine_with(vec![committed(1), melee(2, 50)]);
        engine.submit(ActionId(1), t(0)).expect("submit");
        engine.submit(ActionId(2), t(5)).expect("submit");
        let late = engine.interrupt(t(21)).expect("interrupt");
        assert!(!late.iter().any(|event| matches!(
            event,
            EngineEvent::Ended { .. } | EngineEvent::ActiveClosed { .. }
        )));
        assert_eq!(engine.active(), Some(ActionId(1)));
        assert_eq!(engine.buffered(), Some(ActionId(2)));

        let mut armored = engine_with(vec![
            ActionSpec {
                interruptible_until: None,
                ..melee(1, 50)
            },
            melee(2, 50),
        ]);
        armored.submit(ActionId(1), t(0)).expect("submit");
        armored.submit(ActionId(2), t(5)).expect("submit");
        let shrugged = armored.interrupt(t(10)).expect("interrupt");
        assert!(shrugged.is_empty());
        assert_eq!(armored.active(), Some(ActionId(1)));
        assert_eq!(armored.buffered(), Some(ActionId(2)));

        let died = armored.die(t(11)).expect("die");
        assert!(died.contains(&EngineEvent::Ended {
            id: ActionId(1),
            tick: t(11),
            reason: ActionEnd::Died
        }));
        assert!(armored.is_dead());
    }

    /// A running action with a declared follow-up only yields to that follow-up in its cancel
    /// window; any other held request keeps waiting (and still expires).
    #[test]
    fn followup_gates_cancel_window_replacement() {
        let mut engine = engine_with(vec![
            ActionSpec {
                followup: Some(ActionId(2)),
                ..melee(1, 100)
            },
            melee(2, 50),
            melee(3, 50),
        ]);
        engine.submit(ActionId(1), t(0)).expect("submit");
        engine.submit(ActionId(3), t(5)).expect("submit");

        let blocked = engine.advance_to(t(40)).expect("advance");
        assert!(!blocked.iter().any(|event| matches!(
            event,
            EngineEvent::Ended { .. } | EngineEvent::Started { .. }
        )));
        assert_eq!(engine.active(), Some(ActionId(1)));
        assert_eq!(engine.buffered(), Some(ActionId(3)));

        // The newest-wins slot takes the actual follow-up, which then replaces at the next tick.
        engine.submit(ActionId(2), t(41)).expect("submit");
        assert_eq!(engine.buffered(), Some(ActionId(2)));
        let replaced = engine.advance_to(t(42)).expect("advance");
        assert!(replaced.contains(&EngineEvent::Ended {
            id: ActionId(1),
            tick: t(42),
            reason: ActionEnd::Replaced { by: ActionId(2) }
        }));
        assert!(replaced.contains(&EngineEvent::Started {
            id: ActionId(2),
            tick: t(42),
            from_buffer: true
        }));
        assert_eq!(engine.active(), Some(ActionId(2)));
        assert_eq!(engine.buffered(), None);
    }

    /// Leaving an action early from inside its active window closes the window immediately before
    /// `Ended`: on interrupt, death and replacement. A late death past the window does not invent
    /// a second close.
    #[test]
    fn early_exit_closes_the_active_window_before_ending() {
        // A wide contact window keeps the action active through its cancel window at 30.
        let reach = |id: u32| ActionSpec {
            contact: Some(TickWindow { from: 20, to: 40 }),
            cancel_from_ticks: Some(30),
            ..melee(id, 60)
        };

        let mut engine = engine_with(vec![reach(1), melee(2, 50)]);
        engine.submit(ActionId(1), t(0)).expect("submit");
        engine.submit(ActionId(2), t(10)).expect("submit");
        let replaced = engine.advance_to(t(31)).expect("advance");
        let ended = replaced
            .iter()
            .position(|event| {
                matches!(
                    event,
                    EngineEvent::Ended { tick, .. } if *tick == t(30)
                )
            })
            .expect("the replacement ends the first action");
        assert_eq!(
            replaced[ended],
            EngineEvent::Ended {
                id: ActionId(1),
                tick: t(30),
                reason: ActionEnd::Replaced { by: ActionId(2) }
            }
        );
        assert_eq!(
            replaced[ended - 1],
            EngineEvent::ActiveClosed {
                id: ActionId(1),
                tick: t(30)
            },
            "ActiveClosed comes immediately before the replacement's Ended"
        );

        let mut engine = engine_with(vec![reach(1)]);
        engine.submit(ActionId(1), t(0)).expect("submit");
        let died = engine.die(t(25)).expect("die");
        assert_eq!(
            died.get(died.len() - 2),
            Some(&EngineEvent::ActiveClosed {
                id: ActionId(1),
                tick: t(25)
            })
        );
        assert_eq!(
            died.last(),
            Some(&EngineEvent::Ended {
                id: ActionId(1),
                tick: t(25),
                reason: ActionEnd::Died
            })
        );

        let mut engine = engine_with(vec![reach(1)]);
        engine.submit(ActionId(1), t(0)).expect("submit");
        let died = engine.die(t(45)).expect("die");
        let closes: Vec<&EngineEvent> = died
            .iter()
            .filter(|event| matches!(event, EngineEvent::ActiveClosed { .. }))
            .collect();
        assert_eq!(
            closes,
            vec![&EngineEvent::ActiveClosed {
                id: ActionId(1),
                tick: t(40)
            }],
            "the tick walk already closed the window once, at its end"
        );
    }
}
