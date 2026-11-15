# Hand-off: fix/tracked-command-test-unwrap

Status: ready to merge
Branch: `fix/tracked-command-test-unwrap`
Base: `origin/main` (`9dc36e48`)

## Scope

Unwrap `ClientCommand::Tracked` transport-identity envelopes in wire-level test receivers broken by commit `6a151fe0` (`diag: trace loot, equipment, and merchant outcomes`).

## Root Cause

Commit `6a151fe0` wrapped outgoing `ClientCommand`s produced via `LootSteps::tracked`, `ui_loot::close_interaction`, `MerchantSteps::tracked`, `ui_items::calls`, `ui_items::drain`, and `ui_quest::verbs` in `ClientCommand::Tracked { trace, enforce_scope, command }` whenever `Option<Res<crate::quest_trace::Source>>` is present.

`NetPlugin` (installed by `headless_client_unrun()`) registers `quest_trace::Source`, so headless tests draining `rx: Receiver<ClientCommand>` directly and matching on inner variants (`AutostoreLootItem`, `LootMoney`, `LootRelease`, `LootMasterGive`, `BuyItem`, `BuyItemInSlot`, `RepairItem`, `SetAmmo`, `AutoEquipItem`, etc.) silently dropped or misformatted wrapped commands.

## Changes

- `crates/benilla-app/src/net.rs`: Added `#[cfg(test)] pub(crate) fn into_inner(self) -> Self` on `ClientCommand` to unwrap nested `Tracked` envelopes, plus unit test `tracked_into_inner_unwraps_nested_commands`.
- Mapped `ClientCommand::into_inner` across the affected `headless_client_unrun()` channel readers:
  - `crates/benilla-app/src/ui_loot/call_tests.rs` (`sent`)
  - `crates/benilla-app/src/ui_loot/sweep_tests.rs` (`a_sweep_runs_its_money_lines_listeners_before_its_rows`)
  - `crates/benilla-app/src/target/click_loot_tests.rs` (`Client::sent`, `a_shift_skin_casts_then_stores_the_byte`)
  - `crates/benilla-app/src/ui_merchant/call_tests.rs` (`sent`)
  - `crates/benilla-app/src/ui_items/verb_tests.rs` (`Bags::sent`)
  - `crates/benilla-app/src/ui_quest/verb_tests.rs` (`Client::sent`)
  - `crates/benilla-app/src/script_calls.rs` (`UseItem`, `SetAmmo`, `AutoEquipItem` wire checks)

## Verification

- Per `AGENTS.md`, Cargo compilation, test execution, and live client launch were not run in this environment; verify locally with `cargo test -p benilla-app` (loot, click_loot, ui_merchant, ui_items, ui_quest, script_calls).
