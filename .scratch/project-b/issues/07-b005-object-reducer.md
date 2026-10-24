# 07 — B-005: object reducer (wire → typed replicated state)

**What to build:** SMSG_UPDATE_OBJECT / COMPRESSED_UPDATE_OBJECT decode into GUID-keyed typed state using local UpdateFields.h (12340) + serializers + captures; create/update/destroy, visibility loss, value-block compression; GUID identity preserved separately from ECS entity ids. Purity: same ordered inputs → same state, no scheduling dependence. Byte fixtures + captured PKT replays as the test seams.

**Blocked by:** 06.

**Status:** ready-for-agent

- [ ] Update-field map from local header round-trips against captures
- [ ] GUID stability across despawn/recreate test
- [ ] Deterministic replay check (H2 precondition) green
