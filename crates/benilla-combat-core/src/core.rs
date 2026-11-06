//! The offline, server-authoritative combat reducer.
//!
//! This is an integration seam, not deployed authority and not a client predictor. Callers in
//! the authoritative server process may feed validated input frames here. A client-side
//! presentation layer must not use this core's effects as proof of server acceptance.

use crate::data::CombatConfig;
use crate::enemy::{Enemy, EnemyPhase, EnemyView};
use crate::error::{CombatError, CombatResult};
use crate::identity::{ActionRevision, PlayerId, Sequence, SessionEpoch};
use crate::input::{
    BufferedDodge, DodgeRejection, DodgeResult, GestureOutcome, GestureState, InputBuffer,
    InputFrame, InputOutcome,
};
use crate::player::{HitOutcome, Player, PlayerPhase, PlayerView};
use crate::time::Ticks;

/// Largest catch-up accepted by one `advance_to` call. A caller with a bigger gap must advance in
/// bounded chunks instead of making the reducer perform unbounded work.
pub const MAX_ADVANCE_TICKS: u64 = 240;

/// Bound on deferred (future-tick) input frames. The gameplay buffer itself still has one slot.
pub const MAX_PENDING_INPUTS: usize = 16;

/// Offline reducer events. They are not network messages and do not prove a live server outcome.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum CombatEvent {
    DodgeBuffered {
        at: Ticks,
        expires_at: Ticks,
        revision: ActionRevision,
    },
    DodgeBufferExpired {
        at: Ticks,
        revision: ActionRevision,
    },
    DodgeStarted {
        at: Ticks,
        revision: ActionRevision,
        stamina_remaining: u32,
    },
    DodgeRejected {
        at: Ticks,
        reason: DodgeRejection,
    },
    DodgeEnded {
        at: Ticks,
        revision: ActionRevision,
    },
    PlayerInterrupted {
        at: Ticks,
        until: Ticks,
    },
    PlayerDied {
        at: Ticks,
    },
    EnemyTelegraphStarted {
        at: Ticks,
        strike_at: Ticks,
    },
    EnemyHit {
        at: Ticks,
        outcome: HitOutcome,
        /// Damage actually applied; zero for a dodged or already-dead target.
        damage: u32,
        player_hp: u32,
        stamina_remaining: u32,
    },
    EncounterReset {
        at: Ticks,
    },
    Reconnected {
        at: Ticks,
        session: SessionEpoch,
        player: PlayerId,
    },
}

#[derive(Clone, PartialEq, Eq, Debug)]
pub struct AdvanceReport {
    pub from: Ticks,
    pub to: Ticks,
    pub events: Vec<CombatEvent>,
}

#[derive(Clone, PartialEq, Eq, Debug)]
pub struct SubmitOutcome {
    /// True when the frame was queued for a future tick and no gesture was evaluated yet.
    pub deferred: bool,
    pub input: InputOutcome,
    pub events: Vec<CombatEvent>,
}

#[derive(Clone, PartialEq, Eq, Debug)]
pub struct ReconcileReport {
    pub from: Ticks,
    pub to: Ticks,
    pub accepted_sequence: Option<Sequence>,
}

/// A complete authoritative state image for reconciliation. Phases are included so a malformed or
/// mixed-version snapshot cannot silently pass as another state.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct CombatSnapshot {
    pub session: SessionEpoch,
    pub player: PlayerId,
    pub server_tick: Ticks,
    pub last_sequence: Option<Sequence>,
    pub action_revision: ActionRevision,
    pub player_hp: u32,
    pub player_max_hp: u32,
    pub stamina: u32,
    pub stamina_max: u32,
    pub stamina_regen_resume_at: Option<Ticks>,
    pub player_phase: PlayerPhase,
    pub dodge_started_at: Option<Ticks>,
    pub interrupted_until: Option<Ticks>,
    pub enemy_hp: u32,
    pub enemy_max_hp: u32,
    pub enemy_phase: EnemyPhase,
    pub enemy_telegraph_started_at: Option<Ticks>,
    pub enemy_active_resolved: bool,
}

#[derive(Clone, Debug)]
pub struct CombatCore {
    config: CombatConfig,
    session: SessionEpoch,
    player_id: PlayerId,
    now: Ticks,
    last_sequence: Option<Sequence>,
    player: Player,
    enemy: Enemy,
    buffer: InputBuffer,
    gesture: GestureState,
    pending: Vec<InputFrame>,
}

