use std::io;
use std::net::{Shutdown, SocketAddr, TcpStream};
use std::time::Duration;

use crate::auth::AuthToken;
use crate::error::{Error, Result};
use crate::action::{ActionRequest, ActionResult};
use crate::frame::{
    ActionResultFrame, HealthFrame, HelloFrame, MAX_FRAME_BYTES, ObservationFrame, PROTOCOL_NAME,
    PROTOCOL_VERSION, ServerFrame, encode, read_frame, write_frame,
};
use crate::health::SourceHealth;
use crate::observation::{OBSERVATION_SCHEMA_VERSION, Observation};

/// Default loopback port used by the Python receiver.
pub const DEFAULT_PORT: u16 = 48_765;

const DEFAULT_CONNECT_TIMEOUT: Duration = Duration::from_secs(2);
const DEFAULT_READ_TIMEOUT: Duration = Duration::from_secs(2);
const DEFAULT_WRITE_TIMEOUT: Duration = Duration::from_millis(250);

/// Configuration for the one-shot, loopback-only sink.
#[derive(Clone, Debug)]
pub struct LoopbackConfig {
    addr: SocketAddr,
    token: AuthToken,
    connect_timeout: Duration,
    read_timeout: Duration,
    write_timeout: Duration,
}

impl LoopbackConfig {
    pub fn new(addr: SocketAddr, token: AuthToken) -> Result<Self> {
        Self::with_timeouts(
            addr,
            token,
            DEFAULT_CONNECT_TIMEOUT,
            DEFAULT_READ_TIMEOUT,
            DEFAULT_WRITE_TIMEOUT,
        )
    }

    pub fn with_timeouts(
        addr: SocketAddr,
        token: AuthToken,
        connect_timeout: Duration,
        read_timeout: Duration,
        write_timeout: Duration,
    ) -> Result<Self> {
        if !addr.ip().is_loopback() {
            return Err(Error::Config(
                "WM native transport must use a loopback address",
            ));
        }
        if addr.port() == 0 {
            return Err(Error::Config("WM native transport requires a port"));
        }
        if [connect_timeout, read_timeout, write_timeout]
            .iter()
            .any(|timeout| timeout.is_zero())
        {
            return Err(Error::Config("WM native timeouts must be non-zero"));
        }
        Ok(Self {
            addr,
            token,
            connect_timeout,
            read_timeout,
            write_timeout,
        })
    }

    pub fn addr(&self) -> SocketAddr {
        self.addr
    }
}

/// Result of one successful synchronous write.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct SendReceipt {
    pub sequence: u64,
    pub json_bytes: usize,
    pub observed_at_unix_ms: i64,
}

