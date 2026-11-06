"""Reusable lifecycle composition for the optional native observation receiver.

This is not a second service. It composes the existing `ObservationStore`,
`ObservationServer`, and `NativeClientObservationLoader` around the lifecycle already owned by
WM's `AutoplayService`. The receiver remains optional: no token means no runtime, and any startup
failure is raised as one typed error after cleanup so the caller can retain its DB-only path.
"""

from __future__ import annotations

import logging
import os
from dataclasses import dataclass

from .constants import (
    DEFAULT_IDLE_TIMEOUT_S,
    DEFAULT_MAX_FUTURE_SKEW_MS,
    DEFAULT_MAX_OBSERVATION_AGE_MS,
    DEFAULT_PORT,
    TOKEN_ENV,
)
from .context_adapter import (
    FallbackNativeContextSnapshotLoader,
    NativeClientObservationLoader,
    NativeSnapshotLoader,
)
from .errors import NativeObservationStartupError
from .receiver import DEFAULT_HOST, ObservationServer, ReceiverHealth
from .store import ObservationStore, SnapshotStatus, StoreStats

logger = logging.getLogger("wm_native.runtime")


@dataclass(frozen=True)
class NativeObservationHealth:
    running: bool
    receiver: ReceiverHealth
    store: StoreStats
    snapshot: SnapshotStatus | None


class NativeObservationRuntime:
    """One store, one receiver, and one native snapshot loader for a service lifetime."""

    def __init__(
        self,
        token: str,
        *,
        host: str = DEFAULT_HOST,
        port: int = DEFAULT_PORT,
        idle_timeout_s: float = DEFAULT_IDLE_TIMEOUT_S,
        max_age_ms: int = DEFAULT_MAX_OBSERVATION_AGE_MS,
        max_future_skew_ms: int = DEFAULT_MAX_FUTURE_SKEW_MS,
        allowed_player_guid: int | None = None,
    ) -> None:
        self._store = ObservationStore(
            max_age_ms=max_age_ms,
            max_future_skew_ms=max_future_skew_ms,
            allowed_player_guid=allowed_player_guid,
        )
        self._server = ObservationServer(
            self._store,
            token,
            host=host,
            port=port,
            idle_timeout_s=idle_timeout_s,
        )
        self._loader = NativeClientObservationLoader(self._store)
        self._host = host
        self._started = False
        self._closed = False

    @classmethod
    def start_from_env(
        cls,
        *,
        token_env: str = TOKEN_ENV,
        host: str = DEFAULT_HOST,
        port: int = DEFAULT_PORT,
        idle_timeout_s: float = DEFAULT_IDLE_TIMEOUT_S,
        max_age_ms: int = DEFAULT_MAX_OBSERVATION_AGE_MS,
        max_future_skew_ms: int = DEFAULT_MAX_FUTURE_SKEW_MS,
        allowed_player_guid: int | None = None,
    ) -> NativeObservationRuntime | None:
        """Construct and start the receiver when an owner token is present.

        Returns `None` when `token_env` is unset or empty, preserving the service's normal
        DB-only behavior. Invalid configuration or bind/start failure raises a single
        `NativeObservationStartupError` after closing the receiver and invalidating the store.
        """
        token = os.environ.get(token_env)
        if not token:
            logger.debug("%s is unset; native observation receiver disabled", token_env)
            return None

        try:
            runtime = cls(
                token,
                host=host,
                port=port,
                idle_timeout_s=idle_timeout_s,
                max_age_ms=max_age_ms,
                max_future_skew_ms=max_future_skew_ms,
                allowed_player_guid=allowed_player_guid,
            )
        except Exception as error:
            raise NativeObservationStartupError(
                "native_observation_start_failed",
                "WM native observation receiver configuration is invalid",
            ) from error

        try:
            return runtime.start()
        except NativeObservationStartupError:
            raise
        except Exception as error:
            runtime.close()
            raise NativeObservationStartupError(
                "native_observation_start_failed",
                "WM native observation receiver failed to start",
            ) from error

    @property
    def store(self) -> ObservationStore:
        return self._store

    @property
    def loader(self) -> NativeClientObservationLoader:
        return self._loader

    @property
    def is_running(self) -> bool:
        return self._server.health().running

    def health(self) -> NativeObservationHealth:
        receiver = self._server.health()
        allowed = self._store.allowed_player_guid
        snapshot = (
            self._store.status_for(allowed) if allowed is not None else None
        )
        return NativeObservationHealth(
            running=receiver.running,
            receiver=receiver,
            store=self._store.stats(),
            snapshot=snapshot,
        )

    def start(self) -> NativeObservationRuntime:
        """Start the receiver once. The server performs the blocking bind on its own thread."""
        if self._closed:
            raise RuntimeError("native observation runtime is closed")
        if self._started:
            return self
        try:
            self._server.start_background()
        except Exception as error:
            self._started = False
            self._closed = True
            self._store.invalidate("startup_failure")
            try:
                self._server.close()
            except Exception:
                pass
            raise NativeObservationStartupError(
                "native_observation_start_failed",
                "WM native observation receiver failed to start",
            ) from error
        self._started = True
        logger.info(
            "native observation receiver started on %s:%s",
            self._host,
            self._server.bound_port,
        )
        return self

    def fallback_loader(self, fallback: NativeSnapshotLoader) -> NativeSnapshotLoader:
        """Compose native-first, existing-loader fallback for this runtime."""
        return FallbackNativeContextSnapshotLoader(primary=self._loader, fallback=fallback)

    def close(self) -> None:
        """Idempotently stop the receiver and invalidate the shared store."""
        if self._closed:
            return
        self._closed = True
        try:
            self._server.close()
        finally:
            self._store.invalidate("runtime_shutdown")
            self._started = False
            logger.info("native observation receiver stopped")

    def __enter__(self) -> NativeObservationRuntime:
        return self.start()

    def __exit__(self, exc_type: object, exc: object, traceback: object) -> None:
        self.close()