impl CombatCore {
    pub fn new(
        config: CombatConfig,
        session: SessionEpoch,
        player: PlayerId,
        start: Ticks,
    ) -> CombatResult<Self> {
        config.validate()?;
        let mut core = Self {
            config,
            session,
            player_id: player,
            now: start,
            last_sequence: None,
            player: Player::new(&config),
            enemy: Enemy::new(&config),
            buffer: InputBuffer::new(),
            gesture: GestureState::default(),
            pending: Vec::new(),
        };
        core.player.sync_phase(start, &core.config);
        core.enemy.sync_phase(start, &core.config);
        Ok(core)
    }

    pub fn config(&self) -> &CombatConfig {
        &self.config
    }

    pub fn session(&self) -> SessionEpoch {
        self.session
    }

    pub fn player_id(&self) -> PlayerId {
        self.player_id
    }

    pub fn now(&self) -> Ticks {
        self.now
    }

    pub fn last_sequence(&self) -> Option<Sequence> {
        self.last_sequence
    }

    pub fn player(&self) -> PlayerView {
        self.player.view(self.now, &self.config)
    }

    pub fn enemy(&self) -> EnemyView {
        self.enemy.view()
    }

    pub fn buffered_dodge(&self) -> Option<BufferedDodge> {
        self.buffer.current()
    }

    pub fn pending_inputs(&self) -> usize {
        self.pending.len()
    }

    /// Validate an input envelope and either apply it now or defer it to its future tick.
    ///
    /// Sequence ordering is committed only after every envelope check passes. A gameplay refusal
    /// (dead, interrupted, no stamina, closed input window) still consumes the sequence, because
    /// the server received it.
    pub fn submit(&mut self, frame: InputFrame) -> CombatResult<SubmitOutcome> {
        if frame.session != self.session {
            return Err(CombatError::SessionMismatch {
                expected: self.session,
                got: frame.session,
            });
        }
        if frame.player != self.player_id {
            return Err(CombatError::PlayerMismatch {
                expected: self.player_id,
                got: frame.player,
            });
        }
        self.check_sequence(frame.sequence)?;
        if frame.action_revision != self.config.dodge.revision {
            return Err(CombatError::ActionRevisionMismatch {
                expected: self.config.dodge.revision,
                got: frame.action_revision,
            });
        }
        if frame.tick < self.now {
            return Err(CombatError::InputInPast {
                now: self.now,
                input: frame.tick,
            });
        }
        let lead = frame.tick.0 - self.now.0;
        if lead > self.config.max_input_lead_ticks {
            return Err(CombatError::InputTooFarAhead {
                now: self.now,
                input: frame.tick,
                limit: self.config.max_input_lead_ticks,
            });
        }
        frame.dodge.validate()?;

        // Accepted input time is nondecreasing. Without this, a higher sequence could arrive on
        // an earlier tick than a frame already waiting, and the deferred frame would apply after
        // its successor.
        if let Some(last_tick) = self.pending.last().map(|pending| pending.tick) {
            if frame.tick < last_tick {
                return Err(CombatError::InputOutOfOrderTick {
                    last: last_tick,
                    got: frame.tick,
                });
            }
        }

        if frame.tick > self.now && self.pending.len() >= MAX_PENDING_INPUTS {
            return Err(CombatError::InputQueueFull {
                limit: MAX_PENDING_INPUTS,
            });
        }

        self.last_sequence = Some(frame.sequence);

        if frame.tick > self.now {
            self.pending.push(frame);
            self.pending
                .sort_by_key(|pending| (pending.tick, pending.sequence));
            return Ok(SubmitOutcome {
                deferred: true,
                input: InputOutcome {
                    sequence: frame.sequence,
                    tick: frame.tick,
                    gesture: GestureOutcome::Deferred,
                    dodge: DodgeResult::None,
                },
                events: Vec::new(),
            });
        }

        let mut events = Vec::new();
        let input = self.apply_input(frame, &mut events);
        Ok(SubmitOutcome {
            deferred: false,
            input,
            events,
        })
    }

