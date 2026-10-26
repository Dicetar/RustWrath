use std::sync::{Arc, Condvar, Mutex, TryLockError};
use std::thread::{self, JoinHandle};
use std::time::{Duration, Instant};

use crate::action::{ActionRequest, ActionResult};
use crate::client::{
    AckReceipt, HealthAckReceipt, LoopbackConfig, LoopbackSink, PeerState,
};
use crate::error::{Error, Result};
use crate::health::SourceHealth;
use crate::observation::Observation;

pub const DEFAULT_RECONNECT_INITIAL_DELAY: Duration = Duration::from_millis(100);
pub const DEFAULT_RECONNECT_MAX_DELAY: Duration = Duration::from_secs(2);
pub const DEFAULT_BUSY_RETRY_DELAY: Duration = Duration::from_millis(1);
pub const DEFAULT_RETRY_MAX_AGE: Duration = Duration::from_secs(60);
pub const DEFAULT_HEALTH_POLL_INTERVAL: Duration = Duration::from_millis(500);
pub const DEFAULT_HEALTH_SEND_INTERVAL: Duration = Duration::from_secs(5);
/// How long a producer's last `publish_health` call stays live. It is a lease on the producer
/// loop, not on the world session: a quiet server keeps renewing it from the same event loop.
pub const DEFAULT_PRODUCER_HEALTH_TTL: Duration = Duration::from_secs(15);
/// Hard bound on results forwarded to the socket per worker tick.
const MAX_RESULTS_PER_TICK: usize = 32;

/// The two ends of the same loopback seam for typed player actions. The worker forwards
/// `requests` to the application and writes `results` back; both are bounded by the WM broker and
/// the client adapter, never by an invented second bus.
#[derive(Clone)]
pub struct ActionBridge {
    pub requests: crossbeam_channel::Sender<ActionRequest>,
    /// Admission-level cancellations, forwarded to the app before its request drain.
    pub cancels: crossbeam_channel::Sender<String>,
    pub results: crossbeam_channel::Receiver<ActionResult>,
}

impl std::fmt::Debug for ActionBridge {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("ActionBridge")
    }
}

/// Configuration for the bounded, background-I/O publisher.
#[derive(Clone, Debug)]
pub struct PublisherConfig {
    loopback: LoopbackConfig,
    reconnect_initial_delay: Duration,
    reconnect_max_delay: Duration,
    retry_max_age: Duration,
    health_poll_interval: Duration,
    producer_health_ttl: Duration,
    actions: Option<ActionBridge>,
}

impl PublisherConfig {
    pub fn new(loopback: LoopbackConfig) -> Self {
        Self {
            loopback,
            reconnect_initial_delay: DEFAULT_RECONNECT_INITIAL_DELAY,
            reconnect_max_delay: DEFAULT_RECONNECT_MAX_DELAY,
            retry_max_age: DEFAULT_RETRY_MAX_AGE,
            health_poll_interval: DEFAULT_HEALTH_POLL_INTERVAL,
            producer_health_ttl: DEFAULT_PRODUCER_HEALTH_TTL,
            actions: None,
        }
    }

    /// Attaches the typed action seam to this publisher. Without it the transport remains
    /// observation-only (the renderless driver's mode).
    pub fn with_actions(mut self, actions: ActionBridge) -> Self {
        self.actions = Some(actions);
        self
    }

    pub fn with_reconnect_backoff(
        loopback: LoopbackConfig,
        reconnect_initial_delay: Duration,
        reconnect_max_delay: Duration,
    ) -> Result<Self> {
        if reconnect_initial_delay.is_zero() || reconnect_max_delay.is_zero() {
            return Err(Error::Config("publisher reconnect delays must be non-zero"));
        }
        if reconnect_initial_delay > reconnect_max_delay {
            return Err(Error::Config(
                "publisher initial reconnect delay must not exceed the maximum",
            ));
        }
        Ok(Self {
            loopback,
            reconnect_initial_delay,
            reconnect_max_delay,
            retry_max_age: DEFAULT_RETRY_MAX_AGE,
            health_poll_interval: DEFAULT_HEALTH_POLL_INTERVAL,
            producer_health_ttl: DEFAULT_PRODUCER_HEALTH_TTL,
            actions: None,
        })
    }

