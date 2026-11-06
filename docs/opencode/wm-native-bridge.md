# Native client -> WM observation bridge (superseded draft)

Updated: 2026-10-25. Status: **SUPERSEDED**. Do not implement against this file.

The stable producer/consumer API is now:

- `docs/opencode/convergence-consumer.md`

That contract replaces schema `wm.native.observation.v1` with
`wm.native.observation.v2`, adds explicit target and visibility state, separates source freshness
from export time, defines session-generation and disconnect/reconnect rules, and specifies the
bounded `ObservationPublisher` handoff.

Kept only as history:

- The initial queue-02 bridge chose loopback TCP with a 4-byte big-endian length prefix and JSON,
  rather than a named pipe or WebSocket, because WM declared only `pydantic` and `PyMySQL` and the
  stdlib-only receiver avoided new dependencies. The current contract retains that transport.
- The old v1 examples, `target_guid`/`visible_guids` field list, synchronous-send claims, and
  one-shot reconnect policy are obsolete. They are intentionally removed from the active
  documentation.
- No production WM file was edited by that draft, and the standalone receiver was never
  WM-integrated.
