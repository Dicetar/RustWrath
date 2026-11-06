"""Errors for the native observation receiver.

Messages are intentionally short and never include the authentication token or observation values.
"""


class WmNativeError(Exception):
    """Base error with a stable machine-readable code."""

    def __init__(self, code: str, message: str) -> None:
        super().__init__(message)
        self.code = code
        self.message = message


class ProtocolError(WmNativeError):
    """The peer sent a malformed frame or an invalid handshake."""


class ObservationValidationError(WmNativeError):
    """An observation object does not satisfy the versioned wire contract."""


class ObservationRejected(WmNativeError):
    """A structurally valid observation is stale, duplicate, or out of scope."""


class NativeObservationStartupError(WmNativeError):
    """The optional native receiver could not be constructed or started."""
