"""One-client, loopback-only observation receiver.

The receiver validates wire frames, keeps only the latest source-fresh observation, and exposes that
store to a WM context adapter. Every accepted connection starts a new ordering scope; disconnect,
protocol failure, idle timeout, and server shutdown invalidate the native snapshot immediately. It
never executes commands, mutates the game/server, or builds a second decision loop.
"""

from __future__ import annotations

import argparse
import ipaddress
import logging
import os
import socket
import struct
import threading
from dataclasses import dataclass
from typing import Final

from .constants import (
    DEFAULT_IDLE_TIMEOUT_S,
    DEFAULT_MAX_FUTURE_SKEW_MS,
    DEFAULT_MAX_OBSERVATION_AGE_MS,
    DEFAULT_PORT,
    MAX_FRAME_BYTES,
    TOKEN_ENV,
)
from .errors import ProtocolError, WmNativeError
from .protocol import (
    ack_frame,
    error_frame,
    health_ack_frame,
    hello_ack_frame,
    parse_client_frame,
    parse_hello,
    validate_token,
)
from .store import ObservationStore

DEFAULT_HOST: Final = "127.0.0.1"
_HANDSHAKE_TIMEOUT_S: Final = 5.0
_SEND_TIMEOUT_S: Final = 1.0
_STOP_JOIN_TIMEOUT_S: Final = 2.0

logger = logging.getLogger("wm_native.receiver")


@dataclass(frozen=True)
class ReceiverHealth:
    running: bool
    thread_alive: bool
    failed: bool
    client_connected: bool
    bound_port: int | None
    accepted: int
    health_acks: int
    rejected: int
    last_accepted: tuple[int, int, int] | None  # (sequence, state_revision, world_generation)
    last_health: tuple[int, int, int, str] | None  # epoch, world, player, state
    last_error: str | None


class _ConnectionClosed(Exception):
    pass


