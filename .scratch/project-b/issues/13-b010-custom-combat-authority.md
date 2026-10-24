# 13 — B-010: server-authoritative dodge (experimental combat v1)

**What to build:** hit-outcome propagation call graph (melee/spell/periodic/split/shield/environmental paths with irreversible outputs marked); dodge implemented by forcing/reusing the stock dodge outcome under custom eligibility at hit-outcome construction (prior art: stock dodge propagation; sparring HITINFO_FAKE_DAMAGE), carried consistently into the attack packet, procs (ProcSkillsAndAuras/Reactives/TriggerAurasProcOnEvent) and combat log; allowlist of dodgeable sources; timing v1 = server monotonic [start,end) eligibility at authoritative resolution, no rewind; duplicate/sequence rejection; stamina state machine Ready→Windup→Active→Recovery + Dodge/Interrupted. One enemy, one telegraph, one dodge, stamina, death/reset.

**Blocked by:** 09.

**Status:** ready-for-agent

- [ ] Call-graph deliverable with decision point chosen per path
- [ ] Before/during/after-window, insufficient stamina, duplicate/delayed/out-of-order, interruption/death, reconnect all agree client/server
