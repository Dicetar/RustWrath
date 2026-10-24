# 06 — B-003: renderless 3.3.5 session against lab AzerothCore

**What to build:** authenticate → realm → world session → character enum → enter world → 5-minute stable session → clean logout/reconnect, headless, in benilla-srp/benilla-protocol lineage. Directional ARC4-drop1024 header crypto per local AuthCrypt; digest per local WorldSocket; bad credentials fail cleanly; partial reads, multi-packet reads, normal/large headers, compressed update bounds checked. Decoder layer is time-free (arrival metadata passed explicitly). Lab isolation: separate ports/DB/accounts (ProjectB-Lab proposal from 01).

**Blocked by:** 04.

**Status:** session stack PROVEN live end-to-end 2026-10-24 (AUTH_OK, burst, enum, login, 20 s dwell, clean logout; commit a9938208) — formal closure on the 5-minute + reconnect evidence now running. Realm address connect + opcode-ledger row updates land with the reducer ticket.

- [ ] Opcode ledger rows implemented status updated as messages land
- [ ] 5-minute session + logout/reconnect against lab AC
- [ ] Zero wall-clock reads below the reducer boundary
