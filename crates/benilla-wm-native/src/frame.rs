use std::io::{Read, Write};
use std::net::TcpStream;

use serde::{Deserialize, Serialize};

use crate::error::{Error, Result};
use crate::health::SourceHealth;
use crate::observation::{OBSERVATION_SCHEMA_VERSION, Observation};

/// Wire protocol name.
pub const PROTOCOL_NAME: &str = "benilla.wm_native";

/// Wire protocol version.
pub const PROTOCOL_VERSION: u32 = 1;

/// Hard frame bound shared by the Rust sender and Python receiver.
pub const MAX_FRAME_BYTES: usize = 64 * 1024;

#[derive(Serialize)]
pub(crate) struct HelloFrame<'a> {
    #[serde(rename = "type")]
    pub kind: &'static str,
    pub protocol: &'static str,
    pub protocol_version: u32,
    pub schema_version: &'static str,
    pub token: &'a str,
}

#[derive(Serialize)]
pub(crate) struct ObservationFrame<'a> {
    #[serde(rename = "type")]
    pub kind: &'static str,
    pub observation: &'a Observation,
}

#[derive(Serialize)]
pub(crate) struct HealthFrame<'a> {
    #[serde(rename = "type")]
    pub kind: &'static str,
    pub health: &'a SourceHealth,
}

#[derive(Debug, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub(crate) enum ServerFrame {
    HelloAck {
        protocol: String,
        protocol_version: u32,
        schema_version: String,
        max_frame_bytes: u32,
    },
    Ack {
        sequence: u64,
        state_revision: u64,
        world_generation: u64,
    },
    HealthAck {
        session_epoch: u64,
        world_generation: u64,
        player_guid: u64,
    },
    Error {
        code: String,
        message: String,
    },
}

pub(crate) fn encode<T: Serialize>(value: &T) -> Result<Vec<u8>> {
    let bytes = serde_json::to_vec(value)?;
    if bytes.is_empty() {
        return Err(Error::EmptyFrame);
    }
    if bytes.len() > MAX_FRAME_BYTES {
        return Err(Error::FrameTooLarge {
            len: bytes.len(),
            max: MAX_FRAME_BYTES,
        });
    }
    Ok(bytes)
}

pub(crate) fn write_frame(stream: &mut TcpStream, payload: &[u8]) -> Result<()> {
    if payload.is_empty() {
        return Err(Error::EmptyFrame);
    }
    if payload.len() > MAX_FRAME_BYTES {
        return Err(Error::FrameTooLarge {
            len: payload.len(),
            max: MAX_FRAME_BYTES,
        });
    }

    let length = payload.len() as u32;
    stream.write_all(&length.to_be_bytes())?;
    stream.write_all(payload)?;
    stream.flush()?;
    Ok(())
}

pub(crate) fn read_frame(stream: &mut TcpStream) -> Result<Vec<u8>> {
    let mut header = [0_u8; 4];
    stream.read_exact(&mut header)?;

    let length = u32::from_be_bytes(header) as usize;
    if length == 0 {
        return Err(Error::EmptyFrame);
    }
    if length > MAX_FRAME_BYTES {
        return Err(Error::FrameTooLarge {
            len: length,
            max: MAX_FRAME_BYTES,
        });
    }

    let mut payload = vec![0_u8; length];
    stream.read_exact(&mut payload)?;
    Ok(payload)
}
