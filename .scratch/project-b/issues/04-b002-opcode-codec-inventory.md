# 04 — B-002: opcode ledger + wrath codec coverage inventory

**What to build:** for each message in the P1 minimum transcript (auth→world session→initial updates→movement), a table row: local AC handler owner; gtker wow_world_messages 0.3.x wrath codec present y/n; adoption status (adopt / adopt+verify / rewrite); fixture status. Owner map corrections already fixed in (WorldSocket HandlePing :734, LOGIN_VERIFY_WORLD CharacterHandler :806, authserver AuthSession.cpp). Extension-ID ledger from 01 cross-checked. Inventory precedes any hand-written codec.

**Blocked by:** 01.

**Status:** ready-for-agent

- [ ] Minimum-transcript message list from source-observed session path
- [ ] Per-message coverage/adoption table filled
- [ ] UpdateObject/MovementInfo representation in wrath module verified (docs listing was truncated)
- [ ] Adopted-codec risk register (compression, update fields, local extensions)
