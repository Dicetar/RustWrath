# Wrath combat log: SWING_MISSED absorb and resist

Status: ready to merge
Owner: OpenCode / DeepSeek V4.1 Flash Free
Branch: oc/swing-missed-absorb-resist
Issue: 18 (first-loop behavior C06), 25
Next: none; the spell-event families are the next slice.

`MissKind` gains `Absorb`/`Resist`. `swing_outcome` tests `0x20` before `0x80` after the `0x10`
miss bit, so a row with both is ABSORB; the damage row, victim-state switch, header, names, retry
and `ShouldShowFeedback` are unchanged. `swing_missed_args` appends absorbed, resisted or blocked
for ABSORB/RESIST/BLOCK (zero included) and nothing for the other kinds.
Reference: `UnitCombatLog_C.cpp::UnitCombatLog` (0x007512B0) 1009-1035; `SetMissInfo`
(0x0074D980, infoFlags 0x2000/0x4000); `PushEvent` (0x0074E290) tenth value.
Tests: `swing_missed_absorb_and_resist_carry_their_totals` and
`a_row_with_absorb_and_resist_is_absorb`; `swing_missed_names_each_victim_state` now expects
ABSORB/RESIST where it asserted retirement. CI: not run yet.
Not proven in game: that a live absorbed/resisted swing reaches the stock frame with the summed
total, or that the server sends both bits with the values the reader sums.
