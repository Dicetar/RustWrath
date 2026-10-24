# 08 — B-006: movement + world join (walk in Elwynn on AC)

**What to build:** typed reducer state feeds the app entity mirror; movement messages (local input, remote interpolation, server corrections, tile boundary crossing, map transition resets); basic chat, selection, diagnostic state display; minimal observation export (session, player, map, position, target, visible ids, freshness, sequence) reusing reducer output. Ten-minute walk/stops/turns/jumps/chat/disconnect-return without parser failure; movement discrepancy tolerance recorded beforehand.

**Blocked by:** 05 (route assets), 07 (object reducer).

**Status:** ready-for-agent

- [ ] LOGIN milestone exit condition met on lab server (walk/stops/turns/jumps, tile crossing, chat, disconnect-return)
- [ ] Movement discrepancy tolerance recorded before the trial
- [ ] Observation export schema versioned and populated from reducer output
