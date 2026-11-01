# Argon checkpoint: 3.3.5a bindings, WMO liquid flags, Warrior stance/bytes2, and SpellGo slot release

Pinned 12340 decompilation reference: `e67b6ffef23655dc7da061cb2d36b7963f6ef71a` (`Terrorblade/wowdecomp`).

## Changes applied

1. **3.3.5a `Bindings.xml` movement and posture verbs (`crates/benilla-ui/src/script/input_verbs.rs`, `wrath_natives.rs`)**
   - Stock 3.3.5a `Bindings.xml` binds `Space` (`JUMP`) to `JumpOrAscendStart()` / `AscendStop()`, `X` (`SITORSTAND`) to `SitStandOrDescendStart()` / `DescendStop()`, and `MOVEANDSTEER` to `MoveAndSteerStart()` / `MoveAndSteerStop()`.
   - Registered these six verbs on `UiTarget::Wrath335` per 12340 `InputControl_Scripts.cpp` (`0x005FBF80`, `0x005FC0A0`, `0x005FC140`, `0x005FC780`, `0x005FC840`) and `GameUI_Scripts.cpp` (`0x0051B1D0`), preserving the 1.12 global table on `UiTarget::Vanilla112`.

2. **Dry WMO interior swimming fix (`crates/benilla-formats/src/models/wmo/root.rs`, `group.rs`, `crates/benilla-assets/src/wmo.rs`, `crates/benilla-world/src/terrain_stream/spawn/mod.rs`)**
   - In 12340 `CMapObjGroup::Create` (`MapObjRead.cpp:523`, `0x007D82E0`) and `CMapObjGroup::GetLiquidType` (`MapObjGroup.cpp:1568`, `0x007D7310`), when `MOHD.flags & 0x4` (`SMOHeader::m_flags` at `+0x3c`) is set, `MOGP.groupLiquid == 0` means **no liquid** (`m_liquidType = 0`), whereas in legacy roots (`MOHD.flags & 0x4 == 0`), `15` (`0xf`) means no liquid and `0` maps to water (`13`/`14`).
   - Furthermore, in `CMapObjGroup::QueryLiquid` (`MapObjGroup.cpp:1218`, `0x007C8360`), whole-group submersion (`*level = 3.4028235e38f`, `f32::MAX`) only applies when `m_liquidType != 0` **and** `!m_liquidTiles.x || !m_liquidTiles.y` (the group has no `MLIQ` grid).
   - Added `WmoRoot::use_liquid_type_dbc_id`, `WmoRoot::resolve_group_liquid`, and `wmo_group_liquid_mesh_for_root`, and gated `WmoPortalInstance::flooded` on the absence of `group_liquids[gi]`.

3. **Wrath `UNIT_FIELD_BYTES_1` & `UNIT_FIELD_BYTES_2` synthesis (`crates/benilla-app/src/wrath.rs`)**
   - In 12340, `CGUnit_C::GetShapeshiftForm` (`Unit_C.cpp:6652`) reads `UNIT_FIELD_BYTES_2[3]` (Wrath field `122` byte 3, where Warrior Battle Stance `17` lives) and `UnitVisFlagUpdateHandler` (`Unit_C.cpp:5128`) reads `UNIT_FIELD_BYTES_1[2]` (Wrath field `74` byte 2).
   - Added `bytes_2` (`UNIT_FIELD_BYTES_2`) to `WrathEntity`, projected it onto 1.12 `FIELD_UNIT_BYTES_2` (`164`, restoring `unit_sheath_state()`), and synthesized 1.12 `FIELD_UNIT_BYTES_1` (`138`: `[stand_state, pet_talents, shapeshift_form, vis_flags]`).
   - Added `FieldChanged` and `FieldNotice::Shapeshift` / `PLAYER_REGEN_DISABLED` / `PLAYER_REGEN_ENABLED` emission in `wrath_apply_world` when `FIELD_UNIT_BYTES_1` (`138`), `FIELD_UNIT_BYTES_2` (`164`), or `FIELD_UNIT_FLAGS` (`46`) change, unblocking `GetBonusBarOffset()`, Warrior stance bonus bar paging, and stance-gated spells.

4. **Wrath `SMSG_SPELL_GO` in-flight cast release & transfer cleanup (`crates/benilla-app/src/wrath.rs`)**
   - In 1.12, `SMSG_CAST_RESULT` (`0x130`) was sent on both success and failure and popped `PendingCast` (`spell/net.rs:1025`). In 3.3.5a (`12340`), `0x130` is `SMSG_CAST_FAILED` (sent on failure only), while a successful self cast goes directly to `SMSG_SPELL_GO` (`0x132`).
   - Synthesized `SessionEvent::CastResult { spell_id, success: true, reason: None, arg: None }` ahead of `SessionEvent::SpellGo` when `caster == self_guid`, and cleared `PendingCast`, `QueuedMeleeSpell`, `ActiveChannel`, and `AutoRepeatActive` on `changes.world_transferred`.

5. **Hotpatch pose pass root-motion guard (`crates/benilla-hotpatch-bevy/src/lib.rs`)**
   - Extended `apply_pose` to evaluate non-parked rigs when `rig.has_billboard` or `roots_changed.contains(rig.joints_root)` so `finalize_rig_worlds` (`compose.rs:157`) does not compose unpatched locals restored by `Last`'s `restore_pose` when a static rig moves in world space.
