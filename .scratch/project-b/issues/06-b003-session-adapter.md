# 06 — B-003: renderless 3.3.5 session against lab AzerothCore

**What to build:** authenticate → realm → world session → character enum → enter world → 5-minute stable session → clean logout/reconnect, headless, in benilla-srp/benilla-protocol lineage. Directional ARC4-drop1024 header crypto per local AuthCrypt; digest per local WorldSocket; bad credentials fail cleanly; partial reads, multi-packet reads, normal/large headers, compressed update bounds checked. Decoder layer is time-free (arrival metadata passed explicitly). Lab isolation: separate ports/DB/accounts (ProjectB-Lab proposal from 01).

**Blocked by:** 04.

**Status:** DONE 2026-10-24 — full acceptance evidenced: 5-minute dwell + clean LOGOUT_COMPLETE + immediate reconnect exit 0 (transcript in session log; commits a9938208, 9fdd0158). Bad-credentials clean-failure case and large/compressed-packet fixtures carry into ticket 07 (reducer) where captured streams exist. Realm-address connect from the realm list lands with the first non-lab target.

- [x] Opcode ledger rows implemented status updated as messages land
- [x] 5-minute session + logout/reconnect against lab AC
- [x] Zero wall-clock reads below the reducer boundary
