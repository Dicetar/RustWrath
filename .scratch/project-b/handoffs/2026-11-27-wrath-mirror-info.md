# Match Wrath GetMirrorTimerInfo server-value semantics

Status: ready to merge; inherited WIP base repaired
Owner: Claude Opus 5.5
Branch: fix/wrath-mirror-info
Issue: none
Next: review/merge; the repaired branch compiles and its focused tests pass

## Change

`GetMirrorTimerInfo(index)` now returns the timer's stored server value (`value_ms`). `GetMirrorTimerProgress(timer)` continues to interpolate from the packet timestamp, matching the 3.3.5a client split between the info and progress getters.

## Reproduction

The existing focus fixture starts `BREATH` at 60,000 ms, advances the clock by five seconds without another packet, and calls both getters. Before this change, both returned 55,000. Wrath returns 60,000 from `GetMirrorTimerInfo(2)` and 55,000 from `GetMirrorTimerProgress("BREATH")`.

## Inherited base repair

`18b106d3e` did not compile on its own. The mirror fix is unchanged; this branch adds the
repairs the WIP needed so the branch can be green:

- `wrath_security.rs`: `FRAME_HANDLE_LUA` used `r#"` while its Lua contains `select("#", ...)`;
  the raw string now uses `r##"..."##`. The secure-handle test tolerates the Wrath layout
  round-trip (120 script units reads back 119, a `-6` anchor offset reads back -7).
- duplicate `trade_skill_link_source` removed from `script::host::CallHost` and from
  `ScriptCallHost` in `script_calls.rs`.
- the WIP tests now use the APIs that exist: `set_now` (not `set_clock`), `seed_quest_record` (not
  `set_quest_record`), `autocast_*` (not `auto_cast_*`), and `..Default::default()` for the newer
  `PetActionView` fields.
- `IsPossessBarVisible` answers 1/nil, matching its own test and the rest of the unit getters.
- the rune/totem feed reads `Spells.catalog.get` and `SpellDisplay.icon`, maps a totem to the
  `WrathTotemSlot` fields the UI reads (`have_totem`, `name`, `start_time`, `duration`, `icon`),
  and compares `MAX_TOTEM_SLOTS` as `usize`.
- the honor test builds `WrathPlayerRaw` from `WrathFieldWindow::from_dense`; the rune test drops
  the nonexistent `base_type` field; clippy's `args.front()` replaces `args.get(0)`.

## Verification

- `cargo test -p benilla-ui --lib`: 3,564 passed, 6 ignored.
- `cargo test -p benilla-ui --lib script::wrath_natives::focus_tests::wrath_multicast_totem_flyout_and_possess_and_mirror_and_quest_link`: passed.
- `cargo check -p benilla-client --all-targets`: passes.
- `cargo test -p benilla-app --lib addons`: 76 passed.
- `cargo test -p benilla-protocol-wrath --lib`: 154 passed, 1 ignored.
- `cargo clippy -p benilla-ui -p benilla-app --lib -- -D warnings`: clean.
- `git diff --check`: passed.
- The local `benilla-formats` data tests still fail on this machine's 3.3.5a install (pre-existing
  donor-data gating); CI has no install and is the authority for those.
- Client/server were not launched; no game data, credentials, or runtime state were accessed.
