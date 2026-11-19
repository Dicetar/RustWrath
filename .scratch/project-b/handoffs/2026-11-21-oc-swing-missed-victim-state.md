# Wrath combat log: SWING_MISSED (miss and victim states)

Status: ready to merge
Owner: OpenCode / Particle-1 (overseer formatted and pushed after the free model hit its rate limit)
Branch: oc/swing-missed-victim-state
Issue: 18 (first-loop behavior C06), 25
Next: none; the absorb/resist slice is next.

One `swing_outcome` now replaces the SWING_DAMAGE-only predicate, in 12340's order:
ShouldShowFeedback, the wound/block-with-damage row, `hit_info & 0x10` (MISS), absorb (0x20) and
resist (0x80) retired, then the victim-state switch (2 DODGE, 3 PARRY, 4 BLOCK, 6 EVADE, 7 IMMUNE,
8 DEFLECT); anything else retires. `SWING_MISSED` posts the eight header values, the miss string,
and the blocked total only for BLOCK. Names, retry and UNKNOWNOBJECT stay as they were.

Reference: `UnitCombatLog_C.cpp::UnitCombatLog` 0x007512B0 lines 933-1073,
`CombatLogEntry::SetMissInfo`/`PushEvent` (infoFlags 0x4/0x4000/0x8000), `g_spellMissStrings`.

Tests: `swing_missed_names_each_victim_state`, `feed_delivers_a_dodged_swing_as_swing_missed`,
`a_zero_damage_block_is_swing_missed_block_with_the_blocked_amount`,
`a_missed_melee_spell_round_is_dropped_by_should_show_feedback`; updated
`feed_delivers_a_wounded_swing_and_silently_retires_the_rest` (a dodge is no longer retired).

CI 44613104542: Windows check and the changed-crate tests green; the gates job is red only on the pre-existing `player-clippy` debt in files this ticket did not touch. No Cargo, client or server run. Not proven in game: that a live dodge/parry
reaches the stock combat-log frame, or that a blocked row with no damage is spelled 0.0 (the
reference's amount, pushed as a number, not nil).