    /// Bounds how long a snapshot may be retried after a failed write/ACK or idle disconnect.
    pub fn with_retry_max_age(mut self, retry_max_age: Duration) -> Result<Self> {
        if retry_max_age.is_zero() {
            return Err(Error::Config("publisher retry max age must be non-zero"));
        }
        self.retry_max_age = retry_max_age;
        Ok(self)
    }

    /// Sets the idle peer-health probe cadence.
    pub fn with_health_poll_interval(mut self, health_poll_interval: Duration) -> Result<Self> {
        if health_poll_interval.is_zero() {
            return Err(Error::Config(
                "publisher health poll interval must be non-zero",
            ));
        }
        self.health_poll_interval = health_poll_interval;
        Ok(self)
    }

    /// Bounds how long the publisher may keep sending source health last supplied by the producer.
    /// The producer event loop must call `publish_health` again inside this lease; a stopped
    /// producer then loses in-world eligibility even while the publisher worker survives.
    pub fn with_producer_health_ttl(mut self, producer_health_ttl: Duration) -> Result<Self> {
        if producer_health_ttl.is_zero() {
            return Err(Error::Config(
                "publisher producer health TTL must be non-zero",
            ));
        }
        self.producer_health_ttl = producer_health_ttl;
        Ok(self)
    }

    pub fn loopback(&self) -> &LoopbackConfig {
        &self.loopback
    }
}

/// Acknowledges a non-blocking handoff, not socket delivery.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct PublishReceipt {
    pub sequence: u64,
    pub state_revision: u64,
    /// True when this observation replaced an older observation that had not been sent.
    pub coalesced: bool,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct SnapshotStamp {
    pub sequence: u64,
    pub state_revision: u64,
    pub world_generation: u64,
    pub observed_at_unix_ms: i64,
}

