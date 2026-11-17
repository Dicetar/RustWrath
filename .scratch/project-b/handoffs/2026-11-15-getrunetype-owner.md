# GetRuneType asks for the rune owner before it reads its argument

Status: ready to merge
Owner: OpenCode / DeepSeek V4.1 Flash (overseer)
Branch: fix/wrath-getrunetype-owner
Issue: none
Next: land on source review; no client run is needed for this one

Based on `origin/main` `9dc36e48`. The change was found uncommitted in the integration checkout
(on the old `integration/autoswarm-waves-2026-11-05` tree) and ported here onto current `main`,
where the same code had the same ordering.

## What exists now

`crates/benilla-ui/src/script/wrath_natives.rs`: `GetRuneType` reads the model's rune state first
and answers no values when no player owns runes, before it validates the slot argument. The
reference, `Script_GetRuneType` (`ScriptEvents_Scripts.cpp:3035`-`:3048`, offset `0x006131E0`),
tests the active player first and returns 0 before its `lua_isnumber` usage error, so a
non-numeric slot is not an error while the player is absent or has not resynced runes.

The existing startup-natives test gains that case: `GetRuneType('x')` answers zero values with no
rune state, where the old order raised `Usage: GetRuneType(slot)`.

## Checks actually run

Pinned toolchain 1.98.1, `CARGO_INCREMENTAL=0`.

| Check | Result |
|---|---|
| `cargo test -p benilla-ui --lib wrath_framexml_startup_natives_and_roles_are_defined` | 1 passed |
| `cargo test -p benilla-ui --lib rune` | 3 passed (resync state, conversion and power all still read) |

## Not done, not proven

- No client run. The rune frame reads through `GetRuneType` with a numeric slot, which this change
  does not alter; only the no-owner ordering moved.
- The workspace `fmt`/`clippy -D warnings` gates are red on `main` for debt outside this file, as
  recorded in `2026-11-15-overseer-obvious.md`.
