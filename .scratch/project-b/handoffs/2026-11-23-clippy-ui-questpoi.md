# Handoff: clippy fix in crates/benilla-ui/src/script/questpoiframe.rs

Status: ready to merge
Owner: Big Pickle (opencode/big-pickle)
Branch: oc/clippy-ui-questpoi (checked out from origin/main)
Issue: none
Next: land after green CI

## What changed

One source file only: `crates/benilla-ui/src/script/questpoiframe.rs`. This also adds the task note below; no other paths were touched.

Clippy finding from main run 44770772111, line 254: manual `!RangeInclusive::contains` implementation. In the `GetTooltipIndex` Lua binding, the zero-based index guard was:

```rust
let i = num(lua, i) as i64 - 1;
with_poi(lua, &this, |st| {
    if i > 3 || i < 0 {
        0
    } else {
        ...
    }
})
```

Replaced with clippy's equivalent idiomatic form:

```rust
    if !(0..=3).contains(&i) {
```

`i` is an `i64`, so the inferred `RangeInclusive<i64>` gives `!(0 <= i && i <= 3)`, which is exactly the original `i > 3 || i < 0` (same result for every value, including negatives and overflow-saturated casts). No behavior change, no `#[allow]`.

The other guidance carried with the task does not apply to this file: there is no `isolate_lowest_one` bit idiom and no `args.get(0)` here, so nothing was changed for those. The `mouse_over` UV guard already used `(0.0..=1.0).contains(&x)` / `(&y)` and was left alone.

## Checks not run

No Cargo check/build/test or clippy run, and no client or server launch, per the session's explicit no-build/no-launch rule. CI compiles and tests the pushed branch; its result is not asserted here.

## What is not proven

Compilation and the absence of remaining clippy findings are not verified locally. Behavioral equivalence of the guard is by source inspection only; no focused Lua case was run in this session.
