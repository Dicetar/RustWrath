# 10 — B-001B-2: run the live-change proof (A→B on running server)

**What to build:** execute the 03 contract on the lab WM runtime: library B saved while A active; newly purchased B executes while the running A instance completes correctly on A; the selected logical action chooses B, rollback selects A; WM performs publication/selection through its typed owned path (or the missing adapter is recorded as a gate dependency, not silently downgraded). Server PID + binary hash unchanged across A→B→rollback; invalid B rejected with A still usable; no duplicated pulses/timers; logout-ends-instances recorded as the documented limit in the proof packet.

**Blocked by:** 03.

**Status:** ready-for-agent

- [ ] Four-point evidence recorded with 1-2 kept distinct from 3-4
- [ ] No-reboot evidence: PID and binary hash identical across A→B→rollback
- [ ] Invalid-B rejection demonstrated with A usable
- [ ] Proof packet filed in LIVE_PROOF_BACKLOG house style
