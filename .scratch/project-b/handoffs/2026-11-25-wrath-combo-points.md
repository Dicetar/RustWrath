# Wrath 3.3.5a (`12340`) combo point packet, descriptor projection, and UI wiring

- Status: ready for workstation cargo check/test
- Owner: cloud
- Branch: main
- Issue: #18
- Next: workstation `cargo test` on `benilla-protocol-wrath`, `benilla-ui`, and `benilla-app` (`update_combo_points_reads_packed_target_and_count`, `wrath_get_combo_points_accepts_unit_and_target_tokens`, `combo_points_update_marks_dirty_and_clears_on_scope_reset`)

## 12340 & AzerothCore Reference Anchors

- `SMSG_UPDATE_COMBO_POINTS = 0x39D` (`Opcodes.h:955`)
- AzerothCore `Player::SendComboPoints` / `Player::ClearComboPoints`: `[PackedGuid target][u8 points]` (cleared state sends `[0x00, 0x00]`)
- `12340` `Packet_SMSG_UPDATE_COMBO_POINTS`: updates local combo target GUID and combo point count, then fires `UNIT_COMBO_POINTS` (with unit token) and `PLAYER_COMBO_POINTS`
- `12340` `Script_GetComboPoints` (`0x00611670`) -> `CGGameUI::GetComboPoints` (`0x00513920`): accepts optional `(unit, target)` tokens (`"player"` / `"vehicle"`, defaulting target to `"target"`)

## Summary of Changes

1. **`crates/benilla-protocol-wrath`**:
   - Defined `SMSG_UPDATE_COMBO_POINTS = 0x39D` in `src/opcode.rs` and added it to the AzerothCore opcode table assertion test.
   - Added `ComboPointsUpdate { target: u64, points: u8 }` and `read_update_combo_points` in `src/gameplay/wrath_systems.rs` with unit coverage for packed target + count, cleared `[0x00, 0x00]`, truncated prefix rejection, and trailing byte rejection.
   - Wired `SMSG_UPDATE_COMBO_POINTS` under `DecoderFamily::WrathSystems` and `ServerEvent::ComboPoints` in `src/gameplay.rs`.
2. **`crates/benilla-app`**:
   - Added `WrathFeedEvent::ComboPoints` and `WrathWorldView::{combo_points, combo_points_dirty}` in `src/wrath.rs`, resetting state on world scope resets (`clear_rune_calendar`) and projecting `(points, target)` into the self player's synthesized `ObjectFields` (`FIELD_PLAYER_FIELD_BYTES` `1222` byte 1 and `FIELD_PLAYER_FIELD_COMBO_TARGET` `714..716`) during `wrath_apply_world`.
   - Emitted `FieldChanged` on `1222` / `714..716` and `FieldNotice::ComboPoints` so finisher usability (`src/spell/usable.rs:420`), finisher cast validation (`src/spell/cast_send.rs:915`), action-bar reactivity (`src/ui_action/state.rs:113`), and UI model sync (`src/ui_unit.rs:1849`) all update immediately even when `SMSG_UPDATE_COMBO_POINTS` arrives without a same-frame `SMSG_UPDATE_OBJECT`.
   - Updated `FieldNotice::ComboPoints` in `src/ui_unit/field_events.rs` to fire `UNIT_COMBO_POINTS` (`"player"`, plus `"vehicle"` when a vehicle action bar is active) alongside `PLAYER_COMBO_POINTS` on `UiTarget::Wrath335`.
3. **`crates/benilla-ui`**:
   - Updated `GetComboPoints` in `src/script/unit/bindings.rs` to support Wrath 3.3.5a 0-, 1-, and 2-argument `(unit, target)` signatures when `model.is_wrath()` is active while keeping 1.12 donor semantics intact.
   - Added `wrath_get_combo_points_accepts_unit_and_target_tokens` in `src/script/wrath_natives.rs`.

## Verification

- Compilation and Cargo tests: **not run** (cloud container has no `cargo` / `rustc` / `sccache` toolchain or game data).
