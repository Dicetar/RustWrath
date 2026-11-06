"""Validated observation models for the WM native seam."""

from __future__ import annotations

import math
from dataclasses import dataclass
from typing import Any

from .constants import MAX_VISIBLE_GUIDS, OBSERVATION_SCHEMA_VERSION
from .errors import ObservationValidationError

_U64_MAX = (1 << 64) - 1
_U32_MAX = (1 << 32) - 1
_I64_MAX = (1 << 63) - 1

POSITION_KEYS = frozenset({"x", "y", "z", "orientation"})
TARGET_BASE_KEYS = frozenset({"state"})
TARGET_VALUE_KEYS = frozenset({"state", "guid"})
VISIBILITY_KEYS = frozenset({"complete", "total_guid_count", "guids"})
HEALTH_KEYS = frozenset(
    {
        "session_epoch",
        "world_generation",
        "player_guid",
        "state",
        "last_server_packet_at_unix_ms",
    }
)
SOURCE_SESSION_STATES = frozenset({"in_world", "transferring", "disconnected"})
OBSERVATION_KEYS = frozenset(
    {
        "schema_version",
        "session_epoch",
        "world_generation",
        "player_guid",
        "sequence",
        "state_revision",
        "observed_at_unix_ms",
        "exported_at_unix_ms",
        "client_uptime_ms",
        "map_id",
        "zone_id",
        "area_id",
        "position_observed_at_unix_ms",
        "position",
        "target",
        "visibility",
    }
)


def _as_int(
    value: object,
    *,
    name: str,
    minimum: int = 0,
    maximum: int = _U64_MAX,
) -> int:
    if isinstance(value, bool) or not isinstance(value, int):
        raise ObservationValidationError("invalid_type", f"{name} must be an integer")
    if value < minimum or value > maximum:
        raise ObservationValidationError("out_of_range", f"{name} is out of range")
    return value


def _as_float(value: object, *, name: str) -> float:
    if isinstance(value, bool) or not isinstance(value, (int, float)):
        raise ObservationValidationError("invalid_type", f"{name} must be a number")
    number = float(value)
    if not math.isfinite(number):
        raise ObservationValidationError("non_finite", f"{name} must be finite")
    return number


def _optional_int(
    value: object,
    *,
    name: str,
    minimum: int = 0,
    maximum: int = _U64_MAX,
) -> int | None:
    if value is None:
        return None
    return _as_int(value, name=name, minimum=minimum, maximum=maximum)


def _require_object(value: object, *, name: str) -> dict[str, Any]:
    if not isinstance(value, dict):
        raise ObservationValidationError("invalid_shape", f"{name} must be an object")
    return value


def _require_exact_keys(
    value: dict[str, Any], expected: frozenset[str], *, name: str
) -> None:
    if set(value) != expected:
        raise ObservationValidationError(
            "invalid_keys", f"{name} must contain exactly {sorted(expected)}"
        )


@dataclass(frozen=True)
class Position:
    x: float
    y: float
    z: float
    orientation: float

    @classmethod
    def from_mapping(cls, raw: object) -> Position:
        value = _require_object(raw, name="position")
        _require_exact_keys(value, POSITION_KEYS, name="position")
        position = cls(
            x=_as_float(value["x"], name="position.x"),
            y=_as_float(value["y"], name="position.y"),
            z=_as_float(value["z"], name="position.z"),
            orientation=_as_float(
                value["orientation"], name="position.orientation"
            ),
        )
        position.validate()
        return position

    def validate(self) -> None:
        _as_float(self.x, name="position.x")
        _as_float(self.y, name="position.y")
        _as_float(self.z, name="position.z")
        _as_float(self.orientation, name="position.orientation")

    def to_payload(self) -> dict[str, float]:
        return {
            "x": self.x,
            "y": self.y,
            "z": self.z,
            "orientation": self.orientation,
        }


