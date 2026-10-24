# 07 — B-005: object reducer (wire → typed replicated state)

**What to build:** SMSG_UPDATE_OBJECT / COMPRESSED_UPDATE_OBJECT decode into GUID-keyed typed state using local UpdateFields.h (12340) + serializers + captures; create/update/destroy, visibility loss, value-block compression; GUID identity preserved separately from ECS entity ids. Purity: same ordered inputs → same state, no scheduling dependence. Byte fixtures + captured PKT replays as the test seams.

**Blocked by:** 06.

**Status:** DONE 2026-10-25 — wire parser + pure reducer + 381 generated field constants; 15 unit tests + live-fixture replay green (fixtures/login-burst.bin: 1350 live packets incl. compressed updates and spline creates; deterministic x3). Live-taught corrections: 0x1F6 = [u32 size][zlib(update payload)] with NO inner opcode; spline create data per MovementPacketBuilder.

- [x] Update-field map from local header round-trips against captures
- [x] GUID stability across despawn/recreate test
- [x] Deterministic replay check (H2 precondition) green