    /// Advance tick by tick. Zero ticks is a no-op; more than [`MAX_ADVANCE_TICKS`] is refused.
    pub fn advance_to(&mut self, target: Ticks) -> CombatResult<AdvanceReport> {
        let from = self.now;
        if target < from {
            return Err(CombatError::TimeWentBackwards {
                now: from,
                requested: target,
            });
        }
        let span = target.0 - from.0;
        if span > MAX_ADVANCE_TICKS {
            return Err(CombatError::AdvanceTooLarge {
                from,
                to: target,
                limit: MAX_ADVANCE_TICKS,
            });
        }

        let mut events = Vec::new();
        let mut tick = from.0;
        while tick < target.0 {
            tick = tick
                .checked_add(1)
                .ok_or(CombatError::TickArithmeticOverflow)?;
            let now = Ticks(tick);
            self.now = now;
            self.step(now, &mut events);
        }

        Ok(AdvanceReport {
            from,
            to: target,
            events,
        })
    }

    pub fn begin_enemy_telegraph(&mut self) -> CombatResult<CombatEvent> {
        if self.player.is_dead() {
            return Err(CombatError::PlayerDead);
        }
        if self.enemy.is_dead() {
            return Err(CombatError::EnemyDead);
        }
        let strike_at = self
            .now
            .checked_add(self.config.enemy.windup_ticks)
            .ok_or(CombatError::TickArithmeticOverflow)?;
        if !self.enemy.begin_telegraph(self.now) {
            return Err(CombatError::EnemyBusy);
        }
        Ok(CombatEvent::EnemyTelegraphStarted {
            at: self.now,
            strike_at,
        })
    }

    /// Restore both actors. The session, player, clock, and accepted sequence survive so a reset
    /// cannot be used to replay an old input. Deferred future inputs are discarded.
    pub fn reset_encounter(&mut self) -> CombatEvent {
        self.player.reset();
        self.enemy.reset();
        self.buffer.clear();
        self.pending.clear();
        self.gesture.reset();
        CombatEvent::EncounterReset { at: self.now }
    }

    /// Start a new connection generation. The epoch must advance and time must not go backwards.
    pub fn reconnect(
        &mut self,
        session: SessionEpoch,
        player: PlayerId,
        at: Ticks,
    ) -> CombatResult<CombatEvent> {
        if session <= self.session {
            return Err(CombatError::SessionNotAdvanced {
                previous: self.session,
                requested: session,
            });
        }
        if at < self.now {
            return Err(CombatError::TimeWentBackwards {
                now: self.now,
                requested: at,
            });
        }

        self.session = session;
        self.player_id = player;
        self.now = at;
        self.last_sequence = None;
        self.player.reset();
        self.enemy.reset();
        self.buffer.clear();
        self.pending.clear();
        self.gesture.reset();
        Ok(CombatEvent::Reconnected {
            at,
            session,
            player,
        })
    }

    pub fn snapshot(&self) -> CombatSnapshot {
        CombatSnapshot {
            session: self.session,
            player: self.player_id,
            server_tick: self.now,
            last_sequence: self.last_sequence,
            action_revision: self.config.dodge.revision,
            player_hp: self.player.hp,
            player_max_hp: self.player.max_hp,
            stamina: self.player.stamina.current(),
            stamina_max: self.player.stamina.max(),
            stamina_regen_resume_at: self.player.stamina.regen_resume_at(),
            player_phase: self.player.phase,
            dodge_started_at: self.player.dodge_started_at(),
            interrupted_until: self.player.interrupted_until(),
            enemy_hp: self.enemy.hp,
            enemy_max_hp: self.enemy.max_hp,
            enemy_phase: self.enemy.phase,
            enemy_telegraph_started_at: self.enemy.telegraph_started_at(),
            enemy_active_resolved: self.enemy.active_resolved(),
        }
    }

