# 2026-11-06 Handoff: Next Session Roadmap (Steps 3, 4, and 5)

## Baseline and Identity
- **Current Commit**: `1cc5e4fe` (on `origin/main` and `origin/wotlk`), merged with `b60362fb` (sccache config) and `f222a1f8` (Step 2 cast gate + pet/talent consumers).
- **Completed Step 1**: Reconciled and merged 3.3.5a build 12340 wire encoders/decoders, movement acks, DBC schemas, and sccache build accelerators (`31041480`).
- **Completed Step 2**: Completed lossless 12340 inventory pre-send cast gate readers in `ui_action::errors` & `spell::cast_send`, wired real state consumers for `PetSpellLearned`/`PetSpellRemoved`/`PetGuids` in `ui_pet` and `TalentInfo`/`TalentsInvoluntarilyReset` in `ui_talent`, and updated Issue 18 / RFC ledgers.
- **Reference Pin**: `D:/Projects/wowdecomp-reference` at `e67b6ffef23655dc7da061cb2d36b7963f6ef71a`.

---

## Step 3: Combat Log Event Dispatch & U1/U2 AzerothCore Parity

### Objective
Repair and complete the `COMBAT_LOG_EVENT_UNFILTERED` pipeline for 3.3.5a build 12340 to address the U1 review findings and support combat log addons (`WMBridge`, recount/combat log frames) without dropped events, source misattribution, or name lookup loss.

### Target Files
- `crates/benilla-app/src/ui_script/wrath_combat_log_feed.rs`
- `crates/benilla-app/src/wrath.rs`
- `crates/benilla-ui/src/script/`
- `.scratch/project-b/handoffs/2026-11-06-u1-supervisor-review.md`

### Tasks
1. **Source Attribution & Teammate Kill Integrity**:
   - Ensure `kill.killer` resolves against its own GUID rather than defaulting to the local player's name when a party member or pet secures the kill.
   - Verify `guid_hex` outputs the 18-character `0x` + 16 hex digit format expected by 12340 FrameXML.
2. **Ordered Event Queue vs. Overwritten Snapshots**:
   - Maintain the bounded sequence `world.party_kills: Vec<WrathPartyKill>` across frames so multiple kills arriving within one frame are all drained in order rather than truncated to a single latest option.
   - Clear pending events on scope retirement / world transfer (`world_generation` or `session_epoch` change).
3. **Bounded Name Lookup & Retries**:
   - Keep pending party kill events in the queue until `endpoint_name` resolves via `NameCache` or `PARTY_KILL_NAME_RETRY_SECONDS` elapses.
   - Track `party_kill_unresolved` and `party_kill_dropped` counters.
4. **Targeted Verification Cases**:
   - `cargo test -p benilla-app --lib ui_script::wrath_combat_log_feed::tests`
   - Test teammate party kill attribution (distinct killer GUID and name).
   - Test burst event delivery (2+ kills arriving before a single VM feed frame).

---

## Step 4: Addon Surface Parity (Bartender4 & WM_Multiclass D02/D06 Contracts)

### Objective
Unblock addon loading for installed 3.3.5a addons by providing the missing FrameXML script natives and secure action stubs identified in `.scratch/project-b/issues/18-stock-replacement-inventory.md` (Sections A & B).

### Target Files
- `crates/benilla-ui/src/script/action.rs`
- `crates/benilla-ui/src/script/object.rs`
- `crates/benilla-ui/src/script/object/attributes.rs`
- `crates/benilla-ui/src/script/wrath_spell.rs`
- `crates/benilla-app/src/wrath_natives.rs`

### Tasks
1. **`GetActionInfo` Arity Parity (D06 / Bartender4 blocker)**:
   - 3.3.5a stock contract returns 3 values: `actionType, id, subType` (where `subType` feeds `IsHelpfulSpell` in Bartender4 `actionbutton.lua:106`).
   - Update `crates/benilla-ui/src/script/action.rs` to push the third return (`subType`).
2. **`GetItemCount` Nil vs. 0 Parity**:
   - In 3.3.5a stock client, `GetItemCount` returns `0` rather than `nil` when an item is not found. Reconcile `crates/benilla-ui/src/script/item_stats/wrath.rs` to return `0` for stock bag and Bagnon callers.
3. **Secure State Driver Global & Widget Method Stubs**:
   - Provide safe implementations or explicit no-op registrations for `RegisterStateDriver`, `UnregisterStateDriver`, and `WrapScript` on secure frames to prevent Bartender4 Lua syntax exceptions during addon boot.