class ObservationServer:
    """A single-listener server with at most one authenticated client."""

    def __init__(
        self,
        store: ObservationStore,
        token: str,
        *,
        host: str = DEFAULT_HOST,
        port: int = DEFAULT_PORT,
        idle_timeout_s: float = DEFAULT_IDLE_TIMEOUT_S,
    ) -> None:
        if not isinstance(store, ObservationStore):
            raise TypeError("store must be an ObservationStore")
        validate_token(token)
        address = ipaddress.ip_address(host)
        if not address.is_loopback:
            raise ValueError("WM native receiver must bind to a loopback address")
        if isinstance(port, bool) or not isinstance(port, int):
            raise TypeError("port must be an integer")
        if port < 0 or port > 65_535:
            raise ValueError("port must be between 0 and 65535")
        if idle_timeout_s <= 0:
            raise ValueError("idle_timeout_s must be positive")

        self._store = store
        self._token = token
        self._host = host
        self._port = port
        self._address = address
        self._idle_timeout_s = float(idle_timeout_s)
        self._logger = logger

        self._stop = threading.Event()
        self._lock = threading.Lock()
        self._listener: socket.socket | None = None
        self._client: socket.socket | None = None
        self._thread: threading.Thread | None = None
        self._ready = threading.Event()
        self._bound_port: int | None = None
        self._startup_error: BaseException | None = None
        self._failed = False
        self._accepted = 0
        self._health_acks = 0
        self._rejected = 0
        self._last_accepted: tuple[int, int, int] | None = None
        self._last_health: tuple[int, int, int, str] | None = None
        self._last_error: str | None = None

    def __repr__(self) -> str:
        return (
            f"ObservationServer(host={self._host!r}, port={self._port}, "
            f"bound_port={self._bound_port})"
        )

    @property
    def store(self) -> ObservationStore:
        return self._store

    @property
    def bound_port(self) -> int | None:
        return self._bound_port

    def health(self) -> ReceiverHealth:
        with self._lock:
            thread_alive = self._thread is not None and self._thread.is_alive()
            client_connected = self._client is not None
            return ReceiverHealth(
                running=thread_alive and not self._stop.is_set(),
                thread_alive=thread_alive,
                failed=self._failed,
                client_connected=client_connected,
                bound_port=self._bound_port,
                accepted=self._accepted,
                health_acks=self._health_acks,
                rejected=self._rejected,
                last_accepted=self._last_accepted,
                last_health=self._last_health,
                last_error=self._last_error,
            )

    def start_background(self) -> ObservationServer:
        """Start the receiver thread and wait until it has bound or failed."""
        if self._thread is not None:
            raise RuntimeError("observer server was already started")
        self._thread = threading.Thread(
            target=self._run,
            name="wm-native-observation",
            daemon=True,
        )
        self._thread.start()
        if not self._ready.wait(timeout=5.0):
            self.close()
            raise RuntimeError("timed out starting the WM native observation receiver")
        if self._startup_error is not None:
            error = self._startup_error
            self._startup_error = None
            raise error
        return self

    def serve_forever(self) -> None:
        """Block and serve exactly one active connection at a time."""
        try:
            self._serve()
        finally:
            self._close_sockets()
            self._store.invalidate("server_stop")

    def close(self) -> None:
        """Stop accepting, invalidate WM visibility, and join the background thread."""
        self._stop.set()
        self._close_sockets()
        self._store.invalidate("server_close")
        thread = self._thread
        if (
            thread is not None
            and thread.is_alive()
            and thread is not threading.current_thread()
        ):
            thread.join(timeout=_STOP_JOIN_TIMEOUT_S)

    def _run(self) -> None:
        try:
            self.serve_forever()
        except BaseException as error:
            self._startup_error = error
            with self._lock:
                self._failed = True
                self._last_error = f"{type(error).__name__}: {error}"[:256]
            if self._ready.is_set() and not self._stop.is_set():
                self._logger.exception(
                    "WM native observation receiver stopped unexpectedly"
                )
        finally:
            self._ready.set()

    def _serve(self) -> None:
        family = socket.AF_INET6 if self._address.version == 6 else socket.AF_INET
        listener = socket.socket(family, socket.SOCK_STREAM)
        try:
            listener.setsockopt(socket.SOL_SOCKET, socket.SO_REUSEADDR, 1)
            bind_address = (
                (self._host, self._port, 0, 0)
                if family == socket.AF_INET6
                else (self._host, self._port)
            )
            listener.bind(bind_address)
            listener.listen(1)
            listener.settimeout(0.5)
            with self._lock:
                self._listener = listener
            self._bound_port = int(listener.getsockname()[1])
            self._ready.set()
            self._logger.info(
                "WM native observation receiver listening on %s:%s",
                self._host,
                self._bound_port,
            )

            while not self._stop.is_set():
                try:
                    client, peer = listener.accept()
                except socket.timeout:
                    continue
                except OSError:
                    if self._stop.is_set():
                        break
                    raise

                if self._stop.is_set():
                    client.close()
                    break

                with self._lock:
                    busy = self._client is not None
                    if not busy:
                        self._client = client

                if busy:
                    self._send_best_effort(
                        client,
                        error_frame(
                            "busy",
                            "receiver already has an active connection",
                        ),
                    )
                    client.close()
                    continue

                try:
                    self._handle_client(client, peer)
                except Exception:
                    self._logger.exception(
                        "unhandled WM native observation connection error"
                    )
                finally:
                    with self._lock:
                        if self._client is client:
                            self._client = None
                    try:
                        client.close()
                    except OSError:
                        pass
                    self._store.invalidate("disconnect")
        finally:
            with self._lock:
                if self._listener is listener:
                    self._listener = None
            try:
                listener.close()
            except OSError:
                pass

    def _record_rejection(self, code: str) -> None:
        with self._lock:
            self._rejected += 1
            self._last_error = code[:256]

    def _handle_client(self, client: socket.socket, peer: object) -> None:
        try:
            client.settimeout(_HANDSHAKE_TIMEOUT_S)
            hello_payload = _read_frame(client)
            parse_hello(hello_payload, expected_token=self._token)
            self._store.begin_connection()
        except socket.timeout:
            self._record_rejection("handshake_timeout")
            self._send_best_effort(
                client, error_frame("handshake_timeout", "handshake timed out")
            )
            self._logger.warning("handshake timed out for %s", peer)
            return
        except _ConnectionClosed:
            return
        except WmNativeError as error:
            self._record_rejection(error.code)
            self._send_best_effort(client, error_frame(error.code, error.message))
            self._logger.warning("rejected handshake from %s: %s", peer, error.code)
            return
        except OSError:
            return

        try:
            self._send_frame(client, hello_ack_frame())
            client.settimeout(self._idle_timeout_s)
        except OSError:
            return

        self._logger.info("accepted WM native observation connection from %s", peer)

        while not self._stop.is_set():
            try:
                payload = _read_frame(client)
            except socket.timeout:
                self._logger.info("closing idle WM native connection from %s", peer)
                return
            except _ConnectionClosed:
                return
            except WmNativeError as error:
                self._send_best_effort(client, error_frame(error.code, error.message))
                self._logger.warning(
                    "rejected observation frame from %s: %s", peer, error.code
                )
                return
            except OSError:
                return

            try:
                frame_type, frame = parse_client_frame(payload)
                if frame_type == "health":
                    health = frame
                    self._store.update_health(health)
                    health_ack = health_ack_frame(
                        session_epoch=health.session_epoch,
                        world_generation=health.world_generation,
                        player_guid=health.player_guid,
                    )
                    self._send_frame(client, health_ack)
                    with self._lock:
                        self._health_acks += 1
                        self._last_health = (
                            health.session_epoch,
                            health.world_generation,
                            health.player_guid,
                            health.state,
                        )
                        self._last_error = None
                    self._logger.debug(
                        "accepted health player=%s world=%s session=%s state=%s",
                        health.player_guid,
                        health.world_generation,
                        health.session_epoch,
                        health.state,
                    )
                    continue
                observation = frame
                stored = self._store.accept(observation)
            except WmNativeError as error:
                self._record_rejection(error.code)
                self._send_best_effort(client, error_frame(error.code, error.message))
                self._logger.warning(
                    "rejected frame from %s: %s", peer, error.code
                )
                return
            except Exception:
                self._record_rejection("internal_error")
                self._send_best_effort(
                    client,
                    error_frame(
                        "internal_error",
                        "frame could not be accepted",
                    ),
                )
                self._logger.exception(
                    "unexpected frame error from %s", peer
                )
                return

            with self._lock:
                self._accepted += 1
                self._last_accepted = (
                    stored.observation.sequence,
                    stored.observation.state_revision,
                    stored.observation.world_generation,
                )
                self._last_error = None

            try:
                self._send_frame(
                    client,
                    ack_frame(
                        sequence=stored.observation.sequence,
                        state_revision=stored.observation.state_revision,
                        world_generation=stored.observation.world_generation,
                    ),
                )
            except OSError:
                return

            self._logger.debug(
                "accepted observation player=%s world=%s session=%s sequence=%s age_ms=%s position_age_ms=%s",
                stored.observation.player_guid,
                stored.observation.world_generation,
                stored.observation.session_epoch,
                stored.observation.sequence,
                stored.age_ms,
                stored.position_age_ms,
            )

    def _send_frame(self, client: socket.socket, frame: bytes) -> None:
        previous_timeout = client.gettimeout()
        client.settimeout(_SEND_TIMEOUT_S)
        try:
            client.sendall(frame)
        finally:
            try:
                client.settimeout(previous_timeout)
            except OSError:
                pass

    def _send_best_effort(self, client: socket.socket, frame: bytes) -> None:
        try:
            self._send_frame(client, frame)
        except OSError:
            pass

    def _close_sockets(self) -> None:
        with self._lock:
            listener = self._listener
            client = self._client
            self._listener = None
            self._client = None
        if listener is not None:
            try:
                listener.close()
            except OSError:
                pass
        if client is not None:
            try:
                client.shutdown(socket.SHUT_RDWR)
            except OSError:
                pass
            try:
                client.close()
            except OSError:
                pass


