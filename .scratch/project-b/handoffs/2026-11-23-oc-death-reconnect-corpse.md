# Re-query the corpse on a reconnect while dead (issue 27 small ticket)

Status: ready to merge
Owner: LongCat 2.5 Free (opencode/longcat-2.5-free)
Branch: oc/death-reconnect-corpse
Issue: 27
Next: owner live run of a dead reconnect against AzerothCore, confirming the answer restores
corpse position/range; then the issue's other blocked rows.

## What changed

- `crates/benilla-app/src/death/mod.rs`, `feed_death`: the no-previous-state dead arm
  (`prev == None`, dead, not a ghost) now sends exactly one `ClientCommand::CorpseQuery`.
  `feed.mirror` is still written before the match, so the ask happens once per session and no
  second ask is added while an answer is pending. The alive-to-dead entry
  (`prev == Some((_, false, false))`) still arms the release window and asks nothing; the
  dead-to-ghost, ghost-to-alive and login-ghost asks are untouched, as is `died_at` arming for
  both dead-entry cases.
- `crates/benilla-app/src/net/session.rs`: read only. Its teardown already writes
  `DisconnectedMessage`, which `end_session_death_feed` turns into a `DeathFeedState` clear;
  world entry marks the session connected again. No edit was needed there.

## Tests

Added in the `death/mod.rs` tests with a captured `NetCommands` channel and the real session-end
edge (`DisconnectedMessage`, `SessionEnd::Lost` through `end_session_death_feed`):

- `reconnect_while_dead_resends_corpse_query` — alive, then alive-to-dead (no ask), then session
  end plus a returning dead body: exactly one `CorpseQuery`, and `died_at` re-armed.
- `no_duplicate_corpse_query_after_reconnect` — after the reconnect's one ask, 20 further frames
  with no answer send none; the mirror latched `(guid, dead, not ghost)`.

No Cargo, build or test run: this session had no Cargo authorization. Existing death tests were
left unchanged.

## Not proven

The live behavior. A reconnect while dead against the server — that AzerothCore re-pushes the
corpse state and that the answer's display map/position restore the range markers — is untested
without a client+server run. Only the source path and the once-per-session latch are unit-covered.
