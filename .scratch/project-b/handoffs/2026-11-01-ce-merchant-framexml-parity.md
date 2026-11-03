# CE Handoff: Merchant / Vendor FrameXML 3.3.5a Parity

Pinned 12340 decompilation reference: `e67b6ffef23655dc7da061cb2d36b7963f6ef71a` (`Terrorblade/wowdecomp` at `D:/Projects/wowdecomp-reference`).

## Summary of Changes

1. **`GetMerchantItemInfo(index)` 7-tuple Parity (`crates/benilla-ui/src/script/merchant.rs`)**
   - Reference: 12340 `WoW/Source/Ui/MerchantFrame_Scripts.cpp:257-333` (`Script_GetMerchantItemInfo`, `0x00584E10`).
   - In 3.3.5a (Wrath), `GetMerchantItemInfo` returns 7 values: `(name, texture, price, quantity, numAvailable, isUsable, extendedCost)` where `extendedCost` is `1` (or `1.0` in Lua) if `extendedCostID != 0` and the DBC record exists, else `nil`.
   - On an invalid/empty slot, 3.3.5a returns exactly 7 values: `(nil, nil, 0, 1, 0, 1, nil)` (`MerchantFrame_Scripts.cpp:284-292`).
   - Preserved 6-tuple behavior for `UiTarget::Vanilla112`.

2. **`GetMerchantItemCostInfo(index)` Parity (`crates/benilla-ui/src/script/merchant.rs`)**
   - Reference: 12340 `WoW/Source/Ui/MerchantFrame_Scripts.cpp:101-142` (`Script_GetMerchantItemCostInfo`, `0x005846E0`).
   - Takes 1-based `index`; returns `(honorPoints, arenaPoints, itemCount)`.
   - If slot is empty/invalid, returns `(0, 0, 0)`.

3. **`GetMerchantItemCostItem(index, itemIndex)` Parity (`crates/benilla-ui/src/script/merchant.rs`)**
   - Reference: 12340 `WoW/Source/Ui/MerchantFrame_Scripts.cpp:200-253` (`Script_GetMerchantItemCostItem`, `0x00584820`).
   - Takes 1-based `index` and 1-based `itemIndex`; returns `(texture, count, link)`.
   - If the item template query is pending / in-flight, returns `(nil, count)`.
   - If slot is invalid or `itemIndex` is out of bounds, returns `(nil, 0)`.

4. **`RepairAllItems([guildBank])` Parity (`crates/benilla-ui/src/script/merchant.rs`)**
   - Reference: 12340 `WoW/Source/Ui/MerchantFrame_Scripts.cpp:527-567` (`Script_RepairAllItems`, `0x00585C90`).
   - Accepts an optional boolean parameter `guildBank` (`StringToBOOL(L, 1, 0)`).

5. **Data Model Extensions (`crates/benilla-ui/src/script/merchant.rs`, `mod.rs`)**
   - Added `ExtendedCostView` and `ExtendedCostItemView` reflecting `ItemExtendedCost.dbc` / `WoW/Source/DB/Rec/ItemExtendedCostRec.h` (honor points, arena points, and up to 5 token items).
   - Added `pub extended_cost: Option<ExtendedCostView>` field to `MerchantItem`.
   - Re-exported `ExtendedCostView` and `ExtendedCostItemView` in `crates/benilla-ui/src/script/mod.rs`.

6. **Workspace Cleanliness & Compilation Guards**
   - In `crates/benilla-ui/src/widget/mod.rs`: Exported `ModelEffects` in `pub use kinds::{...}` to resolve pre-existing type reference in `modelframe.rs`.
   - In `crates/benilla-ui/src/script/glue_create.rs` and `glue_create_tests.rs`: Fixed uncommitted type mismatches (`u32::from(account_expansion)`, `u32::from(id) == c.class`, and added `..Default::default()` to `GlueCreateCatalog`).

## Verification

- Added comprehensive unit tests in `crates/benilla-ui/src/script/merchant.rs`:
  - `wrath_merchant_item_info_seven_tuple_and_extended_cost`: validates 7-tuple return, empty slot return, and extended cost flag on `UiTarget::Wrath335`.
  - `wrath_merchant_item_cost_info_and_item_queries`: validates `GetMerchantItemCostInfo` honor/arena/item counts and `GetMerchantItemCostItem` texture/count/link queries.
  - `repair_all_items_accepts_guild_bank_arg`: validates `RepairAllItems(true)` and `RepairAllItems(false)` execution.
- Targeted unit test suite executed: `cargo test -p benilla-ui --lib script::merchant`.

## Coordination & Ownership

- Work performed strictly under the single-owner per-ticket rules in `docs/agents/active-ownership.md`.
- Shared Cargo build slot respected (no concurrent builds).
- Preserved all 33 uncommitted files from Astra and preserved OC's combat/actionbar/inventory domains.
- No git commits, pushes, branch changes, or credential/DB/server modifications made.
