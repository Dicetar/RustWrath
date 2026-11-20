# Clippy: remove dead items in wrath_inventory.rs

Status: ready to merge
Owner: Beam Free
Branch: oc/clippy-wrath-inventory
Issue: none
Next: land after green CI

## What changed

- `crates/benilla-app/src/wrath_inventory.rs`: removed or test-gated dead items flagged by clippy,
  with no behavior change and no `#[allow]`.
  - Removed unused `WrathFieldWindow::len`, `is_empty`, and `player`.
  - Removed unused `InventoryChanges::is_empty`.
  - Gated `WrathInventory::scope` and `bank_bags_purchased` (methods) with `#[cfg(test)]`.
  - Removed unused `WrathInventory::iter`.
  - Gated `InventoryAddr::to_wire`, `as_bag_byte`, and `lua_bag_id` with `#[cfg(test)]`.
  - Removed unused `InventoryAddr::label`.
  - Removed unused `PlayerInventory::store`, `wrath`, `buyback`, and `bank_bags_purchased`.
  - Gated `WRATH_BUYBACK_SLOT_FIRST` and the `BAG_PLAYER_INVENTORY`/slot imports with `#[cfg(test)]`.

## Kept

- `WrathFieldWindow::from_dense`, `new`, `is_delivered`, `get`, `word_or_zero`, `guid`,
  `from_fields`; all slot accessors and capacities on `PlayerInventory`; constants and fields used
  by production code.
- Serde/attribute-referenced helpers were not affected.

## Checks not run

- No Cargo, no build, no clippy run, no tests, no client/server (per task).

## What is not proven

- No local compilation or clippy confirmation; CI must validate the fix and that no dependent code
  relied on the removed or test-gated items.
- No runtime/behavioral verification; the change removes only unused APIs and is intended to be
  behavior-neutral.
