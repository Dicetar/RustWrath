# 09 — B-007: one ordinary WotLK gameplay loop

**What to build:** accept quest → fight (normal attacks + one selected spell) → loot → progress → turn in → reconnect; plus death/recovery and one rejected action. Server AI and persistence exercised — no client-simulated damage/credit. UI surface only as the loop requires, remaining stock FrameXML gaps listed. Protocol success, UI completeness, and gameplay correctness tracked separately.

**Blocked by:** 08.

**Status:** ready-for-agent

- [ ] Full loop + death/recovery + one rejected action evidenced
- [ ] Reconnect-after-reward and reconnect-after-death checks pass
- [ ] Remaining FrameXML gap list recorded for 12
