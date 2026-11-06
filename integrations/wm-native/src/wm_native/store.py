"""Thread-safe latest-only observation store.

The store is deliberately not a scheduler, director, command bus, or history log. It keeps exactly
one accepted observation for one player scope and lets readers ask for it only while its source
observation time is fresh. Export time and repeated publication never renew freshness.
"""

from __future__ import annotations

import threading
import time
from dataclasses import dataclass
from typing import NoReturn

from .constants import (
    DEFAULT_HEALTH_MAX_AGE_MS,
    DEFAULT_MAX_FUTURE_SKEW_MS,
    DEFAULT_MAX_OBSERVATION_AGE_MS,
)
from .errors import ObservationRejected
from .models import Observation, SourceHealth


@dataclass(frozen=True)
class StoredObservation:
    observation: Observation
    received_at_unix_ms: int
    received_mono_ns: int
    wall_age_ms: int
    position_wall_age_ms: int
    age_ms: int
    position_age_ms: int
    clock_skew_ms: int


@dataclass(frozen=True)
class SnapshotStatus:
    state: str  # unknown | valid | stale | disconnected
    observation: StoredObservation | None
    aggregate_age_ms: int | None
    position_age_ms: int | None
    world_generation: int | None


@dataclass(frozen=True)
class StoreStats:
    accepted: int
    rejected: int
    superseded: int
    invalidations: int
    active_player_guid: int | None
    active_session_epoch: int | None
    active_world_generation: int | None
    last_sequence: int | None
    latest_age_ms: int | None
    latest_position_age_ms: int | None
    source_session_state: str | None
    health_age_ms: int | None
    last_invalidation_reason: str | None


