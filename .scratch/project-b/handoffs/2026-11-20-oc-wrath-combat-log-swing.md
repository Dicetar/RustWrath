# Wrath combat log: SWING_DAMAGE

Status: landed
Owner: OpenCode / DeepSeek 4.1 (fallback after the free model's usage limit)
Branch: oc/wrath-combat-log-swing
Issue: 18 (first-loop behavior C06), 25 (combat-log gap)
Next: none; landed on main as 5fb029bd. SWING_MISSED is the named next slice.

## What changed

- `benilla-protocol-wrath`: `SMSG_ATTACKERSTATEUPDATE` keeps the 12340 `Overkill` word it
  already read and dropped. `ServerEvent::AttackerState` now carries `overkill: u32`.
- `WrathWorldView`: a bounded `swings` queue beside `party_kills`, with its own monotonic
  revision, scope tags, clear and overflow accounting. `WrathFeedEvent::AttackerState` carries
  the raw 12340 hit word, school mask and victim state; it never reuses the 1.12 translated
  values (`vanilla_hit_info`/`vanilla_school`).
- `wrath_combat_log_feed`: after the party-kill loop, drains the swing queue and posts
  `COMBAT_LOG_EVENT_UNFILTERED` / `SWING_DAMAGE` with the eight header values plus damage,
  overkill, schoolMask, resisted, blocked, absorbed, critical, glancing and crushing. Endpoint
  names use the existing helper and the same retry/UNKNOWNOBJECT policy as kills.
- Only victim state WOUND (1) or BLOCK (4) with non-zero damage emits. Every other row retires
  silently for now, including misses, dodges, parries and intact hits.

Reference: `UnitCombatLog_C.cpp::UnitCombatLog` (event index 1, VS_WOUND/VS_BLOCK, non-zero
damage) and `CombatLogEntry::PushEvent` (nine damage values; zero resisted/blocked/absorbed and
absent crit/glance/crush are nil). 12340 victim states come from AzerothCore `Unit.h`.

## Named tests

- `attacker_state_reads_the_12340_overkill_and_conditional_blocks` now asserts the parsed
  overkill word (5).
- `swing_damage_event_has_the_seventeen_wrath_combat_log_values`
- `swing_damage_carries_nonzero_resist_block_and_absorb`
- `feed_delivers_a_wounded_swing_and_silently_retires_the_rest`
- `swing_queue_clears_at_scope_end_and_reports_capacity_loss`

## Overseer review

The 12340 `ShouldShowFeedback` gate was added after review: a blocked melee-spell round
(`melee_spell_id != 0` and victim state not WOUND) is dropped before the SWING_DAMAGE block.
Test: `a_blocked_melee_spell_round_is_not_a_swing_damage_event`.

## CI

CI 44486035669 on cca4ede1: Windows check green, changed-crate tests green; the gates job's
only red step was the pre-existing `player-clippy` debt in untouched lines. Landed as 5fb029bd.
No Cargo command, client or server was run in this task.

## What is not proven in game

- No live session: nothing proves a real 12340 swing reaches the stock combat-log frame, that
  timestamps line up with the addon's expectations, or that both names resolve before the retry
  window in ordinary combat.
- Swing rows go through the shared 16-slot `WrathFeedEvent` queue (only the native cast queue is
  separated), so a long render stall during a melee burst could evict a lifecycle edge. Not
  measured.
- Group-affiliation header flags remain absent, as for party kills; only the GUID-family bits are
  set.
- A non-zero `melee_spell_id` row is emitted only when the victim state is WOUND, matching
  `ShouldShowFeedback`; a blocked melee-spell round is dropped. No spell-damage/log-name path is
  claimed here.
- `SWING_MISSED` is out of scope: the next slice should build event index 2 from 12340's
  `hitInfo & 0x10` (miss/parry/dodge/absorb/resist/block) and name the miss kind.

