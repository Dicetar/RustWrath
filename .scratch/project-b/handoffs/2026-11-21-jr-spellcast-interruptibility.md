# Spellcast interruptibility edges

Status: ready for CI
Owner: ClaudeJr / Claude Opus 5.5
Branch: jr/spellcast-interruptibility
Issue: none
Next: Read CI for the pushed commit, then land after validation.

## Reference conclusion

12340 `SMSG_SPELL_START` and `SMSG_SPELL_GO` cast flags do not encode interruptibility. The
reference derives `CGUnit_C::IsCastNotInterruptible()` from the active cast marker, the cast
spell's `PreventionType == 1`, the known spellbook's interrupt/silence school masks, and aura
immunity effects. Aura changes recalculate the predicate and signal 334/335:
`UNIT_SPELLCAST_INTERRUPTIBLE` and `UNIT_SPELLCAST_NOT_INTERRUPTIBLE`.

## What changed

- `wrath_net_drain` recomputes the active cast state after world/aura updates and on native-only
  frames. It uses known spells with `SPELL_EFFECT_INTERRUPT_CAST` (68) and
  `SPELL_AURA_MOD_SILENCE` (27), then applies modeled effect/state/school/mechanic immunity aura
  effects (37/38/39/77).
- `WrathCastEventKind::InterruptibilityChanged` carries the edge through the existing scoped cast
  event queue.
- `ui_script/wrath_cast.rs` raises the two Wrath UI events only when the boolean changes.
- Existing `UnitCastingInfo` reads the resulting `WrathActiveCast::interruptible` value.

## Known limitation

The current Rust projection does not carry the reference's non-player special school masks or
separate cast-immunity fields. Those cases remain represented only by the modeled aura effects;
no START/GO flag is treated as an interruptibility producer. AzerothCore does not send a distinct
interruptibility packet in this path.

## Verification

- `git diff --check`: pass.
- Local Cargo, client launch and server launch: not run per ticket rules.
- CI: pending for the pushed commit.
- In-game verification: not proven.
