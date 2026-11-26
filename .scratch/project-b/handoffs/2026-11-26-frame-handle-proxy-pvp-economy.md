# Handoff: Restricted Frame-Handle Proxy, PvP/Arena Queue Getters & Economy UI Coverage

**Date:** 2026-11-26  
**Evidence status:** SOURCE-OBSERVED only. Cargo compilation, Rust unit tests, addon corpus re-run, and live client verification were **not run** in this environment (no `cargo`/`rustc`/`sccache` toolchain in container).

## Changed files

1. **Restricted Frame-Handle Proxy (`SecureHandlers.lua:108` follow-up)**
   - `crates/benilla-ui/src/script/wrath_security.rs`: Replaced the raw-frame identity passthrough with a true `newproxy`-backed userdata frame-handle proxy per live frame wrapper cached in weak registry tables (`benilla_wrath_frame_to_handle` / `benilla_wrath_handle_to_frame`) with `__metatable = false` and write protection.
     - `GetFrameHandle(frame_or_handle, create)` is idempotent on existing handles, honors `create == false` lookup-only mode, and rejects non-frames.
     - Added `GetFrameHandleFrame(handle)` and `GetManagedFrame(handle)`.
     - Routed restricted handle methods (`GetName`, `GetObjectType`, `IsObjectType`, `IsProtected`, `GetID`/`SetID`, `GetParent`/`SetParent`, `GetChildren`/`GetChildList`, `GetFrameRef`/`SetFrameRef`, `GetAttribute`/`SetAttribute`, `Show`/`Hide`/`IsShown`/`IsVisible`, `Enable`/`Disable`/`IsEnabled`, geometry/scale/alpha/strata/anchors, `CallMethod`, `Run`/`RunAttribute`/`RunFor`/`ChildUpdate`, override binding & auto-hide helpers) plus Rust helpers `resolve_frame_wrapper` and `run_restricted_body`.
     - Added focused tests `frame_handle_proxy_identity_caching_and_unwrapping` and `frame_handle_routes_methods_hierarchy_refs_and_restricted_execution` alongside the existing test.
   - `crates/benilla-ui/src/script/secure.rs`: Allowed global secure helpers (`SecureHandler*`, `RegisterStateDriver`, `UnregisterStateDriver`, `RegisterUnitWatch`, `UnregisterUnitWatch`) and `SetFrameRef`/`WrapScript`/`UnwrapScript` targets to accept either raw frame tables or restricted frame handles, and wired `SecureHandler:Execute` to run validated snippets via `wrath_security::run_restricted_body`.

2. **PvP / Arena Currency & Queue Getters**
   - `crates/benilla-formats/src/maps.rs`: Added `MapCatalog::battleground_maps()` returning sorted `InstanceType == 3` battleground rows with display names and battleground columns.
   - `crates/benilla-ui/src/script/model.rs` & `crates/benilla-ui/src/script/mod.rs`: Added and re-exported `WrathBattlegroundType` and `WrathWorldPvpQueueSlot` model fields and types.
   - `crates/benilla-ui/src/script/wrath_natives.rs`: Wired `GetNumArenaOpponents` (active `arena1`..`arena5` unit count), `GetNumBattlegroundTypes` & `GetBattlegroundInfo(index)` (`wrath_battleground_types`), `IsActiveBattlefieldArena` (active arena slot `(isArena, isRegistered)`), and `GetWorldPVPQueueStatus(index)` (`wrath_world_pvp_queues` 1..=2 with `("none", nil, nil)` default), plus `UiScript::set_wrath_battleground_types`, `UiScript::set_wrath_world_pvp_queue`, scope clears, and focused test `wrath_pvp_currency_battleground_types_world_pvp_and_arena_getters_read_real_state`.
   - `crates/benilla-app/src/ui_honor.rs`: Allowed `honor_snapshot` on Wrath when `WrathPlayerRaw` is present without waiting for 1.12 session kills, fired `HONOR_CURRENCY_UPDATE` when Wrath honor/arena currency changes, and added `wrath_honor_and_arena_currency_flow_from_raw_fields_and_fire_update`.
   - `crates/benilla-app/src/ui_battlefield.rs`: Projected `MapCatalog::battleground_maps()` into `set_wrath_battleground_types` per `(vm_session, player_level)` on Wrath VMs.

3. **Trade, Mail & Auction App-Level `ui_script` Coverage**
   - `crates/benilla-app/src/ui_script/economy_tests.rs`: Added `shipped_mail_send_tab_attaches_and_sends_through_the_frame`, `shipped_auction_bidder_and_owner_tabs_paint_their_lists`, and MPQ-free `wrath_trade_mail_and_auction_ui_contracts_run_without_mpq` (covering Trade slot-7 enchant/money/accept/close, Wrath 12-attachment inline-body Mail inbox/take/send, and Auction list/bidder/owner/bid/cancel).
   - Updated `.scratch/project-b/issues/18-stock-replacement-inventory.md` and `.scratch/project-b/handoffs/2026-11-25-wrath-era-and-addon-gate.md`.

## Recommended owner verification commands

- `cargo test -p benilla-ui wrath_security::tests`
- `cargo test -p benilla-ui secure::tests`
- `cargo test -p benilla-ui wrath_pvp_currency_battleground_types_world_pvp_and_arena_getters_read_real_state`
- `cargo test -p benilla-app wrath_honor_and_arena_currency_flow_from_raw_fields_and_fire_update`
- `cargo test -p benilla-app ui_script::economy_tests`