4. **Multiclass Request/Poll Host Contract (D02)**:
   - Ensure `MulticlassRequest` returns `1` and queues the caller's serial/args; ensure `MulticlassPoll` returns one hex packet or `nil`.
5. **Targeted Verification Cases**:
   - `cargo test -p benilla-ui --lib script::action::tests`
   - `cargo test -p benilla-ui --lib script::item_stats::tests`

---

## Step 5: Economy & Social Widening (D11 & D12)

### Objective
Migrate mail, trade, and social data models from 1.12 donor constraints to full 3.3.5a build 12340 wire structures, completing Section D & E of the Stock Replacement Inventory.

### Target Files
- `crates/benilla-app/src/economy.rs`
- `crates/benilla-app/src/ui_mail/`
- `crates/benilla-app/src/ui_trade/`
- `crates/benilla-app/src/ui_social/`
- `crates/benilla-app/src/social.rs`
- `crates/benilla-app/src/wrath.rs`

### Tasks
1. **Multi-Attachment Mail Support (D11)**:
   - 3.3.5a supports up to 12 mail attachments per letter (compared to 1 in donor 1.12).
   - Widen `MailListEntry` in `crates/benilla-app/src/economy.rs` and `ui_mail` to track the full attachment list and inline message body mapped at `wrath.rs:4974`.
2. **Trade Socket Enchant & 3.3.5 Trade ID (D11)**:
   - Widen `TradeStatusExtended` to include `trade_id`, and `TradeItem` to include the 3 socket words and enchant IDs.
   - Update `ui_trade/net.rs` to route socket enchant data to the trade UI.
3. **Social & Party Member Stats Widening (D12)**:
   - Replace u16 clamping on party member HP and pet HP with full 32-bit values in `crates/benilla-app/src/social.rs` and `ui_party/net.rs`.
   - Preserve 3.3.5a aura slot counts and LFG role flags in party structures.
4. **Targeted Verification Cases**:
   - `cargo test -p benilla-app --lib economy::tests::mail_list_reads_rows_bodies_and_attachments`
   - `cargo test -p benilla-app --lib social::tests::party_member_stats_full_reads_every_defined_bit`

---

## Rules and Execution Budget
- **Budget Ceiling**: Keep total targeted test cases across all verification runs <= 20 cases (`cargo test` with specific filter paths only; do not run broad workspace test sweeps).
- **Git Protocol**: Always stage explicit files (`git add <file1> <file2>`), never run `git add .` or `git add -A`. Keep `main` and `wotlk` branches in lockstep.
- **Diagnostics**: Run `compile_applet` and `lint_applet` together before closing the session turn.

---

## Completion Status (Steps 3, 4, and 5)
- **Step 3 Completed**: Mounted `ui_script::wrath_combat_log_feed` in `crates/benilla-app/src/ui_script/mod.rs` and wired `crates/benilla-app/src/wrath.rs` to use `crate::ui_script::wrath_combat_log_feed`, preserving distinct killer GUID/name attribution, ordered burst party kill queues across frames, and bounded `NameCache` retry windows.
- **Step 4 Completed**: Updated `GetActionInfo` in `crates/benilla-ui/src/script/action.rs` to return the 3rd value (`subType`: `"spell"`, `"pet"`, or `""`), updated `GetItemCount` in `crates/benilla-ui/src/script/item_stats/wrath.rs` to return `0` on miss instead of `nil`, and verified `RegisterStateDriver`/`UnregisterStateDriver`/`WrapScript` (`script/secure.rs`) and `MulticlassRequest`/`MulticlassPoll` (`script/wrath_natives.rs`).
- **Step 5 Completed**: Widened `MailListEntry` to retain up to 12 attachments (`MAX_MAIL_ITEMS = 12`) and inline body across `benilla-protocol`, `wrath.rs`, and `ui_mail`; widened `TradeItem` (`socket_enchant: [u32; 3]`) and `TradeStatusExtended` (`trade_id: u32`) with gem-socket link formatting in `ui_items` and `ui_trade`; and widened `PartyMemberStatsInfo` to 32-bit `cur_hp`/`max_hp`/`pet_cur_hp`/`pet_max_hp`, 32-bit aura spell IDs across all 64 slots (`AFLAG_NEGATIVE`), `vehicle_seat`, `lfg_roles`, and 3.3.5a assistant/subgroup flag packing in `wrath_group_list`.