impl SnapshotStamp {
    fn from_observation(observation: &Observation) -> Self {
        Self {
            sequence: observation.sequence,
            state_revision: observation.state_revision,
            world_generation: observation.world_generation,
            observed_at_unix_ms: observation.observed_at_unix_ms,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum WorkerState {
    Running,
    Stopped,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum LinkState {
    Disconnected,
    Connecting,
    Connected,
    Retrying,
    Closed,
}

/// Health counters owned by the publisher worker.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PublisherHealth {
    pub worker: WorkerState,
    pub link: LinkState,
    pub closed: bool,
    pub handed_off: u64,
    pub coalesced: u64,
    pub writes: u64,
    pub accepted: u64,
    pub health_acks: u64,
    pub rejected: u64,
    pub retry_expired: u64,
    pub connections: u64,
    pub last_handoff: Option<SnapshotStamp>,
    pub last_written: Option<SnapshotStamp>,
    pub last_accepted: Option<AckReceipt>,
    pub last_health_ack: Option<HealthAckReceipt>,
    pub actions_received: u64,
    pub action_results: u64,
    pub action_errors: u64,
    pub last_action_error: Option<String>,
    pub last_error: Option<String>,
}

impl Default for PublisherHealth {
    fn default() -> Self {
        Self {
            worker: WorkerState::Running,
            link: LinkState::Disconnected,
            closed: false,
            handed_off: 0,
            coalesced: 0,
            writes: 0,
            accepted: 0,
            health_acks: 0,
            rejected: 0,
            retry_expired: 0,
            connections: 0,
            last_handoff: None,
            last_written: None,
            last_accepted: None,
            last_health_ack: None,
            actions_received: 0,
            action_results: 0,
            action_errors: 0,
            last_action_error: None,
            last_error: None,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RetainReason {
    /// The short handoff critical section was contended. Nothing was queued.
    HandoffBusy,
}

/// One bounded latest-unsent snapshot returned by a busy handoff.
///
/// It preserves the exact source timestamp, sequence, and state revision. The caller must choose
/// exactly one retry owner: keep at most one of these and retry it no earlier than
/// [`Self::retry_at`], or, if the producer already retains the same snapshot, drop this value and
/// let the producer re-export it. Doing both would duplicate retention and acknowledgment. Do not
/// mutate it.
#[derive(Debug, PartialEq)]
pub struct RetainedObservation {
    reason: RetainReason,
    observation: Observation,
    retry_at: Instant,
}

impl RetainedObservation {
    fn handoff_busy(observation: Observation) -> Self {
        Self {
            reason: RetainReason::HandoffBusy,
            observation,
            retry_at: Instant::now() + DEFAULT_BUSY_RETRY_DELAY,
        }
    }

    pub fn reason(&self) -> RetainReason {
        self.reason
    }

    pub fn observation(&self) -> &Observation {
        &self.observation
    }

    pub fn sequence(&self) -> u64 {
        self.observation.sequence
    }

    pub fn state_revision(&self) -> u64 {
        self.observation.state_revision
    }

    pub fn observed_at_unix_ms(&self) -> i64 {
        self.observation.observed_at_unix_ms
    }

    pub fn retry_at(&self) -> Instant {
        self.retry_at
    }

    pub fn into_observation(self) -> Observation {
        self.observation
    }
}

/// Result of a non-blocking publication attempt.
#[derive(Debug, PartialEq)]
pub enum PublishOutcome {
    /// The snapshot is now the newest pending handoff (or replaced an older pending one).
    HandedOff(PublishReceipt),
    /// The handoff was busy. The caller chooses the single retry owner: retain this exact snapshot
    /// for a timed retry, or drop it because the producer already retains it.
    Retained(RetainedObservation),
    /// A newer snapshot was already accepted; drop this retained snapshot.
    Superseded {
        sequence: u64,
        latest_sequence: u64,
    },
}

struct SharedState {
    pending: Option<Observation>,
    last_published: Option<Observation>,
    latest_source_health: Option<SourceHealth>,
    latest_source_health_at: Option<Instant>,
    health: PublisherHealth,
    closed: bool,
    stopped: bool,
}

struct Shared {
    state: Mutex<SharedState>,
    wake: Condvar,
}

/// A latest-only bounded sender handoff.
///
/// `publish` performs no socket connection, handshake, read, write, or ACK wait and uses `try_lock`
/// for the handoff slot. One worker owns all blocking socket operations and health probing. At most
/// one observation is pending and one is in flight; a newer publication replaces the pending one.
/// This is intentionally a latest-only observation stream, not a reliable event queue.
pub struct ObservationPublisher {
    shared: Arc<Shared>,
    worker: Option<JoinHandle<()>>,
}

impl ObservationPublisher {
    /// Spawns the I/O worker. The worker connects lazily when the first observation is published.
    pub fn spawn(config: PublisherConfig) -> Result<Self> {
        Self::spawn_internal(config, None)
    }

    /// Spawns a replacement worker that will not attempt a connection until `previous` has fully
    /// stopped and closed its socket.
    ///
    /// This call itself returns immediately; the ordering wait runs on the new worker. Keep
    /// `previous` alive until this returns, then it may be dropped (the old worker still completes
    /// and marks itself stopped).
    pub fn spawn_after(
        config: PublisherConfig,
        previous: &PublisherRetirement,
    ) -> Result<Self> {
        Self::spawn_internal(config, Some(Arc::clone(&previous.shared)))
    }

    fn spawn_internal(
        config: PublisherConfig,
        previous: Option<Arc<Shared>>,
    ) -> Result<Self> {
        let shared = Arc::new(Shared {
            state: Mutex::new(SharedState {
                pending: None,
                last_published: None,
                latest_source_health: None,
                latest_source_health_at: None,
                health: PublisherHealth::default(),
                closed: false,
                stopped: false,
            }),
            wake: Condvar::new(),
        });
        let worker_shared = Arc::clone(&shared);
        let worker = thread::Builder::new()
            .name("benilla-wm-native-publisher".to_owned())
            .spawn(move || {
                let _stop_guard = WorkerStopGuard {
                    shared: Arc::clone(&worker_shared),
                };
                if let Some(previous) = previous.as_ref() {
                    wait_until_stopped(previous);
                }
                run_worker(&worker_shared, config);
            })?;
        Ok(Self {
            shared,
            worker: Some(worker),
        })
    }

    /// Hands off one observation without waiting for socket I/O or the handoff mutex.
    ///
    /// A busy handoff returns the observation back to the caller in
    /// [`PublishOutcome::Retained`] with its original source timestamp and state revision. Either
    /// retry it via [`Self::retry`] no earlier than `retry_at`, even if no further source state
    /// change occurs, or drop it if the producer already retains it and will re-export. If a newer
    /// snapshot was handed off in the meantime, retry returns [`PublishOutcome::Superseded`] and
    /// the older snapshot must be dropped.
    pub fn publish(&self, observation: Observation) -> Result<PublishOutcome> {
        observation.validate()?;
        let mut state = match self.shared.state.try_lock() {
            Ok(state) => state,
            Err(TryLockError::WouldBlock) => {
                return Ok(PublishOutcome::Retained(
                    RetainedObservation::handoff_busy(observation),
                ));
            }
            Err(TryLockError::Poisoned(_)) => return Err(Error::Disconnected),
        };
        if state.closed || state.stopped {
            return Err(Error::Disconnected);
        }
        if let Some(previous) = state.last_published.as_ref() {
            if observation.session_epoch == previous.session_epoch
                && observation.sequence <= previous.sequence
            {
                return Ok(PublishOutcome::Superseded {
                    sequence: observation.sequence,
                    latest_sequence: previous.sequence,
                });
            }
            observation.validate_after(previous)?;
        }
        let receipt = PublishReceipt {
            sequence: observation.sequence,
            state_revision: observation.state_revision,
            coalesced: state.pending.is_some(),
        };
        let stamp = SnapshotStamp::from_observation(&observation);
        state.health.handed_off += 1;
        if receipt.coalesced {
            state.health.coalesced += 1;
        }
        state.health.last_handoff = Some(stamp);
        state.pending = Some(observation.clone());
        state.last_published = Some(observation);
        drop(state);
        self.shared.wake.notify_one();
        Ok(PublishOutcome::HandedOff(receipt))
    }

    /// Hands off current client↔world-server health without altering observation freshness.
    ///
    /// Returns false only when the short handoff lock was contended; the next pump tick retries.
    pub fn publish_health(&self, health: SourceHealth) -> Result<bool> {
        health.validate()?;
        let mut state = match self.shared.state.try_lock() {
            Ok(state) => state,
            Err(TryLockError::WouldBlock) => return Ok(false),
            Err(TryLockError::Poisoned(_)) => return Err(Error::Disconnected),
        };
        if state.closed || state.stopped {
            return Err(Error::Disconnected);
        }
        state.latest_source_health = Some(health);
        state.latest_source_health_at = Some(Instant::now());
        drop(state);
        self.shared.wake.notify_one();
        Ok(true)
    }

    /// Retries a caller-retained snapshot without changing its source time or revision.
    pub fn retry(&self, retained: RetainedObservation) -> Result<PublishOutcome> {
        self.publish(retained.into_observation())
    }

    /// Current publisher ownership: handed off, written, and receiver-accepted are separate.
    pub fn health(&self) -> PublisherHealth {
        match self.shared.state.lock() {
            Ok(state) => state.health.clone(),
            Err(_) => PublisherHealth {
                worker: WorkerState::Stopped,
                link: LinkState::Closed,
                closed: true,
                ..PublisherHealth::default()
            },
        }
    }

    /// Signals shutdown without waiting for the worker or in-flight socket operation.
    ///
    /// Use this on a packet/session transition path. The returned retirement handle can be used to
    /// wait off that path or to order a replacement worker via
    /// [`Self::spawn_after`].
    pub fn retire(mut self) -> PublisherRetirement {
        self.signal_close();
        PublisherRetirement {
            shared: Arc::clone(&self.shared),
            worker: self.worker.take(),
        }
    }

    /// Stops reconnection and joins the worker after its current bounded socket operation.
    ///
    /// This is a blocking lifecycle operation, not a packet-processing handoff. Use [`Self::retire`]
    /// or drop on latency-sensitive paths.
    pub fn close(self) {
        self.retire().join();
    }

    fn signal_close(&self) {
        if let Ok(mut state) = self.shared.state.lock() {
            state.closed = true;
            state.pending = None;
            state.health.closed = true;
            state.health.link = LinkState::Closed;
        }
        self.shared.wake.notify_all();
    }
}

impl Drop for ObservationPublisher {
    fn drop(&mut self) {
        self.signal_close();
    }
}

/// Handle to a publisher that has been signalled to stop but may still be in a bounded socket
/// operation.
pub struct PublisherRetirement {
    shared: Arc<Shared>,
    worker: Option<JoinHandle<()>>,
}

/// Replaces the current publisher, ordering the replacement after the old worker has stopped.
/// This is the one lifecycle policy shared by the renderless driver and the graphical session.
pub fn replace_publisher(
    config: Option<&PublisherConfig>,
    current: &mut Option<ObservationPublisher>,
    retiring: &mut Vec<PublisherRetirement>,
) -> Result<()> {
    if let Some(config) = config {
        if let Some(publisher) = current.take() {
            retiring.push(publisher.retire());
        }
        *current = Some(match retiring.last() {
            Some(retirement) => ObservationPublisher::spawn_after(config.clone(), retirement)?,
            None => ObservationPublisher::spawn(config.clone())?,
        });
    }
    retiring.retain(|retirement| !retirement.is_finished());
    Ok(())
}

/// Retires the current publisher without starting a replacement.
pub fn retire_publisher(
    current: &mut Option<ObservationPublisher>,
    retiring: &mut Vec<PublisherRetirement>,
) {
    if let Some(publisher) = current.take() {
        retiring.push(publisher.retire());
    }
    retiring.retain(|retirement| !retirement.is_finished());
}

impl PublisherRetirement {
    pub fn is_finished(&self) -> bool {
        match self.shared.state.lock() {
            Ok(state) => state.stopped,
            Err(_) => true,
        }
    }

    /// Waits off the packet path for worker completion. Returns true if stopped before the timeout.
    pub fn wait_timeout(&self, timeout: Duration) -> bool {
        let deadline = Instant::now() + timeout;
        let mut state = match self.shared.state.lock() {
            Ok(state) => state,
            Err(_) => return true,
        };
        while !state.stopped {
            let now = Instant::now();
            if now >= deadline {
                break;
            }
            let (next, _) = match self.shared.wake.wait_timeout(state, deadline - now) {
                Ok(result) => result,
                Err(_) => return true,
            };
            state = next;
        }
        state.stopped
    }

    /// Blocks until the old worker has stopped. Lifecycle path only.
    pub fn join(mut self) {
        if let Some(worker) = self.worker.take() {
            let _ = worker.join();
        }
    }
}

struct WorkerStopGuard {
    shared: Arc<Shared>,
}

impl Drop for WorkerStopGuard {
    fn drop(&mut self) {
        if let Ok(mut state) = self.shared.state.lock() {
            state.stopped = true;
            state.pending = None;
            state.health.worker = WorkerState::Stopped;
            if !state.health.closed {
                state.health.link = LinkState::Disconnected;
            }
        }
        self.shared.wake.notify_all();
    }
}

fn update_health(shared: &Arc<Shared>, update: impl FnOnce(&mut PublisherHealth)) {
    if let Ok(mut state) = shared.state.lock() {
        update(&mut state.health);
    }
}

fn is_closed(shared: &Arc<Shared>) -> bool {
    match shared.state.lock() {
        Ok(state) => state.closed || state.stopped,
        Err(_) => true,
    }
}

fn wait_until_stopped(shared: &Arc<Shared>) {
    let mut state = match shared.state.lock() {
        Ok(state) => state,
        Err(_) => return,
    };
    while !state.stopped {
        state = match shared.wake.wait(state) {
            Ok(state) => state,
            Err(_) => return,
        };
    }
}

fn schedule_retry(
    shared: &Arc<Shared>,
    config: &PublisherConfig,
    observation: Observation,
    origin: Instant,
    allow_old: bool,
    retry: &mut Option<Observation>,
    retry_at: &mut Option<Instant>,
    backoff: &mut Duration,
) {
    if !allow_old && origin.elapsed() > config.retry_max_age {
        update_health(shared, |health| {
            health.retry_expired += 1;
            health.last_error = Some("snapshot exceeded the retry age bound".to_owned());
            health.link = LinkState::Disconnected;
        });
        *retry = None;
        *retry_at = None;
        return;
    }
    *retry = Some(observation);
    *retry_at = Some(Instant::now() + *backoff);
    update_health(shared, |health| {
        health.link = LinkState::Retrying;
    });
    *backoff = std::cmp::min(*backoff * 2, config.reconnect_max_delay);
}

fn observation_key(observation: &Observation) -> (u64, u64, u64) {
    (
        observation.session_epoch,
        observation.world_generation,
        observation.sequence,
    )
}

fn restore_pending(shared: &Arc<Shared>, observation: Option<Observation>) {
    let Some(observation) = observation else {
        return;
    };
    if let Ok(mut state) = shared.state.lock() {
        if state.pending.is_none() {
            state.pending = Some(observation);
        }
    }
    shared.wake.notify_one();
}

fn latest_source_health(shared: &Arc<Shared>, ttl: Duration) -> Option<SourceHealth> {
    let (health, at) = {
        let state = shared.state.lock().ok()?;
        (state.latest_source_health.clone()?, state.latest_source_health_at?)
    };
    if at.elapsed() > ttl {
        update_health(shared, |health| {
            health.link = LinkState::Disconnected;
            health.last_error = Some("producer health lease expired".to_owned());
        });
        return None;
    }
    Some(health)
}

/// Moves delivered intents to the application and queued results to the socket. A closed app
/// receiver is not a transport failure: the worker keeps the observation/health stream alive and
/// the broker expires unanswered requests.
fn pump_actions(shared: &Arc<Shared>, sink: &mut LoopbackSink, bridge: &ActionBridge) {
    let (requests, cancels) = sink.take_actions();
    for request in requests {
        let _ = bridge.requests.send(request);
        update_health(shared, |health| health.actions_received += 1);
    }
    for request_id in cancels {
        // The application answers cancellation itself; a cancel for an already-finished request
        // is ignored there.
        let _ = bridge.cancels.send(request_id);
    }
    let results: Vec<_> = bridge
        .results
        .try_iter()
        .take(MAX_RESULTS_PER_TICK)
        .collect();
    for result in results {
        match sink.send_action_result(&result) {
            Ok(()) => update_health(shared, |health| health.action_results += 1),
            Err(error) => update_health(shared, |health| {
                health.action_errors += 1;
                health.last_action_error = Some(error.to_string());
            }),
        }
    }
}

fn run_worker(shared: &Arc<Shared>, config: PublisherConfig) {
    let mut sink: Option<LoopbackSink> = None;
    let mut retry: Option<Observation> = None;
    let mut retry_at: Option<Instant> = None;
    let mut backoff = config.reconnect_initial_delay;
    let mut last_sent: Option<Observation> = None;
    let mut health_sent_on_connection = false;
    let mut last_health: Option<SourceHealth> = None;
    let mut last_health_at: Option<Instant> = None;
    let mut health_retry_at: Option<Instant> = None;
    let mut health_backoff = config.reconnect_initial_delay;
    // Retry provenance (retry_max_age expiry) and receiver-acceptance tracking: the key is
    // the observation's (epoch, sequence, revision) triple; a retry keeps its original
    // origin so the age bound measures from when the snapshot first entered retry.
    let mut retry_key: Option<(u64, u64, u64)> = None;
    let mut retry_origin = Instant::now();
    let mut last_accepted_key: Option<(u64, u64, u64)> = None;

    loop {
        let (mut pending, closed, timed_out) = wait_for_work(
            shared,
            retry.is_some(),
            retry_at,
            Some(config.health_poll_interval),
        );
        if closed {
            if let Some(mut active) = sink.take() {
                active.close();
            }
            return;
        }

        if timed_out {
            if let Some(active) = sink.as_mut() {
                match active.poll_peer() {
                    Ok(PeerState::Alive) => {}
                    Ok(PeerState::Closed) => {
                        if let Some(mut active) = sink.take() {
                            active.close();
                        }
                        health_sent_on_connection = false;
                        update_health(shared, |health| {
                            health.link = LinkState::Disconnected;
                            health.last_error = Some("receiver connection closed".to_owned());
                        });
                        if let Some(last) = last_sent.clone() {
                            let key = observation_key(&last);
                            let origin = if retry_key == Some(key) {
                                retry_origin
                            } else {
                                Instant::now()
                            };
                            schedule_retry(
                                shared,
                                &config,
                                last,
                                origin,
                                last_accepted_key == Some(key),
                                &mut retry,
                                &mut retry_at,
                                &mut backoff,
                            );
                        }
                    }
                    Ok(PeerState::UnexpectedData) | Err(_) => {
                        if let Some(mut active) = sink.take() {
                            active.close();
                        }
                        health_sent_on_connection = false;
                        update_health(shared, |health| {
                            health.link = LinkState::Disconnected;
                            health.last_error =
                                Some("unexpected receiver traffic while idle".to_owned());
                        });
                        if let Some(last) = last_sent.clone() {
                            let key = observation_key(&last);
                            let origin = if retry_key == Some(key) {
                                retry_origin
                            } else {
                                Instant::now()
                            };
                            schedule_retry(
                                shared,
                                &config,
                                last,
                                origin,
                                last_accepted_key == Some(key),
                                &mut retry,
                                &mut retry_at,
                                &mut backoff,
                            );
                        }
                    }
                }
            }
        }

        // Forward peer-delivered intents (and any answers) after the idle poll, so a request
        // read on this tick reaches the app without an extra 500 ms wait.
        if let (Some(active), Some(bridge)) = (sink.as_mut(), config.actions.as_ref()) {
            pump_actions(shared, active, bridge);
        }

        let source_health = latest_source_health(shared, config.producer_health_ttl);
        if let Some(source_health) = source_health {
            let now = Instant::now();
            let due = !health_sent_on_connection
                || last_health.as_ref() != Some(&source_health)
                || last_health_at
                    .is_none_or(|at| at.elapsed() >= DEFAULT_HEALTH_SEND_INTERVAL);
            if due && health_retry_at.is_none_or(|at| now >= at) {
                if sink.is_none() {
                    match LoopbackSink::connect(config.loopback.clone()) {
                        Ok(connected) => {
                            sink = Some(connected);
                            update_health(shared, |health| {
                                health.connections += 1;
                                health.link = LinkState::Connected;
                            });
                        }
                        Err(error) => {
                            update_health(shared, |health| {
                                health.link = LinkState::Disconnected;
                                health.last_error = Some(error.to_string());
                            });
                            health_retry_at = Some(now + health_backoff);
                            health_backoff =
                                std::cmp::min(health_backoff * 2, config.reconnect_max_delay);
                            restore_pending(shared, pending.take());
                            continue;
                        }
                    }
                }
                if let Some(active) = sink.as_mut() {
                    match active.send_health(&source_health) {
                        Ok(ack) => {
                            health_sent_on_connection = true;
                            last_health = Some(source_health);
                            last_health_at = Some(now);
                            health_retry_at = None;
                            health_backoff = config.reconnect_initial_delay;
                            update_health(shared, |health| {
                                health.health_acks += 1;
                                health.last_health_ack = Some(ack);
                                health.link = LinkState::Connected;
                                health.last_error = None;
                            });
                        }
                        Err(error) => {
                            if let Some(mut active) = sink.take() {
                                active.close();
                            }
                            health_sent_on_connection = false;
                            update_health(shared, |health| {
                                health.link = LinkState::Disconnected;
                                health.last_error = Some(error.to_string());
                            });
                            health_retry_at = Some(now + health_backoff);
                            health_backoff =
                                std::cmp::min(health_backoff * 2, config.reconnect_max_delay);
                            restore_pending(shared, pending.take());
                            continue;
                        }
                    }
                }
            }
            if !health_sent_on_connection {
                restore_pending(shared, pending.take());
                continue;
            }
        }

        let observation = match pending {
            Some(observation) => {
                // A pending observation was published after any local retry, so it supersedes it.
                retry = None;
                retry_at = None;
                observation
            }
            None => match retry.take() {
                Some(observation) => observation,
                None => continue,
            },
        };

        let key = observation_key(&observation);
        if retry_key != Some(key) {
            retry_key = Some(key);
            retry_origin = Instant::now();
        }

        if sink.is_none() {
            update_health(shared, |health| {
                health.link = LinkState::Connecting;
            });
            match LoopbackSink::connect(config.loopback.clone()) {
                Ok(connected) => {
                    sink = Some(connected);
                    update_health(shared, |health| {
                        health.connections += 1;
                        health.link = LinkState::Connected;
                    });
                }
                Err(error) => {
                    update_health(shared, |health| {
                        health.link = LinkState::Disconnected;
                        health.last_error = Some(error.to_string());
                    });
                    schedule_retry(
                        shared,
                        &config,
                        observation,
                        retry_origin,
                        false,
                        &mut retry,
                        &mut retry_at,
                        &mut backoff,
                    );
                    continue;
                }
            }
        }

        // Close may have been signalled during connect/handshake. Do not start a new write.
        if is_closed(shared) {
            if let Some(mut active) = sink.take() {
                active.close();
            }
            return;
        }

        let write_result = sink.as_mut().map(|active| active.send(&observation));
        match write_result {
            Some(Ok(_)) => {
                last_sent = Some(observation.clone());
                update_health(shared, |health| {
                    health.writes += 1;
                    health.last_written = Some(SnapshotStamp::from_observation(&observation));
                    health.link = LinkState::Connected;
                });
            }
            Some(Err(error)) => {
                if let Some(mut active) = sink.take() {
                    active.close();
                }
                health_sent_on_connection = false;
                update_health(shared, |health| {
                    health.link = LinkState::Disconnected;
                    health.last_error = Some(error.to_string());
                });
                schedule_retry(
                    shared,
                    &config,
                    observation,
                    retry_origin,
                    last_accepted_key == Some(key),
                    &mut retry,
                    &mut retry_at,
                    &mut backoff,
                );
                continue;
            }
            None => {
                retry = Some(observation);
                continue;
            }
        }

        let ack_result = sink
            .as_mut()
            .map(|active| active.wait_ack(&observation));
        match ack_result {
            Some(Ok(ack)) => {
                retry_at = None;
                backoff = config.reconnect_initial_delay;
                last_accepted_key = Some(observation_key(&observation));
                update_health(shared, |health| {
                    health.accepted += 1;
                    health.last_accepted = Some(ack);
                    health.link = LinkState::Connected;
                    health.last_error = None;
                });
            }
            Some(Err(Error::ReceiverRejected { code, message })) => {
                if let Some(mut active) = sink.take() {
                    active.close();
                }
                health_sent_on_connection = false;
                update_health(shared, |health| {
                    health.rejected += 1;
                    health.link = LinkState::Disconnected;
                    health.last_error = Some(format!("receiver rejected ({code}): {message}"));
                });
                schedule_retry(
                    shared,
                    &config,
                    observation,
                    retry_origin,
                    false,
                    &mut retry,
                    &mut retry_at,
                    &mut backoff,
                );
            }
            Some(Err(error)) => {
                if let Some(mut active) = sink.take() {
                    active.close();
                }
                health_sent_on_connection = false;
                update_health(shared, |health| {
                    health.link = LinkState::Disconnected;
                    health.last_error = Some(error.to_string());
                });
                schedule_retry(
                    shared,
                    &config,
                    observation,
                    retry_origin,
                    last_accepted_key == Some(key),
                    &mut retry,
                    &mut retry_at,
                    &mut backoff,
                );
            }
            None => {
                retry = Some(observation);
            }
        }
    }
}

/// Waits for a handoff, a retry deadline, or a health-poll timeout.
fn wait_for_work(
    shared: &Arc<Shared>,
    has_retry: bool,
    retry_at: Option<Instant>,
    max_wait: Option<Duration>,
) -> (Option<Observation>, bool, bool) {
    let mut state = match shared.state.lock() {
        Ok(state) => state,
        Err(_) => return (None, true, false),
    };
    loop {
        if state.closed || state.stopped {
            return (None, true, false);
        }
        if state.pending.is_some() {
            return (state.pending.take(), false, false);
        }

        let now = Instant::now();
        let deadline = retry_at;
        let has_work = has_retry && deadline.is_some();
        if has_work {
            let due = deadline.expect("checked above");
            if now >= due {
                return (None, false, false);
            }
            let wait = std::cmp::min(due - now, max_wait.unwrap_or(due - now));
            let (next, _) = match shared.wake.wait_timeout(state, wait) {
                Ok(result) => result,
                Err(_) => return (None, true, false),
            };
            state = next;
            if max_wait.is_some_and(|limit| now.elapsed() >= limit) {
                return (None, false, true);
            }
            continue;
        }

        if let Some(limit) = max_wait {
            let (next, timeout) = match shared.wake.wait_timeout(state, limit) {
                Ok(result) => result,
                Err(_) => return (None, true, false),
            };
            state = next;
            if timeout.timed_out() && state.pending.is_none() && !has_retry {
                return (None, false, true);
            }
            continue;
        }

        state = match shared.wake.wait(state) {
            Ok(state) => state,
            Err(_) => return (None, true, false),
        };
    }
}
