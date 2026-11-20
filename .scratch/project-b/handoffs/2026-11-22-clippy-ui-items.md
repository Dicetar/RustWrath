Status: ready to merge
Owner: Beam Free
Branch: oc/clippy-ui-items
Issue: none
Next: land after CI

## What changed

- `crates/benilla-app/src/ui_items/mod.rs`: removed dead donor wrappers with no production or test consumer:
  - `action_count_in_target` (inlined into `action_count`)
  - `collect_inventory`
  - `object_name`
  - `object_link`
  - `object_link_in_target`
- Fixed the unnecessary `mut` on the test-local `objects` binding.

No behavior change: the removed wrappers only delegated to their `*_in_player` / `*_item` / `*_in_target_item` counterparts, and `action_count` now performs the same donor-only projection inline.

## Findings removed vs kept

Removed (no real consumer beyond their own definition):
- line 563 `action_count_in_target` (only called by the now-inlined `action_count`)
- line 695 `collect_inventory` (only self-referential)
- line 1119 `object_name` (only self-referential; consumers use `object_name_item`)
- line 1181 `object_link` (only self-referential; consumers use `object_link_item`)
- line 1221 `object_link_in_target` (only self-referential; consumers use `object_link_in_target_item`)
- line 1554 mutable variable

Kept (test-referenced public-ish API; not deleted per task rule):
- line 110 `wire_pos`: referenced by tests in this module.
- line 505 `count_of`: referenced by tests in `spell/usable.rs`.
- line 554 `action_count`: referenced by tests in this module.
- line 595 `stack_counts`: referenced by tests in `spell/usable.rs`.

## Checks not run

No Cargo, build, client or server runs (per owner policy). Compilation/tests are left to CI.

## What is not proven

Whether the kept test-only donor wrappers still trip clippy in a lib-only build; CI's target set and clippy configuration were not available locally.