    /// Replace local state with an authoritative image. This is not a rewind: the snapshot may be
    /// newer, never older. A different session or player must reconnect instead.
    pub fn reconcile(&mut self, snapshot: CombatSnapshot) -> CombatResult<ReconcileReport> {
        if snapshot.session != self.session {
            return Err(CombatError::SnapshotSessionMismatch {
                expected: self.session,
                got: snapshot.session,
            });
        }
        if snapshot.player != self.player_id {
            return Err(CombatError::SnapshotPlayerMismatch {
                expected: self.player_id,
                got: snapshot.player,
            });
        }
        if snapshot.action_revision != self.config.dodge.revision {
            return Err(CombatError::SnapshotRevisionMismatch {
                expected: self.config.dodge.revision,
                got: snapshot.action_revision,
            });
        }
        if snapshot.player_max_hp != self.config.player_max_hp {
            return Err(CombatError::SnapshotInvalid(
                "player_max_hp does not match the active configuration",
            ));
        }
        if snapshot.server_tick < self.now {
            return Err(CombatError::SnapshotStaleTick {
                current: self.now,
                snapshot: snapshot.server_tick,
            });
        }
        if let Some(current) = self.last_sequence {
            if snapshot
                .last_sequence
                .map_or(true, |accepted| accepted < current)
            {
                return Err(CombatError::SnapshotStaleSequence {
                    current: self.last_sequence,
                    snapshot: snapshot.last_sequence,
                });
            }
        }

        let player = Player::restore(
            snapshot.server_tick,
            &self.config,
            snapshot.player_hp,
            snapshot.stamina,
            snapshot.stamina_max,
            snapshot.stamina_regen_resume_at,
            snapshot.dodge_started_at,
            snapshot.interrupted_until,
            snapshot.player_phase,
        )
        .ok_or(CombatError::SnapshotInvalid(
            "player fields do not form the declared phase",
        ))?;

        let enemy = Enemy::restore(
            snapshot.server_tick,
            &self.config,
            snapshot.enemy_hp,
            snapshot.enemy_max_hp,
            snapshot.enemy_telegraph_started_at,
            snapshot.enemy_active_resolved,
            snapshot.enemy_phase,
        )
        .ok_or(CombatError::SnapshotInvalid(
            "enemy fields do not form the declared phase",
        ))?;

        let from = self.now;
        self.now = snapshot.server_tick;
        self.player = player;
        self.enemy = enemy;
        self.last_sequence = snapshot.last_sequence;
        self.buffer.clear();
        self.pending.clear();
        self.gesture.reset();

        Ok(ReconcileReport {
            from,
            to: self.now,
            accepted_sequence: self.last_sequence,
        })
    }

    fn check_sequence(&self, sequence: Sequence) -> CombatResult<()> {
        let Some(last) = self.last_sequence else {
            return Ok(());
        };
        if sequence == last {
            return Err(CombatError::DuplicateSequence { last });
        }
        if sequence < last {
            return Err(CombatError::StaleSequence { last, got: sequence });
        }
        if sequence.0 - last.0 > self.config.max_sequence_gap {
            return Err(CombatError::SequenceGapTooLarge {
                last,
                got: sequence,
                limit: self.config.max_sequence_gap,
            });
        }
        Ok(())
    }

    fn step(&mut self, now: Ticks, events: &mut Vec<CombatEvent>) {
        let ending_revision = self.player.active_revision();
        if self.player.sync_phase(now, &self.config) {
            if let Some(revision) = ending_revision {
                events.push(CombatEvent::DodgeEnded {
                    at: now,
                    revision,
                });
            }
        }

        self.apply_due_inputs(now, events);
        self.attempt_buffered(now, events);

        if self.player.phase == PlayerPhase::Ready {
            self.player
                .stamina
                .regenerate(now, self.config.stamina.regen_per_tick);
        }

        self.enemy.sync_phase(now, &self.config);
        if self.enemy.take_active_resolution() {
            self.resolve_enemy_hit(now, events);
        }
    }

    fn apply_due_inputs(&mut self, now: Ticks, events: &mut Vec<CombatEvent>) {
        loop {
            let Some(index) = self.pending.iter().position(|frame| frame.tick <= now) else {
                break;
            };
            let frame = self.pending.remove(index);
            let _ = self.apply_input(frame, events);
        }
    }

    fn apply_input(&mut self, frame: InputFrame, events: &mut Vec<CombatEvent>) -> InputOutcome {
        let ending_revision = self.player.active_revision();
        if self.player.sync_phase(frame.tick, &self.config) {
            if let Some(revision) = ending_revision {
                events.push(CombatEvent::DodgeEnded {
                    at: frame.tick,
                    revision,
                });
            }
        }

        let gesture = self
            .gesture
            .update(frame.tick, frame.dodge, self.config.sprint_hold_ticks);
        let dodge = match gesture {
            GestureOutcome::ShortRelease { .. } => {
                self.request_dodge(frame.tick, frame.action_revision, events)
            }
            _ => DodgeResult::None,
        };

        InputOutcome {
            sequence: frame.sequence,
            tick: frame.tick,
            gesture,
            dodge,
        }
    }

