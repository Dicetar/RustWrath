# OpenCode checkpoint - parked character select on the fresh Wrath session

Date: 2026-10-26
Owner: OpenCode (gameplay/integration)
Checkout: `wotlk`, source slice based on `8ef5833d`; GLM's commits `f0513f54` and `3563bc35`
(and its dirty `benilla-procgen` work) are not part of this slice and were not touched.
Scope: source-only wiring of the existing character-select screen to a fresh Wrath session
after a clean logout, the 3.3.5 `SMSG_CHAR_ENUM` parser, and its translation onto the
established 1.12 roster seam. Build and unit tests were run under the Phase 0 authorization;
no rendered, graphical or live-server acceptance was performed (Phase 2-3 still need their own
grant).

## Owned files

Changed by this slice:

- `crates/benilla-app/src/wrath.rs`
- `crates/benilla-protocol-wrath/src/gameplay.rs`
- this checkpoint

Not changed:

- `crates/benilla-procgen` and GLM's other files (`tools/patch_procgen_dungeon.py` stays
  untracked, GLM-owned)
- the stock glue/session state machines, `net.rs`'s stock IO thread and its `CharPick` contract
- the item/container, event-scope and lifecycle additions from the earlier slices (except the
  `apply_item_world` type fix below, which was the open compile error from the item slice)

## Channel and admission

- The stock select screen (`roster::send_pick`, the screen's re-asks, `reconnect.rs`) sends
  through `CharPick`. In Wrath mode `NetPlugin` runs with `connect: false`, so its IO thread
  never spawns, the stock park receiver is dropped, and every send died silently.
  `WrathNetPlugin::build` now rebinds `CharPick` to a wrath-owned channel and drains it from
  `WrathPickQueue` in `wrath_pick_admission`, chained after `wrath_session_ownership` in
  `PreUpdate` so a finished worker is joined in the same frame before an ask is judged.
  The stock run (no wrath config) keeps the stock `CharPick` path untouched.
- Admission gates are `WrathControl::start`'s existing ones (worker/retiring empty,
  `terminal_ready`, empty tagged queue, restart budget after a clean stop). A pick outside
  `ClientState::CharSelect` is stale and dropped, order intact.
- `WrathSessionStart::{Auto, Parked}` records the boot flow versus the parked re-entry.
  `Parked` gets one fresh ask channel per session; `Auto` clears it. A parked link that dies
  does not arm the admission retry (`SessionEnded` refuses it): that is connection loss, the
  app shows the disconnect, and it does not redial under the screen. Retries re-dial `Auto`.

## Parked behaviour

- The boot flow is unchanged: wait for the server's enumeration, enter the first record, and
  publish no roster (there is no screen).
- The parked flow parses the enumeration with `gameplay::parse_char_enum`, publishes
  `SessionEvent::CharacterList` through the existing scope-tagged seam (`roster_from`), then
  serves the screen on the parked link: `GetCharacterListUpdate` re-enumerates and republishes
  (`RosterCause::Asked`), the screen's Enter World ask sends `CMSG_PLAYER_LOGIN` for the picked
  guid. The parked loop has bounded socket reads and the cancel edge only; the screen may sit
  indefinitely, as the stock park does.
- A pick for a guid the session's roster does not hold is refused with a warning and the park
  keeps serving; nothing is manufactured and the session is not torn down.
- Create, delete and rename asks are refused with a warning (the stock run's park loop is not
  ported); `Abandon` is ignored because the parked stop is app-owned.

## Parser and translation (`benilla-protocol-wrath`)

- `parse_char_enum` walks a count byte then `Player::BuildEnumData`'s exact order
  (`Player.cpp:1106`-1299): raw `u64` guid, cstr name, appearance bytes, level, zone/map,
  position, guild, character flags, customize flags, first-login byte, the pet triple, then 23
  visual slots of `u32 display_id, u8 inventory_type, u32 enchant_id`. Trailing bytes are not
  the records' business.
- `roster_from` maps onto the stock `benilla_protocol::Character`: the 19 equipment slots carry
  over (bag slots 19-22 drop, as do customize flags and the first-login byte), and a wire level
  of 0 is the fresh character's mark (`new_character`, level pinned to 1, matching the 1.12
  parse). The stock `Character::read` cannot read a 3.3.5 payload (19 slots without enchant
  ids plus one bag slot), so the Wrath path never calls it.
- The roster publish carries `realm: None` and `generation: 0`: the parked session dialed the
  realm list's first realm and no abandon generation exists on this path.

## Why the parser exists next to the stock one

`Character::read` parses the vendored 1.12 wire shape. A 3.3.5 server writes 23 slots with an
enchant id each; parsing it with the 1.12 reader would desynchronise at the first slot and
poison every later record. Keeping the two parsers separate keeps the stock seam byte-exact
while the Wrath path stays faithful to `BuildEnumData`.

## Verification

Runtime-verified in this environment (cargo 1.99.1, Windows):

- `cargo check --workspace --exclude benilla-procgen`: clean. The exclude is required because
  GLM's `3563bc35` left `crates/benilla-procgen/src/lib.rs` with an unclosed `Ok(ProcgenOutput {`
  call (reported to GLM; not touched here).
- `cargo test -p benilla-protocol-wrath`: 25 lib + 2 replay + 14 session tests green, including
  the new char-enum tests (two-record fixture, 23 visual slots, truncated record refused).
- `cargo test -p benilla-app --lib`: 6582 passed, 2 failed. The two failures are the pre-existing
  `screenshot::tests::names_follow_the_reference_and_never_collide` and
  `names_stamp_the_local_time`, timezone-dependent and unrelated to this slice (they fail at the
  prior commit too). `cargo test -p benilla-app --lib wrath::`: 5 green (2 existing + 3 new:
  roster translation, ask forwarding, no-config refusal).
- `cargo clippy` and `cargo fmt` are not installed in this environment (no rustup); the lint and
  format gates remain UNKNOWN.

Not run (source-observed only): the graphical launch and the live character-select re-entry.
The parked loop's wire behaviour against a real 3.3.5 server, the rendered roster
(`realm: None` display), and the complete logout -> select -> pick -> world cycle are
UNVERIFIED until the Phase 2-3 acceptance in
`2026-10-25-oc-graphical-acceptance.md`.

## Remaining gaps

- Create/delete/rename on the select screen are not wired; the stock park loop stays the only
  owner of those flows.
- The roster publish leaves the realm field empty, so the screen's realm label has no fresh
  value after a parked re-entry.
- A pick that lands in the same frame a worker's terminal feed is still draining can be dropped
  by the admission gates; the screen re-asks when it shows again, which is what recovers it.
- The parked session fetches its first roster with an explicit `CMSG_CHAR_ENUM` even when the
  ask that started it was a pick, not an enumeration (the pick-start has no roster of its own
  yet and the entry needs bounded state to validate against).
- The `WrathControl::running` dead-code warning predates this slice (no caller at `8ef5833d`
  either) and was left alone.
