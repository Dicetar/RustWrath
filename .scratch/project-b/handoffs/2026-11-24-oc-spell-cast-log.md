# SPELL_CAST_START / SUCCESS / FAILED combat-log feed

Status: ready to merge
Owner: OpenCode / LongCat 2.5 Free
Branch: oc/spell-cast-log
Issue: none
Next: integrator review and merge; the owner builds and launches by hand

Finished by OpenCode / Hy4 flash Free after the previous worker hit a provider free limit. The
previous worker's uncommitted `wrath.rs` row structs, feed events, queues and apply methods were
kept intact and are part of this branch.

## Scope

Add `SPELL_CAST_START`, `SPELL_CAST_SUCCESS` and `SPELL_CAST_FAILED` to the Wrath
`COMBAT_LOG_EVENT_UNFILTERED` feed. Reuse the already-decoded `WrathGameplayEvent::SpellStart`,
`SpellGo` and `CastFailed` rows; do not decode `SMSG_SPELL_START`/`GO`/`FAILURE` again and do
not edit `wrath_native.rs`, `ui_script/wrath_cast.rs` or `refresh_cast_interruptibility`.
Do not edit `crates/benilla-ui/src/script/combat_log.rs`.

## Reference

- `SMSG_SPELL_START = 0x131`, `SMSG_SPELL_GO = 0x132`, `SMSG_SPELL_FAILURE = 0x133`
  (`Net/Types.h:313-315`).
- `UnitCombatLogCastStart` OFFSET `0x00751920` (`UnitCombatLog_C.cpp:2254`), called at
  `Spell_C.cpp:1094` after a non-hidden START: event 5, destination nil, 11 values.
- `UnitCombatLogCastGo` OFFSET `0x007519E0` (`UnitCombatLog_C.cpp:2272`), called at
  `Spell_C.cpp:2108` after a non-hidden GO: event 6, the packet's object target, 11 values.
- `UnitCombatLogSpellFail` OFFSET `0x00751AD0` (`UnitCombatLog_C.cpp:1556`), called from
  `Spell_C.cpp:6793/6856/6858` when the active player's cast error is displayed: event 7,
  destination nil, 12 values (the 0x2 reason text). `SMSG_SPELL_FAILURE` does not create a
  combat-log entry; `SPELL_FAILED_DONT_REPORT` (0x17) and an empty text are dropped.
  `CombatLogEntry::PushEvent` OFFSET `0x0074E290`.
- All three drop a spell whose `Attributes & 0x180` is set, whose name is empty, or which is
  hidden in the combat log (`SpellIsHiddenInCombatLog`: learn-spell effect 36,
  `AttributesExD & 1`). GO also drops an open-lock spell. START/GO drop a packet with
  `castFlags & 0x1` (hidden).

## What is on the branch

- `crates/benilla-app/src/wrath.rs`: the three row structs, feed events, bounded queues with
  monotonic revisions and drop/unresolved counters, and the three `apply_*` methods from the
  previous worker are unchanged. Added the three queue clears to `clear_rune_calendar`, the three
  `WrathFeedEvent` apply arms in `wrath_net_drain`, and three borrowed `if let` pushes in the
  session-loop catch-all — `SpellStart` → `SpellCastStart`, `SpellGo` → `SpellCastSuccess`,
  `CastFailed` → `SpellCastFailed`. The event is never moved or consumed there, so
  `update_from_gameplay` and `translate_live` still run on the same packet.
- `crates/benilla-app/src/ui_script/wrath_combat_log_feed.rs`: six memo counters and their
  drop/unresolved diagnostics, three drains with the existing scope/name-retry semantics,
  `cast_log_header` (nil destination = guid 0, Lua nil name, flags 0), a shared spell trio,
  the `SpellIsHiddenInCombatLog` gate (plus `open_lock` for GO only), and the three arg builders.
  START/SUCCESS post 11 values, FAILED posts 12 (trio + reason text). The failure source is the
  active player (`world.player_guid`), the `SMSG_CAST_FAILED` recipient; its text comes from
  `crate::ui_action::CAST_FAIL_KEYS[raw]` via `script.lua().globals().get::<String>(key).ok()`,
  falls back to the key, and an empty text drops the row.

## Deviations

- `%s` argument formatting for the failure text (`FailArgs::fill`) is out of scope: the raw
  GlobalString is posted unchanged, as the note's earlier scope allowed.
- A spell with no `Spell.dbc` row is dropped, matching the reference's `GetRecord` gate, rather
  than posted with a nil name as the damage/heal builders do.

## Checks

CI run 44948973389 failed to compile: `CAST_FAIL_KEYS` was re-exported only under `#[cfg(test)]`, `targets.unit` is an `Option<u64>`, and one test binding was unnecessarily mutable. The overseer exposed the constant unconditionally, stored `unwrap_or(0)` for a nil target, and removed the `mut`. Re-run pending. No Cargo command, no client or server launch, no `target` directory; `rustfmt`
was used only to format the two edited Rust files. CI compiles and tests every pushed branch and
is the authority.

Named tests (added, no existing test deleted or truncated):
- `spell_cast_start_has_the_eleven_wrath_values`
- `spell_cast_success_names_the_packet_target`
- `spell_cast_failed_posts_the_error_text`
- `hidden_or_open_lock_casts_are_dropped`
- `cast_start_without_cast_time_is_dropped`

Not proven: no local compile, no CI run, and no in-client gameplay observation. The owner builds
and launches by hand after the batch lands.
