//! The enemy decision brain: a pure, integer-tick harness for the RFC's telegraph loop.
//!
//! The AI chooses *which* action to start and *when*; the action rules own the windows. This
//! module is the offline harness and the future server adapter's rules surface: it consumes the
//! creature's compiled [`ActionTable`] (its own attack clips, compiled per rig) and reports
//! telegraph, contact and recovery edges. It never rolls damage, checks range geometry or
//! talks to the map — the caller supplies the tick and decides when the target is in reach.
//!
//! Cadence is authored (`EnemyBrainConfig`), not extracted: the server adapter feeds the
//! creature's `UNIT_FIELD_BASEATTACKTIME` into the same field when it lands.

use crate::action::{ActionId, ActionKind, ActionSpec, ActionTable};
use crate::error::{CombatError, CombatResult};
use crate::time::{Ticks, TickWindow};

/// Authored enemy pacing.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct EnemyBrainConfig {
    /// Ticks between attack starts at the quickest.
    pub cadence_ticks: u64,
    /// Ticks of stagger recovery after a poise break.
    pub stagger_ticks: u64,
}

/// What the brain reports, in order.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum EnemyEvent {
    /// The chosen strike began; its telegraph is the gap to the active window.
    Telegraphed {
        id: ActionId,
        tick: Ticks,
        window: TickWindow,
    },
    /// The contact window closed.
    ContactClosed { id: ActionId, tick: Ticks },
    /// The strike ran to its end; recovery is over.
    StrikeEnded { id: ActionId, tick: Ticks },
    /// A poise break cut the strike short; nothing starts again until `until`.
    Staggered { tick: Ticks, until: Ticks },
    /// The creature died; nothing starts again.
    Died { tick: Ticks },
}

#[derive(Clone, Copy, Debug)]
struct ActiveAttack {
    spec: ActionSpec,
    started: Ticks,
    closed: bool,
}

/// The enemy's decision state over its compiled moves.
#[derive(Clone, Debug)]
pub struct EnemyBrain {
    table: ActionTable,
    moves: Vec<ActionId>,
    config: EnemyBrainConfig,
    active: Option<ActiveAttack>,
    next_move: usize,
    ready_at: Ticks,
    dead: bool,
    last: Ticks,
}

impl EnemyBrain {
    /// Build a brain over validated `moves` in `table`. Every move must resolve to an
    /// `EnemyStrike` action; anything else is a configuration error, never a silent skip.
    pub fn new(
        table: ActionTable,
        moves: Vec<ActionId>,
        config: EnemyBrainConfig,
    ) -> CombatResult<Self> {
        table.validate()?;
        if moves.is_empty() {
            return Err(CombatError::InvalidConfig(
                "an enemy brain needs at least one move",
            ));
        }
        for id in &moves {
            match table.get(*id) {
                Some(spec) if spec.kind == ActionKind::EnemyStrike => {}
                _ => {
                    return Err(CombatError::InvalidConfig(
                        "enemy moves must be EnemyStrike actions of the same table",
                    ))
                }
            }
        }
        if config.cadence_ticks == 0 || config.stagger_ticks == 0 {
            return Err(CombatError::InvalidConfig(
                "enemy cadence and stagger ticks must be non-zero",
            ));
        }
        Ok(Self {
            table,
            moves,
            config,
            active: None,
            next_move: 0,
            ready_at: Ticks::ZERO,
            dead: false,
            last: Ticks::ZERO,
        })
    }

    /// The running strike, if any.
    pub fn active(&self) -> Option<ActionId> {
        self.active.map(|active| active.spec.id)
    }

    /// When the next strike may start.
    pub fn ready_at(&self) -> Ticks {
        self.ready_at
    }

    /// Whether the brain is dead.
    pub fn is_dead(&self) -> bool {
        self.dead
    }