@dataclass(frozen=True)
class TargetObservation:
    """Explicit unknown, observed-no-target, or observed-target state."""

    state: str
    guid: int | None = None

    @classmethod
    def unknown(cls) -> TargetObservation:
        return cls("unknown")

    @classmethod
    def none(cls) -> TargetObservation:
        return cls("none")

    @classmethod
    def target(cls, guid: int) -> TargetObservation:
        target = cls("target", guid)
        target.validate()
        return target

    @classmethod
    def from_mapping(cls, raw: object) -> TargetObservation:
        value = _require_object(raw, name="target")
        state = value.get("state")
        if not isinstance(state, str):
            raise ObservationValidationError(
                "invalid_target_state", "target state must be a string"
            )
        if state in {"unknown", "none"}:
            _require_exact_keys(value, TARGET_BASE_KEYS, name="target")
            target = cls(state)
        elif state == "target":
            _require_exact_keys(value, TARGET_VALUE_KEYS, name="target")
            target = cls(
                state,
                _as_int(value["guid"], name="target.guid", minimum=1),
            )
        else:
            raise ObservationValidationError(
                "invalid_target_state", "target state must be unknown, none, or target"
            )
        target.validate()
        return target

    def validate(self) -> None:
        if self.state not in {"unknown", "none", "target"}:
            raise ObservationValidationError(
                "invalid_target_state", "target state must be unknown, none, or target"
            )
        if self.state == "target":
            _as_int(self.guid, name="target.guid", minimum=1)
        elif self.guid is not None:
            raise ObservationValidationError(
                "invalid_target", "unknown/none target states must not carry a guid"
            )

    def to_payload(self) -> dict[str, Any]:
        if self.state == "target":
            return {"state": self.state, "guid": self.guid}
        return {"state": self.state}


@dataclass(frozen=True)
class Visibility:
    """Bounded GUID export with explicit completeness and pre-bound total."""

    complete: bool
    total_guid_count: int
    guids: tuple[int, ...]

    @classmethod
    def from_mapping(cls, raw: object) -> Visibility:
        value = _require_object(raw, name="visibility")
        _require_exact_keys(value, VISIBILITY_KEYS, name="visibility")
        complete = value["complete"]
        if not isinstance(complete, bool):
            raise ObservationValidationError(
                "invalid_type", "visibility.complete must be a boolean"
            )
        raw_guids = value["guids"]
        if not isinstance(raw_guids, list):
            raise ObservationValidationError(
                "invalid_type", "visibility.guids must be an array"
            )
        if len(raw_guids) > MAX_VISIBLE_GUIDS:
            raise ObservationValidationError(
                "too_many_visible_guids",
                f"visibility.guids exceeds {MAX_VISIBLE_GUIDS} entries",
            )
        visibility = cls(
            complete=complete,
            total_guid_count=_as_int(
                value["total_guid_count"], name="visibility.total_guid_count"
            ),
            guids=tuple(
                _as_int(guid, name="visibility.guids[]", minimum=1)
                for guid in raw_guids
            ),
        )
        visibility.validate()
        return visibility

    def validate(self) -> None:
        if not isinstance(self.complete, bool):
            raise ObservationValidationError(
                "invalid_type", "visibility.complete must be a boolean"
            )
        _as_int(self.total_guid_count, name="visibility.total_guid_count")
        if not isinstance(self.guids, tuple):
            raise ObservationValidationError(
                "invalid_type", "visibility.guids must be a tuple"
            )
        if len(self.guids) > MAX_VISIBLE_GUIDS:
            raise ObservationValidationError(
                "too_many_visible_guids",
                f"visibility.guids exceeds {MAX_VISIBLE_GUIDS} entries",
            )
        previous: int | None = None
        for guid in self.guids:
            _as_int(guid, name="visibility.guids[]", minimum=1)
            if previous is not None:
                if previous == guid:
                    raise ObservationValidationError(
                        "duplicate_visible_guid",
                        "visibility.guids contains a duplicate",
                    )
                if previous > guid:
                    raise ObservationValidationError(
                        "unsorted_visible_guids",
                        "visibility.guids must be sorted by full u64 GUID",
                    )
            previous = guid
        if self.total_guid_count < len(self.guids):
            raise ObservationValidationError(
                "invalid_visibility_total",
                "visibility.total_guid_count is smaller than the exported list",
            )
        if self.complete and self.total_guid_count != len(self.guids):
            raise ObservationValidationError(
                "inconsistent_visibility",
                "complete visibility requires total_guid_count == len(guids)",
            )

    def to_payload(self) -> dict[str, Any]:
        return {
            "complete": self.complete,
            "total_guid_count": self.total_guid_count,
            "guids": list(self.guids),
        }


