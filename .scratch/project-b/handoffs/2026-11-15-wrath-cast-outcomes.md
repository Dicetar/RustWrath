# Cast outcomes: the lost cast correlation and the double terminal

Status: ready to merge
Owner: OpenCode / DeepSeek V4.1 Flash (overseer) with one AutoSwarm `opencode` child
Branch: fix/wrath-cast-outcomes
Issue: none
Next: owner launch checks for Auto Shot and an interrupted cast; one real logout export should show one exact chain per cast
Based on: origin/main `ad0a6a15`. Brief B4 in `rustwrath-agent-briefs-2026-11-15.md`.

## AUTO-05 — the correlation gap (fixed)

Evidence: `benilla-config/Logs/Launcher/latest-tickets.md` AUTO-05 — one cast appears as a local
chain under its action id and a separate native chain with "action unknown (local action not
retained)", with `wire=8 attempt=9` against `wire=8 attempt=unknown`.

Cause: attribution was re-derived at snapshot time from the bounded `cast_candidates` ring. A
session end retires that ring (`SessionEnded`), and long sessions evict it at
`MAX_OPEN_INTERACTIONS`, while the report is built from the retained event rows. The rows still
carry the join — the local interaction, the spell, the scope and the admitted wire count — so a
join that was exact when the edge arrived degraded to "not retained" in the export.

Fix (`crates/benilla-app/src/quest_trace.rs`): `cast_attribution` keeps the ring path and adds a
retained-row path (`cast_attribution_from_evidence`) that applies the same exact/strong/heuristic
rules to the rows, joining by `(scope, spell, admitted wire)` regardless of the native attempt
(local rows have no attempt: 12340's attempt serial is native-only). `CastAttribution` carries
`evidence_interaction` so the inspector names the recovered action; a per-write candidate refresh
covers a transport write that outruns the tap's admitted row.

Tests: `a_native_cast_edge_keeps_its_local_chain_after_the_candidate_ring_evicts_it` (70 newer
writes evict the ring; the retained edge still attributes exactly to its action) and
`a_write_that_arrives_before_its_admission_still_joins_exactly`.

## AUTO-03 — spell 75, Auto Shot "Interrupted reason=40" (fixed)

Reason 40 is genuinely `SPELL_FAILED_INTERRUPTED` (AzerothCore `SharedDefines.h`; 1.12 key
`0x23`), so the mapping was right. Two real defects sat behind the symptom:

- **One cancel produced two terminals.** 12340 sends `SendCastResult` + `SendInterrupted`, and
  `SendInterrupted(0)` can follow a real failure. The adapter now synthesizes a 1.12 result only
  when the native authority resolved a new terminal, and the native model drops a same-attempt
  broadcast duplicate (identity is scope + caster + spell + monotonic count).
- **A running auto-repeat's interrupted terminal fired a false "Interrupted".** 12340 ends a
  repeat with `SendInterrupted(SPELL_FAILED_INTERRUPTED)` (`Spell::cancel`); 1.12's
  `StopAutoRepeatSpell` has no failed or interrupted edge. The terminal now stops the repeat and
  reports nothing (`spell/net.rs`, `spell/report.rs::is_interrupt_reason`).

Tests: `an_interrupted_auto_repeat_stops_without_a_failure_edge`,
`an_interrupt_with_zero_reason_does_not_fail_the_same_cast_twice`,
`a_cancel_resolves_one_terminal_not_a_failed_and_an_interrupted_pair`,
`a_zero_reason_interrupt_is_not_a_second_native_terminal`.

## F05 walk — no further defect proven

Checked and found correct: the `cast_send` guard ladder; `read_cast_failed`'s single-word arms
against AC `Spell::WriteCastResultInfo` (fishing and totem arms leave bytes unread but the packet
ends there); `read_spell_start`/`read_spell_go` layouts; channel stop as `CHANNEL_UPDATE(0)` →
`SPELLCAST_CHANNEL_STOP`, never a failure; cooldowns/GCD/category pads; power events (runes
deliberately unposted, documented in `wrath_power_feed`); combo points via the field watcher.

## Checks actually run

Pinned toolchain 1.98.1, own target dir, `CARGO_INCREMENTAL=0`.

| Check | Result |
|---|---|
| `cargo check -p benilla --features hotpatch` | Finished, no errors |
| the six new tests | 6 passed |
| `cargo test -p benilla-app --lib quest_trace::` | 15 passed |
| `cargo test -p benilla-app --lib wrath_native::` | 8 passed |
| `cargo test -p benilla-app --lib spell::net::` | 70 passed, 1 failed — **pre-existing**: `a_second_esc_resends_the_strikes_cancel_and_the_echoes_fire_nothing` fails identically on clean `main` (`REFUSED 0x23` + an extra `CHANGED`), so its expectation is stale, not this branch |

Three fixes were needed to make the child's tests compile: the test used
`gameplay::WrathGameplayEvent` (the type is imported unqualified), formatted
`Vec<CastBarEdge>` with `{:?}` (no `Debug`), and its 70-write burst overflowed the 256-event
ingress ring before draining, so the native edge was dropped before the assertion.

## Not done, not proven

- No client run and no live export: the export should now show one `confidence=exact` chain per
  cast with its action id, but that is the owner's next session to confirm.
- The stale Esc test above should be re-pinned to the current stream (it fails on `main`).
- The end-to-end presence of the aura/absorb/proc reasons beyond the single-word arms was not
  re-verified against a live server.

## Owner check list

1. Cast a damage spell on a mob: one cast bar, one result, damage and power shown.
2. Cast out of range and while moving: the failure text, no duplicate line.
3. Get hit during a cast: "Interrupted" once.
4. A channelled spell: start, ticks, stop on cancel and on being interrupted (one terminal).
5. A cooldown spell twice: the cooldown start and the second press's refusal.
6. Auto Shot: start it, deselect/let the target die — no red "Interrupted" line; a genuine interrupt of
   the shot still shows it.
