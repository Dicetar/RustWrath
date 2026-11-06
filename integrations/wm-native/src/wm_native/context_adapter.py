"""Adapter from the native observation store to WM's context snapshot shape."""

from __future__ import annotations

from datetime import datetime, timezone
from typing import Any, Protocol

from .store import ObservationStore


class NativeSnapshotLoader(Protocol):
    def load_latest(self, *, player_guid: int) -> dict[str, Any] | None:
        ...


def _iso8601_utc_ms(unix_ms: int) -> str:
    seconds, millis = divmod(unix_ms, 1_000)
    moment = datetime.fromtimestamp(seconds, tz=timezone.utc).replace(
        microsecond=millis * 1_000
    )
    return moment.isoformat(timespec="milliseconds").replace("+00:00", "Z")


class NativeClientObservationLoader:
    """Implements `wm.context.builder.NativeContextSnapshotLoader`.

    It intentionally exposes only an accepted, in-scope, still-fresh observation. If the store has
    no observation, another player's observation, or an expired observation, it returns `None` so
    WM's existing context path degrades to its normal "no snapshot" behavior.
    """

    def __init__(self, store: ObservationStore) -> None:
        if not isinstance(store, ObservationStore):
            raise TypeError("store must be an ObservationStore")
        self._store = store

    def load_latest(self, *, player_guid: int) -> dict[str, Any] | None:
        stored = self._store.latest_for(int(player_guid))
        if stored is None:
            return None

        observation = stored.observation
        stats = self._store.stats()
        return {
            "snapshot_id": observation.sequence,
            "request_id": None,
            "occurred_at": _iso8601_utc_ms(observation.observed_at_unix_ms),
            "player_guid": observation.player_guid,
            "context_kind": "native_client_observation",
            "radius": None,
            "map_id": observation.map_id,
            "zone_id": observation.zone_id,
            "area_id": observation.area_id,
            "source": "benilla-wm-native",
            "payload": {
                "schema_version": observation.schema_version,
                "snapshot_state": "valid",
                "session_epoch": observation.session_epoch,
                "world_generation": observation.world_generation,
                "sequence": observation.sequence,
                "state_revision": observation.state_revision,
                "observed_at_unix_ms": observation.observed_at_unix_ms,
                "exported_at_unix_ms": observation.exported_at_unix_ms,
                "received_at_unix_ms": stored.received_at_unix_ms,
                "age_ms": stored.age_ms,
                "position_age_ms": stored.position_age_ms,
                "source_session_state": stats.source_session_state,
                "health_age_ms": stats.health_age_ms,
                "clock_skew_ms": stored.clock_skew_ms,
                "client_uptime_ms": observation.client_uptime_ms,
                "position_observed_at_unix_ms": observation.position_observed_at_unix_ms,
                "position": observation.position.to_payload(),
                "target": observation.target.to_payload(),
                "visibility": observation.visibility.to_payload(),
            },
        }


class FallbackNativeContextSnapshotLoader:
    """Native-client-first composition over the existing WM snapshot loader.

    This preserves the current DB snapshot as a read-only fallback when the native client has not
    produced a fresh observation. It is a selector between two read-only loaders, not a director,
    history store, or mutation path.
    """

    def __init__(
        self,
        *,
        primary: NativeSnapshotLoader,
        fallback: NativeSnapshotLoader,
    ) -> None:
        self._primary = primary
        self._fallback = fallback

    def load_latest(self, *, player_guid: int) -> dict[str, Any] | None:
        snapshot = self._primary.load_latest(player_guid=int(player_guid))
        if snapshot is not None:
            return snapshot
        return self._fallback.load_latest(player_guid=int(player_guid))