@dataclass(frozen=True)
class SourceHealth:
    session_epoch: int
    world_generation: int
    player_guid: int
    state: str
    last_server_packet_at_unix_ms: int | None

    @classmethod
    def from_mapping(cls, raw: object) -> SourceHealth:
        value = _require_object(raw, name="health")
        _require_exact_keys(value, HEALTH_KEYS, name="health")
        health = cls(
            session_epoch=_as_int(
                value["session_epoch"], name="health.session_epoch", minimum=1
            ),
            world_generation=_as_int(
                value["world_generation"], name="health.world_generation", minimum=1
            ),
            player_guid=_as_int(
                value["player_guid"], name="health.player_guid", minimum=1
            ),
            state=value["state"],
            last_server_packet_at_unix_ms=_optional_int(
                value["last_server_packet_at_unix_ms"],
                name="health.last_server_packet_at_unix_ms",
                maximum=_I64_MAX,
            ),
        )
        health.validate()
        return health

    def validate(self) -> None:
        _as_int(self.session_epoch, name="health.session_epoch", minimum=1)
        _as_int(self.world_generation, name="health.world_generation", minimum=1)
        _as_int(self.player_guid, name="health.player_guid", minimum=1)
        if self.state not in SOURCE_SESSION_STATES:
            raise ObservationValidationError(
                "invalid_session_state", "health.state is not a known session state"
            )
        if self.last_server_packet_at_unix_ms is not None:
            _as_int(
                self.last_server_packet_at_unix_ms,
                name="health.last_server_packet_at_unix_ms",
                maximum=_I64_MAX,
            )

    def same_scope(self, observation: Observation) -> bool:
        return (
            self.session_epoch == observation.session_epoch
            and self.world_generation == observation.world_generation
            and self.player_guid == observation.player_guid
        )

    def to_payload(self) -> dict[str, Any]:
        return {
            "session_epoch": self.session_epoch,
            "world_generation": self.world_generation,
            "player_guid": self.player_guid,
            "state": self.state,
            "last_server_packet_at_unix_ms": self.last_server_packet_at_unix_ms,
        }


