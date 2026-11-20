# Swarm result 2026-11-22

Owner: the OpenCode DeepSeek overseer. This is the only report for the session; the task notes on
the landed branches carry the per-ticket detail. No local Cargo, client or server was run and no
`target` directory was created.

## Tasks

1. **`SPELL_*` combat-log events: partly landed.** `SPELL_DAMAGE` from
   `SMSG_SPELLNONMELEEDAMAGELOG` landed as dbf8ab97: the decode keeps 12340's overkill word and
   the feed posts the reference's twenty values (8 header, spell id/name/school mask, then damage,
   overkill, school, resisted, blocked, absorbed, critical, glancing, crushing), with nil for
   absent amounts. The other families (miss, heal, energize, periodic, aura applied/removed, cast
   start/success/failed) were not started; they share the same three files and had to follow this
   slice. Not proven: a live 12340 row in the stock combat-log frame.
2. **`player-clippy`: not green; general fmt and clippy are green.** Landing seven one-file
   clippy tickets and the rustfmt repair 607136b6 made the main run 44670987041 pass `fmt` and the
   `clippy` step for the first time. Its `test` step caught this swarm's zero-based
   argument-index test, fixed in bb1d3474. The final main run 44673299445 then passed `fmt` and
   `clippy` and ran 6860 tests; the only three failures are outside the swarm's files and came
   from the merged agent work `cf992aa0`: `GetAuctionSellItemInfo()` answers 9 values where the
   gate wants 6, and two `ui_quest_log` world-entry tests assert an empty send queue. Because
   `test` failed first, the dedicated `player-clippy` step was skipped, so its remaining count is
   unmeasured; the previous ~55-finding debt is reduced by the seven files below. No run has gone
   green.
3. **`UNIT_*`/`PLAYER_*` coverage audit: landed in this note.** Of the admitted Wrath
   `UNIT_*`/`PLAYER_*` names, all are raised except four:

   | Event | Raise site / producer | Status |
   |---|---|---|
   | `UNIT_RUNIC_POWER`, `UNIT_MAXRUNIC_POWER` | `ui_script/wrath_power_feed.rs:EVENTS` posts both from the resource owner | raised |
   | `UNIT_SPELLCAST_START/STOP/SUCCEEDED/FAILED/INTERRUPTED/DELAYED` | `ui_script/wrath_cast.rs:feed` posts them from the cast edges | raised |
   | `UNIT_SPELLCAST_CHANNEL_START/UPDATE/STOP` | `ui_script/wrath_cast.rs:feed`, channel edges | raised |
   | `UNIT_SPELLCAST_INTERRUPTIBLE`, `UNIT_SPELLCAST_NOT_INTERRUPTIBLE` | none: `WrathActiveCast.interruptible` is never set, and no verified 12340 attribute bit is mapped | Blocked: no producer |
   | `PLAYER_FOCUS_CHANGED` | none: the `focus` unit token exists but no `SetFocus`/focus-set producer raises the event | Blocked: no producer |
   | `PLAYER_ROLES_ASSIGNED` | none: decoded LFG updates carry no roles and `SMSG_GROUP_LIST` roles are roster-only | Blocked: no producer |

   No code ticket was written for the four because the current protocol code has no producer to
   raise them from; inventing one would violate the reference workflow.
4. **Gap lists for issues 25-27: landed.** Issue 25 (combat) 4bfafddf, issue 26 (quest/loot/reward)
   956d4d3e, issue 27 (death/recovery) 611c2403. Each has a source gap table, small tickets and
   `Blocked: needs owner run` rows for live-only proof.

## Tickets and models

- `oc/spell-damage-feed`: Iceman Free wrote the slice; the overseer removed an invalid `Eq`
  derive and fixed the zero-based test. (A first Flint free attempt produced no edit and was
  stopped; no commit.)
- `oc/clippy-quest-trace`: Iceman Free.
- `oc/clippy-ui-items`: Beam Free; the overseer gated four test-only readers and removed a stale
  import.
- `oc/clippy-combat-profiles`: LongCat 2.5 Free wrote the fix; Hy4 flash Free finished the note.
- `oc/clippy-ui-talent`: Beam Free.
- `oc/clippy-ui-party-pets`: DeepSeek V4.1 Flash Free.
- `oc/clippy-ui-chat-feed`: Beam Free.
- `oc/clippy-ui-macro-store`: Hy4 flash Free investigated; the overseer applied the one-line
  `#[cfg(test)]` gate.
- `oc/gap-25-combat`: Mimo free (Ling free hit the daily free-model limit before editing).
- `oc/gap-26-quest-loot`: ERNIE 5.5 Flash Preview free.
- `oc/gap-27-death`: Nemotron 3.5 Super VL free (Particle-1 free hit the daily free-model limit
  before editing).
- The main-compile repair, the Task 3 audit and this note were the overseer's DeepSeek work.

Free-model tickets: 11. Paid DeepSeek fallbacks for a worker ticket: none. Two free-model runs were
reassigned after `free-models-per-day-high-balance` (Ling, Particle); one free-model run (Flint)
was stopped after producing no edit and the ticket completed on Iceman Free.

Landed commits: dbf8ab97, 6153d36f, 3dbfa556, 6ea089ae, 8077db3d, 94fc4a79, c9d1402b, 963f42e3,
4bfafddf, 956d4d3e, 611c2403, 607136b6, bb1d3474. Not proven: any live combat-log, clippy-green
gates job, or the three non-swarm test failures above.
