use std::{fmt, io};

/// Errors from the native observation seam.
///
/// No variant contains the authentication token, an observation payload, or a raw credential.
#[derive(Debug)]
pub enum Error {
    /// The local configuration is unsafe or incomplete.
    Config(&'static str),
    /// An observation violates the versioned wire contract.
    InvalidObservation(&'static str),
    /// A zero-length frame was read or attempted.
    EmptyFrame,
    /// A frame exceeds the negotiated or protocol maximum.
    FrameTooLarge { len: usize, max: usize },
    /// A frame was not valid JSON for its declared shape.
    Json(serde_json::Error),
    /// Socket I/O failed.
    Io(io::Error),
    /// The receiver did not answer the hello frame in time.
    HandshakeTimeout,
    /// The receiver refused the hello frame.
    HandshakeRejected { code: String, message: String },
    /// The receiver accepted hello but disagreed on protocol details.
    ProtocolMismatch {
        field: &'static str,
        expected: String,
        got: String,
    },
    /// A write timed out because the receiver cannot keep up.
    Backpressure,
    /// The connection is closed or unusable.
    Disconnected,
    /// The observation belongs to a different player than this sink.
    PlayerChanged { last: u64, got: u64 },
    /// The observation belongs to an older session than this sink.
    StaleSession { last: u64, got: u64 },
    /// The observation repeats the last accepted sequence.
    DuplicateSequence { last: u64 },
    /// The observation sequence went backwards within one session.
    SequenceNotIncreasing { last: u64, got: u64 },
    /// Client uptime went backwards without a new session.
    UptimeNotIncreasing { last: u64, got: u64 },
    /// Source observation time went backwards without a new session.
    ObservationTimeRegression { last: i64, got: i64 },
    /// The source state revision went backwards without a new session.
    StateRevisionRegression { last: u64, got: u64 },
    /// One state revision was used for two different exported states.
    StateRevisionConflict { revision: u64 },
    /// Unchanged state was assigned a new source timestamp to appear fresh.
    UnchangedStateRefresh { last: i64, got: i64 },
}

pub type Result<T> = std::result::Result<T, Error>;

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Config(message) => write!(f, "invalid configuration: {message}"),
            Self::InvalidObservation(message) => write!(f, "invalid observation: {message}"),
            Self::EmptyFrame => f.write_str("empty frame"),
            Self::FrameTooLarge { len, max } => {
                write!(f, "frame is {len} bytes; maximum is {max}")
            }
            Self::Json(error) => write!(f, "invalid JSON frame: {error}"),
            Self::Io(error) => write!(f, "transport I/O error: {error}"),
            Self::HandshakeTimeout => f.write_str("timed out during WM native handshake"),
            Self::HandshakeRejected { code, message } => {
                write!(f, "receiver rejected handshake ({code}): {message}")
            }
            Self::ProtocolMismatch {
                field,
                expected,
                got,
            } => write!(f, "protocol mismatch for {field}: expected {expected}, got {got}"),
            Self::Backpressure => f.write_str("receiver is not accepting data (write timeout)"),
            Self::Disconnected => f.write_str("receiver connection is closed"),
            Self::PlayerChanged { last, got } => {
                write!(f, "observation player changed: last {last}, got {got}")
            }
            Self::StaleSession { last, got } => {
                write!(f, "observation session went backwards: last {last}, got {got}")
            }
            Self::DuplicateSequence { last } => {
                write!(f, "observation sequence {last} was already accepted")
            }
            Self::SequenceNotIncreasing { last, got } => {
                write!(f, "observation sequence is not increasing: last {last}, got {got}")
            }
            Self::UptimeNotIncreasing { last, got } => {
                write!(f, "client uptime is not increasing: last {last}, got {got}")
            }
            Self::ObservationTimeRegression { last, got } => {
                write!(f, "source observation time went backwards: last {last}, got {got}")
            }
            Self::StateRevisionRegression { last, got } => {
                write!(f, "state revision went backwards: last {last}, got {got}")
            }
            Self::StateRevisionConflict { revision } => {
                write!(f, "state revision {revision} describes conflicting states")
            }
            Self::UnchangedStateRefresh { last, got } => {
                write!(f, "unchanged state changed source time: last {last}, got {got}")
            }
        }
    }
}

impl std::error::Error for Error {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Json(error) => Some(error),
            Self::Io(error) => Some(error),
            _ => None,
        }
    }
}

impl From<io::Error> for Error {
    fn from(error: io::Error) -> Self {
        Self::Io(error)
    }
}

impl From<serde_json::Error> for Error {
    fn from(error: serde_json::Error) -> Self {
        Self::Json(error)
    }
}
