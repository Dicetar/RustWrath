# Clippy: gate the test-only buyback slot and dense window

Status: ready to merge
Owner: Beam Free (first pass) / OpenCode DeepSeek overseer (gate)
Branch: oc/clippy-wrath-inventory
Issue: none
Next: land after green CI

## What changed

`WRATH_BUYBACK_SLOT_FIRST` is used only by the already `#[cfg(test)]` `InventoryAddr::to_wire`,
and `WrathFieldWindow::from_dense` is called only from test fixtures, so both are now
`#[cfg(test)]`. The unrelated blank-line churn from the first pass was dropped. No behavior
change.

## Checks

No local Cargo/build/test/client/server; CI is the compile authority.

## Not proven

Only the lint removal; no runtime change.
