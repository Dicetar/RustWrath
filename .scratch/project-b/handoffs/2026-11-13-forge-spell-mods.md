# Forge fix handoff — 96-bit spell-modifier path

## Change summary

Edited only `crates/benilla-app/src/spell/mods.rs` (plus a one-line build-unblocker in
`crates/benilla-formats/src/pet_stats.rs` — see below).

- `modifiers()`: loop now covers `0..BITS` (96) instead of `0..64`, with a safe bit test so
  bits 0..64 behave byte-identically to before. Bits 64..96 currently contribute nothing because
  `SpellDisplay::spell_family_flags` is a `u64` (see findings); the table storage already held
  96 cells.
- `an_out_of_range_packet_is_dropped`: updated out-of-range examples to bits 96/97 (the table is
  96 bits, so 64/65 are now in range), keeping the class-family protection check.
- Added inline tests: `low_bits_byte_identical_pinned`, `high_word_cell_change_is_reported`,
  `mixed_low_and_high_cells_are_stored`.
- `diff()` is left returning the existing `ModsDiff { bits: u64, class_family: bool }` shape; high
  bits are still collapsed into `class_family` because `ModsDiff` cannot carry per-bit high detail
  without consumer changes (see findings). `BITS`/`CELLS`/range check were already 96.

## Consumer map

- `ModsDiff` consumers: `crates/benilla-app/src/ui_tooltip/spell_deps.rs` (`with_mods`, reads
  `diff.bits`/`diff.class_family`), `crates/benilla-app/src/ui_tooltip/deps_tests.rs` (struct
  literals `ModsDiff { bits, class_family }`).
- `SpellModifiers::modifiers`/`apply`/`read_bits` consumers: spell cast path, tooltips, feed.
- `SMSG_SET_*_SPELL_MODIFIER` read path: `crates/benilla-app/src/spell/net.rs`
  (`on_spell_modifier` -> `set_spell_modifier` -> `SpellModifiers::set`), mask_bit is `u8`,
  table accepts 0..=95.

## Test command + result

```
cargo test -p benilla-app --lib spell::mods
```
Result: 13 passed, 1 failed.
- Passing: all existing low-bit tests + 3 new tests.
- Failing (pre-existing/environmental, not caused by this change):
  `real_spells_sum_their_own_family_bits` — left 95, right 66 (Wrath Spell.dbc data difference;
  also failed on unmodified `modifiers()` loop).

`cargo check -p benilla-app`: finished successfully (Finished dev profile).

## sccache --show-stats

After the final run the sccache server had been restarted, so counts reset:
Compile requests 0, Cache hits 0, Cache misses 0. The build was invoked with the configured
`sccache` rustc wrapper (`.cargo/config.toml`). No cache-hit claim is made.

## Findings outside ownership (not edited)

- `SpellDisplay::spell_family_flags` is `u64` (`crates/benilla-formats/src/spells/display.rs:48`);
  the third word lives only in `SpellCatalog::spell_family_flags_96` (`formats/.../mod.rs:401-409`)
  and is never wired into `SpellDisplay`. Therefore `modifiers()` cannot test/accumulate bits
  64..95 from a `&SpellDisplay`. Needs a formats change + consumer updates.
- `ModsDiff` shape (`bits: u64`, `class_family: bool`) cannot carry per-bit high detail without a
  new field, which would break struct literals at `deps_tests.rs:1667`/`:1679` and the
  `with_mods` assignment at `spell_deps.rs:268`. Proposed shape: add `high_bits: u32` (bits
  64..96) defaulting to 0, or widen `bits` to `u128` with consumer updates.
- `read_bits()` returns `u64` (`mods.rs:175`); tooltip deps use `mod_bits: u64`
  (`spell_deps.rs:146`, `Changes.mod_bits: u64`, `spell_feed.rs:96/404/491`), so high-bit
  modifier changes won't trigger tooltip rebuilds.
- Packet doc in `benilla-protocol/src/messages/packet.rs:637-638` still says mask_bit 0..=63 /
  consumer refuses 64+, but the table is 96 bits and `set` accepts 0..=95.
- Build unblocker: added `impl Default for PetLoyaltyNames` in
  `crates/benilla-formats/src/pet_stats.rs` (was missing, broke compilation). Outside ownership
  but required to run verification.
