# 11 — B-008: WM observation adapter

**What to build:** small local transport (named pipe or loopback WebSocket — chosen after checking current WM dependencies; gRPC optional, not required) exposing the 08 observation export to the existing Python WM perception/context owners. Server event ingestion remains authoritative for outcomes; the client channel is additive. Session/player scope preserved across reconnect.

**Blocked by:** 07 (observation contract shape), 08 (export populated).

**Status:** ready-for-agent

- [ ] WM reads a live session's observation stream end-to-end
- [ ] Transport choice recorded with dependency rationale
- [ ] No second director/registry created in the client
