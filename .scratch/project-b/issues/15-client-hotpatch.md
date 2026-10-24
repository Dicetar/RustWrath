# 15 — Client-side hot-patching (owner long-term goal)

**What to build:** live client-side changes without client restart/rebuild, staged: C1 data/config/Lua hot-reload on the manifest loader + addon host; C2 WM-pushed versioned client-content publications over the native client channel; C3 optional dylib hot-swap for presentation modules. Ticket 05's loader must keep a re-import seam; ticket 11's transport design must anticipate C2 push semantics. Affected surfaces and acceptance per stage to be specified when reached.

**Blocked by:** 05 (loader seam, C1), 11 (transport, C2).

**Status:** long-term — owner goal recorded 2026-10-24 in docs/baseline/owner-decisions.md

- [ ] C1: change a client asset + a Lua file on disk; running client reflects both without restart
- [ ] C2: WM publishes a client-content revision to a connected client; applied live, versioned, rollback = previous revision
- [ ] C3 (optional): swap one native presentation module without client restart, at a safe state boundary
