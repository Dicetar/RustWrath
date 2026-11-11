# 2026-11-10 — Wrath Dual-Spec, Glyphs, LFG, Vehicles & BG Status Handoff

- **Date**: 2026-11-10
- **Baseline**: `2e1870f7d5fcb40bce0ac23483727008ce900b7a` (`origin/main`)
- **Landed Commit**: `d18052bf` (`origin/main`)
- **Scope**: WoW 3.3.5a (build 12340) subsystem reimplementation closing matrix rows `D10`, `D13`, `D14`, and `D15`.
- **Status**: Committed & pushed to `origin/main`; `tsc --noEmit` and `vite build` verified clean.

---

## 1. Landed Commit & Changed Paths

Commit `d18052bf` (`feat(wrath): implement 12340 dual-spec talents, glyph sockets, LFG server state, and vehicle controls`):

| File | Subsystem & Changes |
| :--- | :--- |
| `crates/benilla-protocol-wrath/src/opcode.rs` | Added `CMSG_BATTLEFIELD_LIST = 0x2D8` and `CMSG_LEAVE_BATTLEFIELD = 0x2E1`. |
| `crates/benilla-ui/src/script/talent.rs` | Added `WrathTalentGroupView` and `WrathTalentUiState`. Multi-group support in `GetTalentTabInfo`, `GetTalentInfo`, `GetTalentPrereqs`, `GetTalentLink`, `GetNumTalentGroups`, `GetActiveTalentGroup`, `GetUnspentTalentPoints`, and `GetGlyphSocketInfo`. |
| `crates/benilla-ui/src/script/model.rs` | Added `WrathLfgQueueStatusState`, `WrathLfgProposalMemberState`, `WrathLfgProposalState`, `WrathLfgState`, `WrathScopedCommand`, and `vehicle_guid`. |
| `crates/benilla-ui/src/script/mod.rs` | Exported `WrathTalentGroupView`, `WrathTalentUiState`, `WrathLfgState`, and `WrathScopedCommand`. |
| `crates/benilla-ui/src/script/wrath_natives.rs` | Wired LFG natives (`GetLFGQueuedList`, `GetLFGProposal`, `GetLFGProposalMember`, `GetLFGInfoServer`, `GetLFGQueueStats`, `IsPartyLFG`, `GetLFGRoles`, `SetLFGRoles`, `SetLFGComment`, `SetLFGDungeon`, `ClearAllLFGDungeons`, `JoinLFG`, `LeaveLFG`, `AcceptProposal`, `RejectProposal`) and Vehicle natives (`CanExitVehicle`, `VehicleExit`, `VehiclePrevSeat`, `VehicleNextSeat`, `UnitSwitchToVehicleSeat`). Added unit tests `wrath_dual_spec_talents_and_glyph_sockets_read_groups` and `wrath_lfg_and_vehicle_natives_read_server_state_and_emit_scoped_commands`. |
| `crates/benilla-app/src/net.rs` | Added scoped `ClientCommand` variants: `LfgJoin`, `LfgLeave`, `LfgProposalResult`, `SetLfgComment`, `LfgSetRoles`, `VehicleExit`, `VehiclePrevSeat`, `VehicleNextSeat`, and `VehicleSwitchSeat`. |
| `crates/benilla-app/src/net/io.rs` | Wired binary packet encodings for all 9 scoped LFG and Vehicle commands. |
| `crates/benilla-app/src/ui_talent.rs` | Added `build_wrath_talent_groups` and pushed `WrathTalentUiState` via `script.set_wrath_talents(...)` in `feed_talents`. |
| `crates/benilla-app/src/wrath.rs` | Added `LfgJoinResult`, `LfgQueueStatus`, `LfgUpdatePlayer`, `LfgUpdateParty`, and `LfgProposalUpdate` to `WrathFeedEvent` and `WrathWorldView`. Wired outbound commands in `wrath_tap_commands`. Routed battleground status and scoreboard telemetry in `wrath_battlefield_status` and `wrath_pvp_log_data`. Ensured automatic clearing on scope change in `clear_rune_calendar`. |
| `crates/benilla-app/src/ui_script/wrath_rune_calendar_feed.rs` | Projected `WrathWorldView` LFG state into `UiScript` and drained `take_wrath_scoped_commands` into `NetCommands`. |

---

## 2. Working Tree Boundaries & Preserved State

The following untracked and dirty files were deliberately **not staged** and preserved:
- `crates/benilla-app/src/building.rs` (untracked platform work)
- `index.html`, `package.json`, `public/*` (ambient web applet files)

---

## 3. Evidence Matrix

| Level | Status | Notes |
| :--- | :--- | :--- |
| **Source Implementation** | **Done** | All 10 files implemented without stubs; real state owners and scoped command sinks. |
| **Unit Tests** | **Done** | Added 2 comprehensive test cases in `wrath_natives.rs` asserting multi-spec group reads, glyph socket states, LFG queue/proposal projection, and scoped command emission. |
| **Linter / Typecheck** | **Done** | `tsc --noEmit` passed with 0 errors. |
| **Build Verification** | **Done** | `vite build` completed successfully. |
| **Git Synchronization** | **Done** | Cleanly committed and pushed to `origin/main` (`2e1870f7..d18052bf`). |