    /// Advance to `now`: report the running strike's edges, and start the next move when the
    /// slot is free and the cadence allows.
    pub fn tick(&mut self, now: Ticks) -> CombatResult<Vec<EnemyEvent>> {
        self.check_time(now)?;
        self.last = now;
        if self.dead {
            return Ok(Vec::new());
        }
        let mut events = self.edges(now);
        if self.active.is_none() && now >= self.ready_at {
            let id = self.moves[self.next_move % self.moves.len()];
            self.next_move = self.next_move.wrapping_add(1);
            if let Some(spec) = self.table.get(id).copied() {
                self.active = Some(ActiveAttack {
                    spec,
                    started: now,
                    closed: false,
                });
                // The next start waits at least the cadence and never overlaps the strike.
                let interval = self.config.cadence_ticks.max(spec.total_ticks);
                self.ready_at = now.saturating_add(interval);
                events.push(EnemyEvent::Telegraphed {
                    id,
                    tick: now,
                    window: spec
                        .contact
                        .unwrap_or(TickWindow { from: 0, to: 0 }),
                });
            }
        }
        Ok(events)
    }

    /// The edge events of the running strike only (no start decision).
    fn edges(&mut self, now: Ticks) -> Vec<EnemyEvent> {
        let mut events = Vec::new();
        let Some(active) = self.active else {
            return events;
        };
        let elapsed = now.elapsed_since(active.started);
        if let Some(window) = active.spec.contact {
            if !active.closed && elapsed >= window.to {
                if let Some(slot) = self.active.as_mut() {
                    slot.closed = true;
                }
                events.push(EnemyEvent::ContactClosed {
                    id: active.spec.id,
                    tick: now,
                });
            }
        }
        if elapsed >= active.spec.total_ticks {
            self.active = None;
            events.push(EnemyEvent::StrikeEnded {
                id: active.spec.id,
                tick: now,
            });
        }
        events
    }

    fn check_time(&self, now: Ticks) -> CombatResult<()> {
        if now < self.last {
            return Err(CombatError::TimeWentBackwards {
                now: self.last,
                requested: now,
            });
        }
        Ok(())
    }

    /// A poise break: the running strike ends without completing, and nothing starts until the
    /// stagger recovery elapses.
    pub fn interrupt(&mut self, now: Ticks) -> CombatResult<Vec<EnemyEvent>> {
        self.check_time(now)?;
        self.last = now;
        let mut events = self.edges(now);
        self.active = None;
        let until = now.saturating_add(self.config.stagger_ticks);
        self.ready_at = self.ready_at.max(until);
        events.push(EnemyEvent::Staggered { tick: now, until });
        Ok(events)
    }

