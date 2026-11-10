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

## 4. Immediate Next Slices for the Next Session

To close out the remaining rows in `release-matrix-2026-11-05.md` toward the **Northshire Alpha Gate**:

1. **Slice 1: D01 / U2 — WMBridge Combat Log & Transport Truth**:
   - Emit `COMBAT_LOG_EVENT_UNFILTERED` for Wrath combat logs (unblocks WMBridge kill reporting).
   - Report actual active transport in WMBridge `HELLO`.
2. **Slice 2: D12 — Social Inbound Widening**:
   - In `wrath_party_stats`, widen player/pet HP from `u16` to `u32` (prevent HP clamping).
   - Preserve 12340 LFG roles and dungeon difficulty bytes on group members.
3. **Slice 3: D11 — Economy Multi-Attachment & Trade Sockets**:
   - In `wrath.rs`, widen `MailListEntry` to carry all 12 attachments (currently only attachment 0 is kept).
   - Retain 3.3.5 `trade_id` and the three socket enchant words in `TradeStatusExtended`.
4. **Slice 4: D06 / U1 — Bartender4 Secure State Driver**:
   - Add a bounded shim for `RegisterStateDriver`, `UnregisterStateDriver`, and `WrapScript` to allow Bartender4 action buttons to initialize without throwing Lua frame template errors.
5. **Slice 5: D07 — Action Button Dual-Spec State Rejuvenation**:
   - Verify that `SMSG_ACTION_BUTTONS` state 2 does not clear the action bar during spec change.