class ObservationStore:
    """Latest-only, one-player, freshness-checked observation store."""

    def __init__(
        self,
        *,
        max_age_ms: int = DEFAULT_MAX_OBSERVATION_AGE_MS,
        max_future_skew_ms: int = DEFAULT_MAX_FUTURE_SKEW_MS,
        allowed_player_guid: int | None = None,
    ) -> None:
        if isinstance(max_age_ms, bool) or not isinstance(max_age_ms, int):
            raise TypeError("max_age_ms must be an integer")
        if max_age_ms <= 0:
            raise ValueError("max_age_ms must be positive")
        if isinstance(max_future_skew_ms, bool) or not isinstance(
            max_future_skew_ms, int
        ):
            raise TypeError("max_future_skew_ms must be an integer")
        if max_future_skew_ms < 0:
            raise ValueError("max_future_skew_ms must not be negative")
        if allowed_player_guid is not None:
            if isinstance(allowed_player_guid, bool) or not isinstance(
                allowed_player_guid, int
            ):
                raise TypeError("allowed_player_guid must be an integer or None")
            if allowed_player_guid <= 0:
                raise ValueError("allowed_player_guid must be positive")

        self._max_age_ms = max_age_ms
        self._max_future_skew_ms = max_future_skew_ms
        self._allowed_player_guid = allowed_player_guid
        self._lock = threading.Lock()
        self._latest: StoredObservation | None = None
        self._player_guid: int | None = None
        self._session_epoch: int | None = None
        self._world_generation: int | None = None
        self._last_sequence: int | None = None
        self._last_uptime_ms: int | None = None
        self._accepted = 0
        self._rejected = 0
        self._superseded = 0
        self._invalidations = 0
        self._last_invalidation_reason: str | None = None
        self._ever_accepted = False
        self._source_health: SourceHealth | None = None
        self._health_mono_ns: int | None = None

    @property
    def max_age_ms(self) -> int:
        return self._max_age_ms

    @property
    def max_future_skew_ms(self) -> int:
        return self._max_future_skew_ms

    @property
    def allowed_player_guid(self) -> int | None:
        return self._allowed_player_guid

    @staticmethod
    def _now_ms() -> int:
        return time.time_ns() // 1_000_000

    @staticmethod
    def _now_mono_ns() -> int:
        return time.monotonic_ns()

    def update_health(
        self, health: SourceHealth, *, now_mono_ns: int | None = None
    ) -> None:
        health.validate()
        mono = self._now_mono_ns() if now_mono_ns is None else now_mono_ns
        with self._lock:
            previous_scope = (
                self._latest.observation if self._latest is not None else None
            )
            self._source_health = health
            self._health_mono_ns = mono
            if health.state == "disconnected":
                self._latest = None
                self._player_guid = None
                self._session_epoch = None
                self._world_generation = None
                self._last_sequence = None
                self._last_uptime_ms = None
                self._last_invalidation_reason = "source_disconnected"
            elif previous_scope is not None and not health.same_scope(previous_scope):
                self._latest = None
                self._player_guid = None
                self._session_epoch = None
                self._world_generation = None
                self._last_sequence = None
                self._last_uptime_ms = None
                self._last_invalidation_reason = "source_scope_changed"

    def begin_connection(self) -> None:
        """Clear prior state and numeric epoch ordering for a new authenticated connection.

        Process-local session epochs need not increase across process restarts. A new authenticated
        connection is therefore a new ordering scope; within that connection epochs must still
        strictly increase when the world session/reducer generation changes.
        """
        with self._lock:
            self._invalidate_locked("connection_start")

    def invalidate(self, reason: str = "disconnect") -> None:
        """Immediately hide the latest observation from WM readers."""
        if not isinstance(reason, str) or not reason or len(reason) > 64:
            raise ValueError("invalidation reason must be 1..64 characters")
        with self._lock:
            self._invalidate_locked(reason)

    def accept(
        self, observation: Observation, *, now_ms: int | None = None
    ) -> StoredObservation:
        """Accept one in-scope, fresh, ordered observation or raise a rejection."""
        observation.validate()
        received_at_ms = self._now_ms() if now_ms is None else now_ms
        if isinstance(received_at_ms, bool) or not isinstance(received_at_ms, int):
            raise TypeError("now_ms must be an integer")

        with self._lock:
            if (
                self._allowed_player_guid is not None
                and observation.player_guid != self._allowed_player_guid
            ):
                self._reject("player_scope_mismatch", "observation player is out of scope")
            if (
                self._player_guid is not None
                and observation.player_guid != self._player_guid
            ):
                self._reject("player_scope_mismatch", "observation player is out of scope")
            if self._source_health is not None:
                if not self._source_health.same_scope(observation):
                    self._reject(
                        "source_scope_mismatch",
                        "observation is outside the reported session/world scope",
                    )
                if self._source_health.state != "in_world":
                    self._reject(
                        "source_not_in_world",
                        "observation is not eligible while the world session is transferring/disconnected",
                    )

            clock_skew_ms = observation.observed_at_unix_ms - received_at_ms
            export_skew_ms = observation.exported_at_unix_ms - received_at_ms
            if max(clock_skew_ms, export_skew_ms) > self._max_future_skew_ms:
                self._reject(
                    "future_observation", "observation timestamp is too far in the future"
                )

            wall_age_ms = max(0, received_at_ms - observation.observed_at_unix_ms)
            position_wall_age_ms = max(
                0, received_at_ms - observation.position_observed_at_unix_ms
            )
            received_mono_ns = self._now_mono_ns()

            previous = self._latest.observation if self._latest is not None else None
            if previous is not None:
                if observation.session_epoch < previous.session_epoch:
                    self._reject("stale_session", "observation session is older than the active session")
                if observation.session_epoch == previous.session_epoch:
                    if observation.world_generation < previous.world_generation:
                        self._reject(
                            "stale_world_generation",
                            "observation world generation went backwards",
                        )
                    if observation.sequence == previous.sequence:
                        self._reject(
                            "duplicate_sequence", "observation sequence was already accepted"
                        )
                    if observation.sequence < previous.sequence:
                        self._reject(
                            "stale_sequence", "observation sequence went backwards"
                        )
                    if observation.client_uptime_ms < previous.client_uptime_ms:
                        self._reject(
                            "non_monotonic_uptime",
                            "client uptime went backwards within the session",
                        )
                    if observation.observed_at_unix_ms < previous.observed_at_unix_ms:
                        self._reject(
                            "observation_time_regression",
                            "source observation time went backwards within the session",
                        )
                    if (
                        observation.world_generation == previous.world_generation
                        and observation.position_observed_at_unix_ms
                        < previous.position_observed_at_unix_ms
                    ):
                        self._reject(
                            "position_time_regression",
                            "position observation time went backwards within the world",
                        )
                    if observation.state_revision < previous.state_revision:
                        self._reject(
                            "state_revision_regression",
                            "state revision went backwards within the session",
                        )
                    if observation.state_revision == previous.state_revision:
                        if not observation.same_state(previous):
                            self._reject(
                                "state_revision_conflict",
                                "one state revision describes conflicting states",
                            )
                        if (
                            observation.observed_at_unix_ms
                            != previous.observed_at_unix_ms
                        ):
                            self._reject(
                                "unchanged_state_refresh",
                                "unchanged state cannot acquire a new source timestamp",
                            )

            stored = StoredObservation(
                observation=observation,
                received_at_unix_ms=received_at_ms,
                received_mono_ns=received_mono_ns,
                wall_age_ms=wall_age_ms,
                position_wall_age_ms=position_wall_age_ms,
                age_ms=wall_age_ms,
                position_age_ms=position_wall_age_ms,
                clock_skew_ms=clock_skew_ms,
            )
            if self._latest is not None:
                self._superseded += 1
            self._latest = stored
            self._player_guid = observation.player_guid
            self._session_epoch = observation.session_epoch
            self._world_generation = observation.world_generation
            self._last_sequence = observation.sequence
            self._last_uptime_ms = observation.client_uptime_ms
            self._accepted += 1
            self._ever_accepted = True
            return stored

    def latest_for(self, player_guid: int) -> StoredObservation | None:
        """Return the latest observation only if it is valid and in scope."""
        status = self.status_for(player_guid)
        return status.observation if status.state == "valid" else None

    def status_for(
        self,
        player_guid: int,
        *,
        max_position_age_ms: int | None = None,
    ) -> SnapshotStatus:
        """Classify the current player's snapshot for WM consumers.

        Aggregate freshness gates the snapshot; position provenance is returned separately and may
        be additionally bounded by the caller's action-class policy without pretending the
        aggregate timestamp proves the player was re-observed.
        """
        if isinstance(player_guid, bool) or not isinstance(player_guid, int):
            return SnapshotStatus("unknown", None, None, None, None)
        if max_position_age_ms is not None:
            if isinstance(max_position_age_ms, bool) or not isinstance(
                max_position_age_ms, int
            ):
                raise TypeError("max_position_age_ms must be an integer or None")
            if max_position_age_ms < 0:
                raise ValueError("max_position_age_ms must not be negative")
        with self._lock:
            if self._latest is None or self._player_guid != player_guid:
                disconnected = (
                    self._latest is None
                    and (
                        (
                            self._ever_accepted
                            and self._last_invalidation_reason
                            in {
                                "disconnect",
                                "server_close",
                                "server_stop",
                                "startup_failure",
                                "runtime_shutdown",
                                "source_disconnected",
                            }
                        )
                        or (
                            self._source_health is not None
                            and self._source_health.state == "disconnected"
                        )
                    )
                )
                return SnapshotStatus(
                    "disconnected" if disconnected else "unknown",
                    None,
                    None,
                    None,
                    None,
                )
            now_ms = self._now_ms()
            now_mono_ns = self._now_mono_ns()
            elapsed_ms = max(
                0, (now_mono_ns - self._latest.received_mono_ns) // 1_000_000
            )
            observed_at_ms = self._latest.observation.observed_at_unix_ms
            exported_at_ms = self._latest.observation.exported_at_unix_ms
            clock_skew_ms = observed_at_ms - now_ms
            export_skew_ms = exported_at_ms - now_ms
            age_ms = self._latest.wall_age_ms + elapsed_ms
            position_age_ms = self._latest.position_wall_age_ms + elapsed_ms
            health_age_ms = (
                max(0, (now_mono_ns - self._health_mono_ns) // 1_000_000)
                if self._health_mono_ns is not None
                else None
            )
            state = "valid"
            if max(clock_skew_ms, export_skew_ms) > self._max_future_skew_ms:
                state = "stale"
            elif self._source_health is None:
                state = "stale"
            elif self._source_health.state != "in_world":
                state = "stale"
            elif not self._source_health.same_scope(self._latest.observation):
                state = "stale"
            elif (
                health_age_ms is None
                or health_age_ms > DEFAULT_HEALTH_MAX_AGE_MS
            ):
                state = "stale"
            elif (
                max_position_age_ms is not None
                and position_age_ms > max_position_age_ms
            ):
                state = "stale"
            observation = (
                StoredObservation(
                    observation=self._latest.observation,
                    received_at_unix_ms=self._latest.received_at_unix_ms,
                    received_mono_ns=self._latest.received_mono_ns,
                    wall_age_ms=self._latest.wall_age_ms,
                    position_wall_age_ms=self._latest.position_wall_age_ms,
                    age_ms=age_ms,
                    position_age_ms=position_age_ms,
                    clock_skew_ms=clock_skew_ms,
                )
                if state == "valid"
                else None
            )
            return SnapshotStatus(
                state,
                observation,
                age_ms,
                position_age_ms,
                self._latest.observation.world_generation,
            )

    def stats(self) -> StoreStats:
        with self._lock:
            now_mono_ns = self._now_mono_ns()
            latest_age_ms: int | None = None
            latest_position_age_ms: int | None = None
            if self._latest is not None:
                elapsed_ms = max(
                    0, (now_mono_ns - self._latest.received_mono_ns) // 1_000_000
                )
                latest_age_ms = self._latest.wall_age_ms + elapsed_ms
                latest_position_age_ms = (
                    self._latest.position_wall_age_ms + elapsed_ms
                )
            health_age_ms = (
                max(0, (now_mono_ns - self._health_mono_ns) // 1_000_000)
                if self._health_mono_ns is not None
                else None
            )
            return StoreStats(
                accepted=self._accepted,
                rejected=self._rejected,
                superseded=self._superseded,
                invalidations=self._invalidations,
                active_player_guid=self._player_guid,
                active_session_epoch=self._session_epoch,
                active_world_generation=self._world_generation,
                last_sequence=self._last_sequence,
                latest_age_ms=latest_age_ms,
                latest_position_age_ms=latest_position_age_ms,
                source_session_state=(
                    self._source_health.state
                    if self._source_health is not None
                    else None
                ),
                health_age_ms=health_age_ms,
                last_invalidation_reason=self._last_invalidation_reason,
            )

    def _invalidate_locked(self, reason: str) -> None:
        self._latest = None
        self._player_guid = None
        self._session_epoch = None
        self._world_generation = None
        self._last_sequence = None
        self._last_uptime_ms = None
        self._source_health = None
        self._health_mono_ns = None
        self._invalidations += 1
        self._last_invalidation_reason = reason

    def _reject(self, code: str, message: str) -> NoReturn:
        self._rejected += 1
        raise ObservationRejected(code, message)
