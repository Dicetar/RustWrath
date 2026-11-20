Status: ready to merge
Owner: DeepSeek V4.1 Flash Free (opencode/deepseek-v4.1-flash-free)
Branch: oc/clippy-ui-bind
Issue: none
Next: land after green CI

# Clippy dead-code: macro bind `derive`

## What changed

- `crates/benilla-ui/src/script/macros/bind.rs`:
  - `derive` (the `SpellBookState`-based wrapper) now carries `#[cfg(test)]`. It is referenced
    only from the file's `#[cfg(test)] mod tests` (`bound`, the `derived` closure and the
    pet-book test); every live caller in `crates/benilla-ui/src/script/macros.rs` (lines 437,
    575, 630) uses the slot-slice form `derive_with_slots`, so the non-test build had no caller
    and clippy reported `function \`derive\` is never used` (run 44770772111, finding 98).
  - The now test-only `SpellBookState` import was split into its own `#[cfg(test)] use` so the
    non-test build does not gain an unused-import warning. `PetBookState` stays unconditional
    because `derive_with_slots` uses it.
  - Doc text on `derive` records that it is the test-suite form; signature, body and every test
    are unchanged. No `#[allow]`, no behavior change.

## Checks not run

- No Cargo, build, clippy, test, client or server run locally, per task constraints and the
  project workflow. CI compiles and tests this branch; that is the only compilation evidence.

## What is not proven

- That CI clippy is clean for this file beyond the reported finding; only finding 98 was
  addressed and no local lint was executed.
- That other files in the crate hold no further dead-code findings; out of scope for this task.