/// Receiver acceptance of one written observation.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct AckReceipt {
    pub sequence: u64,
    pub state_revision: u64,
    pub world_generation: u64,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct HealthAckReceipt {
    pub session_epoch: u64,
    pub world_generation: u64,
    pub player_guid: u64,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PeerState {
    Alive,
    Closed,
    UnexpectedData,
}

/// A single, already-connected observation sink.
///
/// Construction performs the authenticated `hello` handshake and can block up to the configured
/// connect/read timeouts. Later calls to [`Self::send`] are synchronous and can block up to the
/// configured write timeout; they do not queue, retry, or reconnect. Game-session code should use
/// `ObservationPublisher` instead. A write failure marks the sink broken and every later send
/// returns [`Error::Disconnected`]; the caller must deliberately replace it.
pub struct LoopbackSink {
    stream: TcpStream,
    addr: SocketAddr,
    max_frame_bytes: usize,
    last: Option<Observation>,
    broken: bool,
    /// Intents the receiver delivered before an expected ACK; drained by the publisher worker.
    actions: std::collections::VecDeque<ActionRequest>,
    /// Admission-level cancellations, newest after older intents.
    cancels: std::collections::VecDeque<String>,
}

impl LoopbackSink {
    /// Connects to the receiver and verifies its authenticated hello acknowledgement.
    pub fn connect(config: LoopbackConfig) -> Result<Self> {
        let mut stream = TcpStream::connect_timeout(&config.addr, config.connect_timeout)
            .map_err(handshake_io)?;
        stream.set_nodelay(true)?;
        stream.set_read_timeout(Some(config.read_timeout))?;
        stream.set_write_timeout(Some(config.write_timeout))?;

        let hello = HelloFrame {
            kind: "hello",
            protocol: PROTOCOL_NAME,
            protocol_version: PROTOCOL_VERSION,
            schema_version: OBSERVATION_SCHEMA_VERSION,
            token: config.token.expose(),
        };
        let hello_bytes = encode(&hello)?;
        write_frame(&mut stream, &hello_bytes).map_err(handshake_frame)?;

        let ack_bytes = read_frame(&mut stream).map_err(handshake_frame)?;
        let ack: ServerFrame = serde_json::from_slice(&ack_bytes)?;
        let max_frame_bytes = match ack {
            ServerFrame::Error { code, message } => {
                return Err(Error::HandshakeRejected { code, message });
            }
            ServerFrame::Ack { .. } => {
                return Err(Error::UnexpectedServerFrame("ack before hello_ack"));
            }
            ServerFrame::HealthAck { .. } => {
                return Err(Error::UnexpectedServerFrame("health_ack before hello_ack"));
            }
            ServerFrame::ActionRequest { .. } | ServerFrame::ActionCancel { .. } => {
                return Err(Error::UnexpectedServerFrame("action frame before hello_ack"));
            }
            ServerFrame::HelloAck {
                protocol,
                protocol_version,
                schema_version,
                max_frame_bytes,
            } => {
                if protocol.as_str() != PROTOCOL_NAME {
                    return Err(Error::ProtocolMismatch {
                        field: "protocol",
                        expected: PROTOCOL_NAME.to_owned(),
                        got: protocol,
                    });
                }
                if protocol_version != PROTOCOL_VERSION {
                    return Err(Error::ProtocolMismatch {
                        field: "protocol_version",
                        expected: PROTOCOL_VERSION.to_string(),
                        got: protocol_version.to_string(),
                    });
                }
                if schema_version.as_str() != OBSERVATION_SCHEMA_VERSION {
                    return Err(Error::ProtocolMismatch {
                        field: "schema_version",
                        expected: OBSERVATION_SCHEMA_VERSION.to_owned(),
                        got: schema_version,
                    });
                }
                let max_frame_bytes = max_frame_bytes as usize;
                if max_frame_bytes == 0 || max_frame_bytes > MAX_FRAME_BYTES {
                    return Err(Error::ProtocolMismatch {
                        field: "max_frame_bytes",
                        expected: format!("1..={MAX_FRAME_BYTES}"),
                        got: max_frame_bytes.to_string(),
                    });
                }
                max_frame_bytes
            }
        };

        stream.set_read_timeout(Some(config.read_timeout))?;

        Ok(Self {
            stream,
            addr: config.addr,
            max_frame_bytes,
            last: None,
            broken: false,
            actions: std::collections::VecDeque::new(),
            cancels: std::collections::VecDeque::new(),
        })
    }

    pub fn addr(&self) -> SocketAddr {
        self.addr
    }

    pub fn is_broken(&self) -> bool {
        self.broken
    }

    /// Sends exactly one observation without queueing.
    ///
    /// The observation is validated before any bytes are written. A duplicate/old sequence, a
    /// cross-player value, a session regression, non-monotonic uptime/time/revision, or an
    /// unchanged state with a new source timestamp is rejected locally. A new session epoch is
    /// allowed to reset ordering.
    pub fn send(&mut self, observation: &Observation) -> Result<SendReceipt> {
        if self.broken {
            return Err(Error::Disconnected);
        }
        observation.validate()?;
        self.check_scope(observation)?;

        let frame = ObservationFrame {
            kind: "observation",
            observation,
        };
        let bytes = encode(&frame)?;
        if bytes.len() > self.max_frame_bytes {
            return Err(Error::FrameTooLarge {
                len: bytes.len(),
                max: self.max_frame_bytes,
            });
        }

        if let Err(error) = write_frame(&mut self.stream, &bytes) {
            self.broken = true;
            return Err(match error {
                Error::Io(error)
                    if matches!(
                        error.kind(),
                        io::ErrorKind::WouldBlock | io::ErrorKind::TimedOut
                    ) =>
                {
                    Error::Backpressure
                }
                other => other,
            });
        }

        self.last = Some(observation.clone());

        Ok(SendReceipt {
            sequence: observation.sequence,
            json_bytes: bytes.len(),
            observed_at_unix_ms: observation.observed_at_unix_ms,
        })
    }

    /// Sends one observation and requires the receiver's matching acceptance ACK.
    pub fn send_and_ack(&mut self, observation: &Observation) -> Result<AckReceipt> {
        self.send(observation)?;
        self.wait_ack(observation)
    }

    /// Sends world-session health and requires the receiver's scope acknowledgement.
    pub fn send_health(&mut self, health: &SourceHealth) -> Result<HealthAckReceipt> {
        health.validate()?;
        let frame = HealthFrame {
            kind: "health",
            health,
        };
        let bytes = encode(&frame)?;
        if bytes.len() > self.max_frame_bytes {
            return Err(Error::FrameTooLarge {
                len: bytes.len(),
                max: self.max_frame_bytes,
            });
        }
        if let Err(error) = write_frame(&mut self.stream, &bytes) {
            self.broken = true;
            return Err(error);
        }
        loop {
            let frame = self.read_server_frame()?;
            match frame {
                ServerFrame::HealthAck {
                    session_epoch,
                    world_generation,
                    player_guid,
                } if session_epoch == health.session_epoch
                    && world_generation == health.world_generation
                    && player_guid == health.player_guid =>
                {
                    return Ok(HealthAckReceipt {
                        session_epoch,
                        world_generation,
                        player_guid,
                    });
                }
                ServerFrame::HealthAck {
                    session_epoch,
                    world_generation,
                    player_guid,
                } => {
                    self.broken = true;
                    return Err(Error::HealthAckMismatch {
                        session_epoch,
                        world_generation,
                        player_guid,
                    });
                }
                ServerFrame::ActionRequest { request } => self.enqueue_action(request)?,
                ServerFrame::ActionCancel { request_id } => self.cancels.push_back(request_id),
                ServerFrame::Error { code, message } => {
                    self.broken = true;
                    return Err(Error::ReceiverRejected { code, message });
                }
                ServerFrame::Ack { .. } => {
                    self.broken = true;
                    return Err(Error::UnexpectedServerFrame("ack for health"));
                }
                ServerFrame::HelloAck { .. } => {
                    self.broken = true;
                    return Err(Error::UnexpectedServerFrame("hello_ack for health"));
                }
            }
        }
    }

    /// Waits for the ACK matching the observation just written.
    pub fn wait_ack(&mut self, observation: &Observation) -> Result<AckReceipt> {
        loop {
            let frame = self.read_server_frame()?;
            match frame {
                ServerFrame::Ack {
                    sequence,
                    state_revision,
                    world_generation,
                } => {
                    if sequence == observation.sequence
                        && state_revision == observation.state_revision
                        && world_generation == observation.world_generation
                    {
                        return Ok(AckReceipt {
                            sequence,
                            state_revision,
                            world_generation,
                        });
                    }
                    self.broken = true;
                    return Err(Error::AckMismatch {
                        sequence,
                        state_revision,
                        world_generation,
                    });
                }
                ServerFrame::ActionRequest { request } => self.enqueue_action(request)?,
                ServerFrame::ActionCancel { request_id } => self.cancels.push_back(request_id),
                ServerFrame::Error { code, message } => {
                    self.broken = true;
                    return Err(Error::ReceiverRejected { code, message });
                }
                ServerFrame::HealthAck { .. } => {
                    self.broken = true;
                    return Err(Error::UnexpectedServerFrame("health_ack for observation"));
                }
                ServerFrame::HelloAck { .. } => {
                    self.broken = true;
                    return Err(Error::UnexpectedServerFrame("hello_ack"));
                }
            }
        }
    }

    /// Nonblocking-ish peer liveness probe used between publications.
    ///
    /// A tiny temporary read timeout keeps this bounded on the publisher worker; `Alive` means no
    /// EOF was observed, not that a future write will succeed.
    pub fn poll_peer(&mut self) -> Result<PeerState> {
        let previous = self.stream.read_timeout()?;
        self.stream
            .set_read_timeout(Some(Duration::from_millis(25)))?;
        let mut byte = [0_u8; 1];
        let result = self.stream.peek(&mut byte);
        self.stream.set_read_timeout(previous)?;
        match result {
            Ok(0) => {
                self.broken = true;
                Ok(PeerState::Closed)
            }
            Ok(_) => match self.read_server_frame()? {
                ServerFrame::ActionRequest { request } => {
                    self.enqueue_action(request)?;
                    Ok(PeerState::Alive)
                }
                ServerFrame::ActionCancel { request_id } => {
                    self.cancels.push_back(request_id);
                    Ok(PeerState::Alive)
                }
                _ => {
                    self.broken = true;
                    Ok(PeerState::UnexpectedData)
                }
            },
            Err(error)
                if matches!(
                    error.kind(),
                    io::ErrorKind::WouldBlock | io::ErrorKind::TimedOut
                ) =>
            {
                Ok(PeerState::Alive)
            }
            Err(error) => {
                self.broken = true;
                Err(Error::Io(error))
            }
        }
    }

    /// Sends one authoritative action result. No acknowledgement is expected: the WM broker owns
    /// result admission, and a lost result is resolved by its deadline as `Unknown`.
    pub fn send_action_result(&mut self, result: &ActionResult) -> Result<()> {
        if self.broken {
            return Err(Error::Disconnected);
        }
        result.validate()?;
        let frame = ActionResultFrame {
            kind: "action_result",
            result,
        };
        let bytes = encode(&frame)?;
        if bytes.len() > self.max_frame_bytes {
            return Err(Error::FrameTooLarge {
                len: bytes.len(),
                max: self.max_frame_bytes,
            });
        }
        write_frame(&mut self.stream, &bytes)
    }

    /// Drains intents and cancellations delivered before the last expected ACK or idle poll.
    pub fn take_actions(&mut self) -> (Vec<ActionRequest>, Vec<String>) {
        let actions = self.actions.drain(..).collect();
        let cancels = self.cancels.drain(..).collect();
        (actions, cancels)
    }

    fn enqueue_action(&mut self, request: ActionRequest) -> Result<()> {
        request.validate()?;
        self.actions.push_back(request);
        Ok(())
    }

    /// Reads one server frame, mapping an idle read timeout on a half-read frame to a broken link.
    fn read_server_frame(&mut self) -> Result<ServerFrame> {
        let bytes = match read_frame(&mut self.stream) {
            Ok(bytes) => bytes,
            Err(Error::Io(error))
                if matches!(
                    error.kind(),
                    io::ErrorKind::WouldBlock | io::ErrorKind::TimedOut
                ) =>
            {
                self.broken = true;
                return Err(Error::AckTimeout);
            }
            Err(error) => {
                self.broken = true;
                return Err(error);
            }
        };
        serde_json::from_slice(&bytes).map_err(|error| {
            self.broken = true;
            Error::Json(error)
        })
    }

    /// Shuts the connection down. The sink must not be reused afterwards.
    pub fn close(&mut self) {
        let _ = self.stream.shutdown(Shutdown::Both);
        self.broken = true;
    }

    fn check_scope(&self, observation: &Observation) -> Result<()> {
        if let Some(previous) = self.last.as_ref() {
            observation.validate_after(previous)?;
        }
        Ok(())
    }
}

fn handshake_io(error: io::Error) -> Error {
    if matches!(
        error.kind(),
        io::ErrorKind::WouldBlock | io::ErrorKind::TimedOut
    ) {
        Error::HandshakeTimeout
    } else {
        Error::Io(error)
    }
}

fn handshake_frame(error: Error) -> Error {
    match error {
        Error::Io(error)
            if matches!(
                error.kind(),
                io::ErrorKind::WouldBlock | io::ErrorKind::TimedOut
            ) =>
        {
            Error::HandshakeTimeout
        }
        other => other,
    }
}