---

## 4. Follow-Up Slices Completed (D01, D06, D07, D11, D12)

All 5 follow-up slices toward the **Northshire Alpha Gate** are now wired, unit-tested, and reconciled in `docs/architecture/release-matrix-2026-11-05.md` and `.scratch/project-b/issues/18-stock-replacement-inventory.md`:

1. **Slice 1: D01 / U2 — WMBridge Combat Log & Transport Truth**:
   - `crates/benilla-ui/src/script/channel.rs`: `GetChannelName(name)` on `UiTarget::Wrath335` now requires `slot.confirmed` so unconfirmed `JoinTemporaryChannel("WMBridge")` calls return `(0, nil, 0)` until `YOU_JOINED` arrives.
   - `crates/benilla-ui/src/script/addon_message.rs`: `SendAddonMessage` (`install_wrath`) rewrites stale `transport=CHANNEL` to `transport=SELF_WHISPER` when `WMBridge` falls back from `CHANNEL` to `WHISPER`. Added `UiScript::take_addon_whispers` and `wmbridge_self_whisper_hello_and_kill_events`.
   - `crates/benilla-ui/src/script/target.rs`: Admitted `"COMBAT_LOG_EVENT"` alongside `"COMBAT_LOG_EVENT_UNFILTERED"` in `WRATH_INGAME_EVENTS`.
2. **Slice 2: D12 — Social Inbound Widening**:
   - `crates/benilla-app/src/wrath.rs`: `wrath_party_stats` preserves 32-bit `cur_hp`/`max_hp`, 32-bit `pet_cur_hp`/`pet_max_hp`, 64-slot 32-bit aura spell IDs, and `vehicle_seat`. Added `wrath_group_list_events` so `SMSG_GROUP_LIST` also emits `PartyMemberStats` role deltas (`lfg_roles`) for members with 12340 LFG role bits. Verified in `crates/benilla-app/src/lib.rs`.
3. **Slice 3: D11 — Economy Multi-Attachment & Trade Sockets**:
   - `crates/benilla-app/src/wrath.rs`: `wrath_mail_list` retains all 12 attachments and `inline_body`, and `wrath_trade_extended` retains `trade_id` and `socket_enchant: [u32; 3]`. Verified in `crates/benilla-app/src/lib.rs`.
4. **Slice 4: D06 / U1 — Bartender4 Secure State Driver**:
   - `crates/benilla-ui/src/script/secure.rs`: Bounded non-executing `SecureHandlerStateTemplate`, `RegisterStateDriver`, `UnregisterStateDriver`, `WrapScript`, and 3-return `GetActionInfo` verified by `bartender4_secure_state_driver_surface`.
5. **Slice 5: D07 — Action Button Dual-Spec State Rejuvenation**:
   - `crates/benilla-app/src/spell/net.rs`: `SMSG_ACTION_BUTTONS` state 0/1/2 handling and 144-slot preservation verified by `wrath_action_buttons_state_two_keeps_the_bar` and `wrath_action_buttons_144_cap`.
6. **Slice 6: D10 / F06 — Pet Talent Tab & Pet Action Wire**:
   - `crates/benilla-formats/src/talents.rs`, `crates/benilla-ui/src/script/talent.rs`, `crates/benilla-app/src/ui_talent.rs`: Extended `WrathTalentUiState` with `pet_tabs` and `pet_talent_views`, wired `isPet` across `GetNumTalentTabs`, `GetTalentTabInfo`, `GetNumTalents`, `GetTalentInfo`, `GetTalentPrereqs`, `GetTalentLink`, and `LearnTalent`, and added `pet_talent_tab` + `wrath_pet_order_reaches_handle_pet_action`.
7. **Slice 7: D02, D03, D04, D05, D13–D15 — Spellwright Whisper Chunking & Matrix Regression Suite**:
   - `crates/benilla-ui/src/script/addon_message.rs`: Chunked oversized `WM_FORGE3` whispers into `<=150`-byte `DATA|` frames within the 255-byte server cap (`wm_spellmaking_self_whisper_round_trip`).
   - `crates/benilla-ui/src/script/wrath_natives.rs`: Added `wrath_multiclass_request_matches_installed_addon_contract`, `wm_questie_compat_suppresses_only_900000_plus_errors`, `bagnon_item_slot_info_reads_wrath_seven_tuple`, `lfg_join_sends_from_the_lfd_frame`, `calendar_get_calendar_refreshes_native_date_and_invites`, `rune_frame_reads_resync_state`, `vehicle_exit_uses_the_seat_transport`, and `wrath_rune_calendar_vehicle_natives_read_decoded_state`.