@dataclass(frozen=True)
class Observation:
    schema_version: str
    session_epoch: int
    world_generation: int
    player_guid: int
    sequence: int
    state_revision: int
    observed_at_unix_ms: int
    exported_at_unix_ms: int
    client_uptime_ms: int
    map_id: int
    zone_id: int | None
    area_id: int | None
    position_observed_at_unix_ms: int
    position: Position
    target: TargetObservation
    visibility: Visibility

    @classmethod
    def from_mapping(cls, raw: object) -> Observation:
        value = _require_object(raw, name="observation")
        _require_exact_keys(value, OBSERVATION_KEYS, name="observation")

        schema_version = value["schema_version"]
        if not isinstance(schema_version, str):
            raise ObservationValidationError(
                "invalid_type", "schema_version must be a string"
            )

        observation = cls(
            schema_version=schema_version,
            session_epoch=_as_int(
                value["session_epoch"], name="session_epoch", minimum=1
            ),
            world_generation=_as_int(
                value["world_generation"], name="world_generation", minimum=1
            ),
            player_guid=_as_int(value["player_guid"], name="player_guid", minimum=1),
            sequence=_as_int(value["sequence"], name="sequence"),
            state_revision=_as_int(value["state_revision"], name="state_revision"),
            observed_at_unix_ms=_as_int(
                value["observed_at_unix_ms"],
                name="observed_at_unix_ms",
                maximum=_I64_MAX,
            ),
            exported_at_unix_ms=_as_int(
                value["exported_at_unix_ms"],
                name="exported_at_unix_ms",
                maximum=_I64_MAX,
            ),
            client_uptime_ms=_as_int(
                value["client_uptime_ms"], name="client_uptime_ms"
            ),
            map_id=_as_int(value["map_id"], name="map_id", maximum=_U32_MAX),
            zone_id=_optional_int(value["zone_id"], name="zone_id", maximum=_U32_MAX),
            area_id=_optional_int(value["area_id"], name="area_id", maximum=_U32_MAX),
            position_observed_at_unix_ms=_as_int(
                value["position_observed_at_unix_ms"],
                name="position_observed_at_unix_ms",
                maximum=_I64_MAX,
            ),
            position=Position.from_mapping(value["position"]),
            target=TargetObservation.from_mapping(value["target"]),
            visibility=Visibility.from_mapping(value["visibility"]),
        )
        observation.validate()
        return observation

    def validate(self) -> None:
        if self.schema_version != OBSERVATION_SCHEMA_VERSION:
            raise ObservationValidationError(
                "unsupported_schema", "unsupported observation schema_version"
            )
        _as_int(self.session_epoch, name="session_epoch", minimum=1)
        _as_int(self.world_generation, name="world_generation", minimum=1)
        _as_int(self.player_guid, name="player_guid", minimum=1)
        _as_int(self.sequence, name="sequence")
        _as_int(self.state_revision, name="state_revision")
        _as_int(
            self.observed_at_unix_ms,
            name="observed_at_unix_ms",
            maximum=_I64_MAX,
        )
        _as_int(
            self.exported_at_unix_ms,
            name="exported_at_unix_ms",
            maximum=_I64_MAX,
        )
        if self.exported_at_unix_ms < self.observed_at_unix_ms:
            raise ObservationValidationError(
                "export_before_observation",
                "exported_at_unix_ms must not precede observed_at_unix_ms",
            )
        _as_int(
            self.position_observed_at_unix_ms,
            name="position_observed_at_unix_ms",
            maximum=_I64_MAX,
        )
        if self.position_observed_at_unix_ms > self.observed_at_unix_ms:
            raise ObservationValidationError(
                "position_after_observation",
                "position_observed_at_unix_ms must not follow observed_at_unix_ms",
            )
        _as_int(self.client_uptime_ms, name="client_uptime_ms")
        _as_int(self.map_id, name="map_id", maximum=_U32_MAX)
        if self.zone_id is not None:
            _as_int(self.zone_id, name="zone_id", maximum=_U32_MAX)
        if self.area_id is not None:
            _as_int(self.area_id, name="area_id", maximum=_U32_MAX)
        if not isinstance(self.position, Position):
            raise ObservationValidationError(
                "invalid_type", "position must be a Position"
            )
        self.position.validate()
        if not isinstance(self.target, TargetObservation):
            raise ObservationValidationError(
                "invalid_type", "target must be a TargetObservation"
            )
        self.target.validate()
        if not isinstance(self.visibility, Visibility):
            raise ObservationValidationError(
                "invalid_type", "visibility must be a Visibility"
            )
        self.visibility.validate()
        if self.player_guid in self.visibility.guids:
            raise ObservationValidationError(
                "player_in_visibility",
                "visibility must exclude the player's own guid",
            )

    def same_state(self, other: Observation) -> bool:
        """Compare source state, excluding export ordering and timestamps."""
        return (
            self.schema_version == other.schema_version
            and self.session_epoch == other.session_epoch
            and self.world_generation == other.world_generation
            and self.player_guid == other.player_guid
            and self.map_id == other.map_id
            and self.zone_id == other.zone_id
            and self.area_id == other.area_id
            and self.position_observed_at_unix_ms
            == other.position_observed_at_unix_ms
            and self.position == other.position
            and self.target == other.target
            and self.visibility == other.visibility
        )

    def to_payload(self) -> dict[str, Any]:
        return {
            "schema_version": self.schema_version,
            "session_epoch": self.session_epoch,
            "world_generation": self.world_generation,
            "player_guid": self.player_guid,
            "sequence": self.sequence,
            "state_revision": self.state_revision,
            "observed_at_unix_ms": self.observed_at_unix_ms,
            "exported_at_unix_ms": self.exported_at_unix_ms,
            "client_uptime_ms": self.client_uptime_ms,
            "map_id": self.map_id,
            "zone_id": self.zone_id,
            "area_id": self.area_id,
            "position_observed_at_unix_ms": self.position_observed_at_unix_ms,
            "position": self.position.to_payload(),
            "target": self.target.to_payload(),
            "visibility": self.visibility.to_payload(),
        }
