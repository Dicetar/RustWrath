use std::sync::{Arc, Condvar, Mutex, TryLockError};
use std::thread::{self, JoinHandle};
use std::time::{Duration, Instant};

use crate::client::{LoopbackConfig, LoopbackSink};
use crate::error::{Error, Result};
use crate::observation::Observation;

pub const DEFAULT_RECONNECT_INITIAL_DELAY: Duration = Duration::from_millis(100);
pub const DEFAULT_RECONNECT_MAX_DELAY: Duration = Duration::from_secs(2);
pub const DEFAULT_BUSY_RETRY_DELAY: Duration = Duration::from_millis(1);

/// Configuration for the bounded, background-I/O publisher.
#[derive(Clone, Debug)]
pub struct PublisherConfig {
    loopback: LoopbackConfig,
    reconnect_initial_delay: Duration,
    reconnect_max_delay: Duration,
}

impl PublisherConfig {
    pub fn new(loopback: LoopbackConfig) -> Self {
        Self {
            loopback,
            reconnect_initial_delay: DEFAULT_RECONNECT_INITIAL_DELAY,
            reconnect_max_delay: DEFAULT_RECONNECT_MAX_DELAY,
        }
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
        })
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
    closed: bool,
    stopped: bool,
}

struct Shared {
    state: Mutex<SharedState>,
    wake: Condvar,
}

/// A latest-only bounded sender handoff.
///
/// `publish` performs no socket connection, handshake, read, or write and uses `try_lock` for the
/// handoff slot. One worker owns all blocking socket operations. At most one observation is pending
/// and one is in flight; a newer publication replaces the pending one. The caller owns at most one
/// additional `RetainedObservation` after a busy outcome. This is intentionally a latest-only
/// observation stream, not a reliable event queue.
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
        state.pending = Some(observation.clone());
        state.last_published = Some(observation);
        drop(state);
        self.shared.wake.notify_one();
        Ok(PublishOutcome::HandedOff(receipt))
    }

    /// Retries a caller-retained snapshot without changing its source time or revision.
    pub fn retry(&self, retained: RetainedObservation) -> Result<PublishOutcome> {
        self.publish(retained.into_observation())
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
        }
        self.shared.wake.notify_all();
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

fn run_worker(shared: &Arc<Shared>, config: PublisherConfig) {
    let mut sink: Option<LoopbackSink> = None;
    let mut retry: Option<Observation> = None;
    let mut retry_at: Option<Instant> = None;
    let mut backoff = config.reconnect_initial_delay;

    loop {
        let (pending, closed) = wait_for_work(shared, retry.is_some(), retry_at);
        if closed {
            if let Some(mut active) = sink.take() {
                active.close();
            }
            return;
        }

        let observation = match pending {
            Some(observation) => {
                // A pending observation was published after any local retry, so it supersedes it.
                retry = None;
                observation
            }
            None => match retry.take() {
                Some(observation) => observation,
                None => continue,
            },
        };

        if sink.is_none() {
            match LoopbackSink::connect(config.loopback.clone()) {
                Ok(connected) => sink = Some(connected),
                Err(_) => {
                    retry = Some(observation);
                    retry_at = Some(Instant::now() + backoff);
                    backoff = std::cmp::min(backoff * 2, config.reconnect_max_delay);
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

        let send_result = sink.as_mut().map(|active| active.send(&observation));
        match send_result {
            Some(Ok(_)) => {
                retry_at = None;
                backoff = config.reconnect_initial_delay;
            }
            Some(Err(_)) => {
                if let Some(mut active) = sink.take() {
                    active.close();
                }
                retry = Some(observation);
                retry_at = Some(Instant::now() + backoff);
                backoff = std::cmp::min(backoff * 2, config.reconnect_max_delay);
            }
            None => {
                retry = Some(observation);
            }
        }
    }
}

/// Waits for a handoff or a retry deadline. The mutex is never held during socket I/O.
fn wait_for_work(
    shared: &Arc<Shared>,
    has_retry: bool,
    retry_at: Option<Instant>,
) -> (Option<Observation>, bool) {
    let mut state = match shared.state.lock() {
        Ok(state) => state,
        Err(_) => return (None, true),
    };
    loop {
        if state.closed || state.stopped {
            return (None, true);
        }
        let has_work = has_retry || state.pending.is_some();
        if has_work {
            if let Some(deadline) = retry_at {
                let now = Instant::now();
                if now < deadline {
                    let (next, _) = match shared.wake.wait_timeout(state, deadline - now) {
                        Ok(result) => result,
                        Err(_) => return (None, true),
                    };
                    state = next;
                    continue;
                }
            }
            return (state.pending.take(), false);
        }
        state = match shared.wake.wait(state) {
            Ok(state) => state,
            Err(_) => return (None, true),
        };
    }
}
