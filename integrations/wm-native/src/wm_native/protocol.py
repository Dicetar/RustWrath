"""Wire parsing and encoding for the WM native observation receiver."""

from __future__ import annotations

import hmac
import json
import struct
from typing import Any

from .constants import (
    MAX_FRAME_BYTES,
    MAX_TOKEN_BYTES,
    MIN_TOKEN_BYTES,
    OBSERVATION_SCHEMA_VERSION,
    PROTOCOL_NAME,
    PROTOCOL_VERSION,
)
from .errors import ObservationValidationError, ProtocolError
from .models import Observation, SourceHealth

HELLO_KEYS = frozenset(
    {
        "type",
        "protocol",
        "protocol_version",
        "schema_version",
        "token",
    }
)
OBSERVATION_ENVELOPE_KEYS = frozenset({"type", "observation"})
HEALTH_ENVELOPE_KEYS = frozenset({"type", "health"})

_MAX_ERROR_MESSAGE_CHARS = 512


def validate_token(token: str) -> None:
    """Validate an owner-provided token without echoing it."""
    if not isinstance(token, str):
        raise ValueError("token must be a string")
    length = len(token)
    if length < MIN_TOKEN_BYTES or length > MAX_TOKEN_BYTES:
        raise ValueError(
            f"token must be between {MIN_TOKEN_BYTES} and {MAX_TOKEN_BYTES} characters"
        )
    if any(ord(character) < 33 or ord(character) > 126 for character in token):
        raise ValueError("token must contain printable ASCII only")


def _load_json(payload: bytes, *, name: str) -> Any:
    try:
        return json.loads(payload)
    except (json.JSONDecodeError, UnicodeDecodeError) as error:
        raise ProtocolError("invalid_json", f"{name} is not valid JSON") from error


def _require_object(value: object, *, name: str) -> dict[str, Any]:
    if not isinstance(value, dict):
        raise ProtocolError("invalid_shape", f"{name} must be an object")
    return value


def _require_exact_keys(
    value: dict[str, Any], expected: frozenset[str], *, name: str
) -> None:
    if set(value) != expected:
        raise ProtocolError(
            "invalid_keys", f"{name} must contain exactly {sorted(expected)}"
        )


def parse_hello(payload: bytes, *, expected_token: str) -> None:
    """Validate the one-shot hello frame, including a constant-time token check."""
    value = _require_object(_load_json(payload, name="hello"), name="hello")
    _require_exact_keys(value, HELLO_KEYS, name="hello")

    if value["type"] != "hello":
        raise ProtocolError("invalid_hello", "hello type must be 'hello'")
    if value["protocol"] != PROTOCOL_NAME:
        raise ProtocolError("protocol_mismatch", "unsupported protocol")
    version = value["protocol_version"]
    if (
        isinstance(version, bool)
        or not isinstance(version, int)
        or version != PROTOCOL_VERSION
    ):
        raise ProtocolError("protocol_version_mismatch", "unsupported protocol version")
    if value["schema_version"] != OBSERVATION_SCHEMA_VERSION:
        raise ProtocolError("schema_mismatch", "unsupported observation schema version")

    token = value["token"]
    if not isinstance(token, str):
        raise ProtocolError("unauthorized", "invalid token")
    if not hmac.compare_digest(
        token.encode("utf-8"), expected_token.encode("utf-8")
    ):
        raise ProtocolError("unauthorized", "invalid token")


def parse_observation(payload: bytes) -> Observation:
    """Validate an observation envelope and return its typed observation."""
    value = _require_object(
        _load_json(payload, name="observation frame"), name="observation frame"
    )
    _require_exact_keys(value, OBSERVATION_ENVELOPE_KEYS, name="observation frame")
    if value["type"] != "observation":
        raise ProtocolError(
            "invalid_frame_type", "observation frame type must be 'observation'"
        )
    try:
        return Observation.from_mapping(value["observation"])
    except ObservationValidationError:
        raise
    except (TypeError, ValueError) as error:
        raise ObservationValidationError(
            "invalid_observation", "observation is not valid"
        ) from error


def parse_client_frame(payload: bytes) -> tuple[str, Observation | SourceHealth]:
    """Parse a post-handshake client frame as an observation or world-session health."""
    value = _require_object(
        _load_json(payload, name="client frame"), name="client frame"
    )
    frame_type = value.get("type")
    if frame_type == "observation":
        _require_exact_keys(value, OBSERVATION_ENVELOPE_KEYS, name="observation frame")
        return "observation", Observation.from_mapping(value["observation"])
    if frame_type == "health":
        _require_exact_keys(value, HEALTH_ENVELOPE_KEYS, name="health frame")
        return "health", SourceHealth.from_mapping(value["health"])
    raise ProtocolError(
        "invalid_frame_type", "frame type must be 'observation' or 'health'"
    )


def encode_frame(value: Any) -> bytes:
    """Encode one JSON frame with a 4-byte big-endian length prefix."""
    raw = json.dumps(
        value, ensure_ascii=False, separators=(",", ":"), allow_nan=False
    ).encode("utf-8")
    if not raw:
        raise ProtocolError("empty_frame", "refusing to encode an empty frame")
    if len(raw) > MAX_FRAME_BYTES:
        raise ProtocolError("frame_too_large", "frame exceeds the protocol maximum")
    return struct.pack(">I", len(raw)) + raw


def hello_ack_frame() -> bytes:
    return encode_frame(
        {
            "type": "hello_ack",
            "protocol": PROTOCOL_NAME,
            "protocol_version": PROTOCOL_VERSION,
            "schema_version": OBSERVATION_SCHEMA_VERSION,
            "max_frame_bytes": MAX_FRAME_BYTES,
        }
    )


def ack_frame(
    *, sequence: int, state_revision: int, world_generation: int
) -> bytes:
    return encode_frame(
        {
            "type": "ack",
            "sequence": int(sequence),
            "state_revision": int(state_revision),
            "world_generation": int(world_generation),
        }
    )


def health_ack_frame(
    *, session_epoch: int, world_generation: int, player_guid: int
) -> bytes:
    return encode_frame(
        {
            "type": "health_ack",
            "session_epoch": int(session_epoch),
            "world_generation": int(world_generation),
            "player_guid": int(player_guid),
        }
    )


def error_frame(code: str, message: str) -> bytes:
    """Encode a bounded error frame. Never place credentials in `message`."""
    safe_code = code[:64]
    safe_message = message[:_MAX_ERROR_MESSAGE_CHARS]
    return encode_frame({"type": "error", "code": safe_code, "message": safe_message})