    /// The creature died: nothing starts again.
    pub fn die(&mut self, now: Ticks) -> CombatResult<Vec<EnemyEvent>> {
        self.check_time(now)?;
        self.last = now;
        let mut events = self.edges(now);
        self.active = None;
        self.dead = true;
        events.push(EnemyEvent::Died { tick: now });
        Ok(events)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::identity::ActionRevision;
    use crate::time::TickRate;

    fn rate() -> TickRate {
        TickRate::new(100).expect("100 Hz")
    }

    fn strike(id: u32, total: u64) -> ActionSpec {
        ActionSpec {
            id: ActionId(id),
            kind: ActionKind::EnemyStrike,
            total_ticks: total,
            input_from_ticks: total,
            cancel_from_ticks: None,
            iframes: None,
            contact: Some(TickWindow { from: 20, to: 30 }),
            stamina_cost: 0,
            interruptible_until: None,
            followup: None,
        }
    }

    fn brain(moves: Vec<ActionSpec>, cadence: u64) -> EnemyBrain {
        let mut table = ActionTable::new(ActionRevision(1), rate());
        let ids: Vec<ActionId> = moves
            .iter()
            .map(|spec| {
                table.insert(*spec).expect("insert");
                spec.id
            })
            .collect();
        EnemyBrain::new(
            table,
            ids,
            EnemyBrainConfig {
                cadence_ticks: cadence,
                stagger_ticks: 40,
            },
        )
        .expect("brain")
    }

    fn t(value: u64) -> Ticks {
        Ticks::new(value)
    }

    /// The strike reports telegraph, contact-close and end once; the next move starts at the
    /// cadence and the round-robin cycles the move list.
    #[test]
    fn strikes_round_robin_on_the_cadence() {
        let mut brain = brain(vec![strike(1, 50), strike(2, 60)], 70);
        let first = brain.tick(t(0)).expect("tick");
        assert_eq!(
            first,
            vec![EnemyEvent::Telegraphed {
                id: ActionId(1),
                tick: t(0),
                window: TickWindow { from: 20, to: 30 }
            }]
        );
        assert_eq!(brain.active(), Some(ActionId(1)));
        assert_eq!(brain.ready_at(), t(70), "cadence outlives the strike");

        let contact = brain.tick(t(20)).expect("tick");
        assert!(
            contact.is_empty(),
            "the telegraph began at the strike's start; the window opening has no event"
        );
        let closed = brain.tick(t(30)).expect("tick");
        assert!(closed.contains(&EnemyEvent::ContactClosed {
            id: ActionId(1),
            tick: t(30)
        }));
        let ended = brain.tick(t(50)).expect("tick");
        assert!(ended.contains(&EnemyEvent::StrikeEnded {
            id: ActionId(1),
            tick: t(50)
        }));
        assert_eq!(brain.active(), None);
        assert!(
            brain.tick(t(60)).expect("tick").is_empty(),
            "the cadence still gates the next start"
        );
        let next = brain.tick(t(70)).expect("tick");
        assert!(next.contains(&EnemyEvent::Telegraphed {
            id: ActionId(2),
            tick: t(70),
            window: TickWindow { from: 20, to: 30 }
        }));
        assert_eq!(brain.active(), Some(ActionId(2)));
    }

    /// A poise break cuts the strike and holds every start until the stagger recovery ends.
    #[test]
    fn a_poise_break_staggers_before_the_next_strike() {
        let mut brain = brain(vec![strike(1, 50)], 30);
        brain.tick(t(0)).expect("tick");
        let staggered = brain.interrupt(t(10)).expect("interrupt");
        assert!(staggered.contains(&EnemyEvent::Staggered {
            tick: t(10),
            until: t(50)
        }));
        assert_eq!(brain.active(), None);
        assert!(brain.tick(t(40)).expect("tick").is_empty());
        let resumed = brain.tick(t(50)).expect("tick");
        assert!(resumed.contains(&EnemyEvent::Telegraphed {
            id: ActionId(1),
            tick: t(50),
            window: TickWindow { from: 20, to: 30 }
        }));
    }

    /// Death ends the strike and every future start; moves must be EnemyStrike actions, and an
    /// empty or foreign move list is a configuration error.
    #[test]
    fn death_stops_the_brain_and_validation_holds() {
        let mut brain = brain(vec![strike(1, 50)], 30);
        brain.tick(t(0)).expect("tick");
        let died = brain.die(t(5)).expect("die");
        assert!(died.contains(&EnemyEvent::Died { tick: t(5) }));
        assert!(brain.is_dead());
        assert!(brain.tick(t(1_000)).expect("tick").is_empty());

        let mut table = ActionTable::new(ActionRevision(1), rate());
        table.insert(strike(1, 50)).expect("insert");
        assert!(EnemyBrain::new(
            table.clone(),
            vec![],
            EnemyBrainConfig {
                cadence_ticks: 30,
                stagger_ticks: 40
            }
        )
        .is_err());
        let mut melee = strike(2, 50);
        melee.kind = ActionKind::MeleeLight;
        melee.stamina_cost = 20;
        table.insert(melee).expect("insert");
        assert!(EnemyBrain::new(
            table,
            vec![ActionId(2)],
            EnemyBrainConfig {
                cadence_ticks: 30,
                stagger_ticks: 40
            }
        )
        .is_err());
    }
}