def _recv_exact(client: socket.socket, size: int) -> bytes:
    buffer = bytearray()
    while len(buffer) < size:
        chunk = client.recv(size - len(buffer))
        if not chunk:
            raise _ConnectionClosed
        buffer.extend(chunk)
    return bytes(buffer)


def _read_frame(client: socket.socket) -> bytes:
    header = _recv_exact(client, 4)
    length = struct.unpack(">I", header)[0]
    if length == 0:
        raise ProtocolError("empty_frame", "empty frame")
    if length > MAX_FRAME_BYTES:
        raise ProtocolError(
            "frame_too_large",
            f"frame exceeds the {MAX_FRAME_BYTES}-byte maximum",
        )
    return _recv_exact(client, length)


def main(argv: list[str] | None = None) -> int:
    parser = argparse.ArgumentParser(
        description="Loopback-only WM native observation receiver"
    )
    parser.add_argument("--host", default=DEFAULT_HOST)
    parser.add_argument("--port", type=int, default=DEFAULT_PORT)
    parser.add_argument("--token-env", default=TOKEN_ENV)
    parser.add_argument("--player-guid", type=int, default=None)
    parser.add_argument(
        "--max-age-ms", type=int, default=DEFAULT_MAX_OBSERVATION_AGE_MS
    )
    parser.add_argument(
        "--max-future-skew-ms",
        type=int,
        default=DEFAULT_MAX_FUTURE_SKEW_MS,
    )
    parser.add_argument(
        "--idle-timeout", type=float, default=DEFAULT_IDLE_TIMEOUT_S
    )
    parser.add_argument(
        "--log-level",
        default="INFO",
        choices=("DEBUG", "INFO", "WARNING", "ERROR"),
    )
    args = parser.parse_args(argv)

    logging.basicConfig(
        level=getattr(logging, args.log_level),
        format="%(asctime)s %(levelname)s %(name)s: %(message)s",
    )

    token = os.environ.get(args.token_env)
    if token is None:
        parser.error(f"{args.token_env} is not set")

    try:
        store = ObservationStore(
            max_age_ms=args.max_age_ms,
            max_future_skew_ms=args.max_future_skew_ms,
            allowed_player_guid=args.player_guid,
        )
        server = ObservationServer(
            store,
            token,
            host=args.host,
            port=args.port,
            idle_timeout_s=args.idle_timeout,
        )
    except (TypeError, ValueError) as error:
        parser.error(str(error))

    try:
        server.serve_forever()
    except KeyboardInterrupt:
        logger.info("WM native observation receiver interrupted")
    finally:
        server.close()
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