    fn request_dodge(
        &mut self,
        now: Ticks,
        revision: ActionRevision,
        events: &mut Vec<CombatEvent>,
    ) -> DodgeResult {
        if self.player.is_dead() {
            return Self::reject(now, DodgeRejection::Dead, events);
        }
        if self.player.phase == PlayerPhase::Interrupted {
            return Self::reject(now, DodgeRejection::Interrupted, events);
        }
        if !self.player.listening_for_dodge(now, &self.config) {
            return Self::reject(now, DodgeRejection::InputWindowClosed, events);
        }

        if self.player.can_start_dodge(now, &self.config) {
            return self.start_or_reject(now, revision, events);
        }

        let buffered = self.buffer.push(
            revision,
            now,
            self.config.input_buffer_ttl_ticks,
        );
        events.push(CombatEvent::DodgeBuffered {
            at: now,
            expires_at: buffered.expires_at,
            revision,
        });
        DodgeResult::Buffered {
            expires_at: buffered.expires_at,
        }
    }

    fn attempt_buffered(&mut self, now: Ticks, events: &mut Vec<CombatEvent>) {
        if let Some(expired) = self.buffer.expire(now) {
            events.push(CombatEvent::DodgeBufferExpired {
                at: now,
                revision: expired.revision,
            });
            return;
        }

        let Some(buffered) = self.buffer.current() else {
            return;
        };
        if buffered.revision != self.config.dodge.revision {
            self.buffer.clear();
            return;
        }
        if !self.player.can_start_dodge(now, &self.config) {
            return;
        }
        // Keep the request until its TTL rather than spending it on a tick where stamina has not
        // regenerated yet. The next Ready tick after regeneration can still satisfy it.
        if self.player.stamina.current() == 0 {
            return;
        }

        self.buffer.clear();
        let _ = self.start_or_reject(now, buffered.revision, events);
    }

    fn start_or_reject(
        &mut self,
        now: Ticks,
        revision: ActionRevision,
        events: &mut Vec<CombatEvent>,
    ) -> DodgeResult {
        if !self.player.can_start_dodge(now, &self.config) {
            return Self::reject(now, DodgeRejection::NotReady, events);
        }
        if self.player.stamina.current() == 0 {
            return Self::reject(now, DodgeRejection::InsufficientStamina, events);
        }
        if !self.player.stamina.spend(
            self.config.dodge.stamina_cost,
            now,
            self.config.stamina.regen_delay_ticks,
        ) {
            return Self::reject(now, DodgeRejection::InsufficientStamina, events);
        }

        let replaced_revision = self.player.active_revision();
        self.player.start_dodge(now, &self.config, revision);
        if let Some(replaced_revision) = replaced_revision {
            events.push(CombatEvent::DodgeEnded {
                at: now,
                revision: replaced_revision,
            });
        }
        self.buffer.clear();
        let stamina_remaining = self.player.stamina.current();
        events.push(CombatEvent::DodgeStarted {
            at: now,
            revision,
            stamina_remaining,
        });
        DodgeResult::Started {
            revision,
            stamina_remaining,
        }
    }

    fn resolve_enemy_hit(&mut self, now: Ticks, events: &mut Vec<CombatEvent>) {
        let hp_before = self.player.hp;
        let ending_revision = self.player.active_revision();
        let outcome = self
            .player
            .receive_hit(now, &self.config, self.config.enemy.damage);
        if matches!(outcome, HitOutcome::Hit | HitOutcome::Killed) {
            if let Some(revision) = ending_revision {
                events.push(CombatEvent::DodgeEnded { at: now, revision });
            }
        }
        let damage = hp_before.saturating_sub(self.player.hp);
        let stamina_remaining = self.player.stamina.current();
        events.push(CombatEvent::EnemyHit {
            at: now,
            outcome,
            damage,
            player_hp: self.player.hp,
            stamina_remaining,
        });

        match outcome {
            HitOutcome::Hit => {
                if let Some(until) = self.player.interrupted_until() {
                    events.push(CombatEvent::PlayerInterrupted { at: now, until });
                }
                self.buffer.clear();
                self.gesture.reset();
            }
            HitOutcome::Killed => {
                events.push(CombatEvent::PlayerDied { at: now });
                self.buffer.clear();
                self.gesture.reset();
            }
            HitOutcome::Dodged | HitOutcome::IgnoredDead => {}
        }
    }

    fn reject(
        now: Ticks,
        reason: DodgeRejection,
        events: &mut Vec<CombatEvent>,
    ) -> DodgeResult {
        events.push(CombatEvent::DodgeRejected { at: now, reason });
        DodgeResult::Rejected(reason)
    }
}
