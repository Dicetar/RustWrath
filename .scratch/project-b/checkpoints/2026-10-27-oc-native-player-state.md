# Native player-state initialization and refresh — claim and working record

Date: 2026-10-27. Owner: OpenCode (integration lead). Checkout `D:/Projects/project-b-client`,
branch `wotlk`. Reviewed start: `e3c83d90`; current tip read at claim time: `51493bd7` (Astra's
authorized verification record). Preserve every later commit; this is not a reset point.

Campaign: finish the native aura receipt/timing authority on `adc8a631` and complete initial
cooldown ingestion (`SMSG_INITIAL_SPELLS` tail) from the pinned AzerothCore writer, with exact
read-only consumer contracts. No WM schema change is planned; see the decision below.

## File claim (exact)

Editing:
- `crates/benilla-protocol-wrath/src/reducer.rs` — per-slot aura receipt identity/revision and
  arrival clock, observation edges, `reduce_changed_at`.
- `crates/benilla-protocol-wrath/src/session.rs` — pass the packet-arrival clock into the
  reducer's aura path.
- `crates/benilla-protocol-wrath/src/gameplay.rs` — decode the `SMSG_INITIAL_SPELLS` cooldown
  tail from the pinned writer; local `InitialSpellCooldown` type.
- `crates/benilla-protocol/src/messages/spellbook.rs` — widen `SpellCooldown.spell_id` to `u32`
  (3.3.5 ids exceed `u16`; the 1.12 parser converts).
- `crates/benilla-app/src/wrath.rs` — digest aura receipts, presentation GetTime anchors and
  pruning, initial-cooldown translation.
- `crates/benilla-app/src/wrath_native.rs`, `crates/benilla-wm-driver/src/main.rs` — only if the
  producer seam needs the new source facts (availability already carried).
- `crates/benilla-app/src/spell/cooldowns_tests.rs`, `crates/benilla-app/src/ui_items/feed_cooldown_tests.rs`
  — mechanical type annotations only if the widened field requires them (no new cases).
- This checkpoint and a consumer handoff section/file.

Not editing: Astra's `crates/benilla-lua/**`, `crates/benilla-mlua/**`, `crates/benilla-ui/**`,
`crates/benilla-app/src/ui_script/**` (including `wrath_ui_slice_tests.rs`); procgen; WM tracked
or untracked owner files; `.scratch/project-b/verification/**`.

## Decisions taken before code

- **Aura clocks**: acquisition stays in the session (packet arrival, wall ms). The reducer
  receives the timestamp as data and stays replayable. Presentation stamps the UI GetTime
  anchor once per receipt revision; UI expiration conversion is Astra's query (handoff below).
- **WM schema**: v2.3 stays unchanged. The wire aura block carries availability and the wire
  durations; per-slot receipt revisions/clocks are client-internal and are not a required
  receiver fact today. A coordinated v2.4 would be needed if WM later wants them.
- **AURA_UPDATE_ALL replacement vs merge**: not guessed. The pinned writer
  (`Player::GetAurasForTarget`, `Player.cpp:12048`) enumerates the current visible set and sends
  a GUID-only packet only in the forced self case; how the closed client treats omitted slots is
  not established by that writer. benilla keeps merge-with-coverage semantics.
- **Initial cooldown count**: the pinned writer (`Player::SendInitialSpells`,
  `Player.cpp:2730`) declares `m_spellCooldowns.size()` but skips entries with
  `needSendToClient == false` or no `SpellInfo`. The flag is persisted per row by
  `_SaveSpellCooldowns` (`Player.cpp:3612`), so a returning character can load a map with false
  entries and hit the over-declaration. Complete 16-byte entries present are the intended
  client-visible set; a larger declaration is logged, never silently equalised, and a partial
  trailing entry is a protocol error. `SMSG_MODIFY_COOLDOWN` has no caller in the pinned server
  tree (`Player::ModifySpellCooldown` is unused), so no client path requires it.

## Delivery (source)

`6feb5d4d` — aura receipt authority and initial cooldown ingestion:
- `AuraReceipt` per slot (`revision` world-monotonic, `received_at_unix_ms` from the session's
  packet arrival); every add/update block is a receipt (identical deltas included), removals
  drop it, full snapshots always mark an edge; `auras_observed` still separates unknown from
  observed-empty. `reduce_changed_at` keeps the reducer clock-free and replayable. Digest carries
  `aura_receipts`; `WrathAuraAnchors` stamps one GetTime anchor per receipt revision and prunes on
  generation/transfer/session end. Position provenance is untouched by aura edges.
- `SMSG_INITIAL_SPELLS` cooldown tail decoded from the pinned writer (u32 spell, u16 item, u16
  category, spell/category remainders; category bit 31 held). Over-declared counts are the
  server's `needSendToClient` filter (logged, entries used as written); partial tails are refused.
  Entries seed the existing `Cooldowns` owner through `SpellBook`; session end clears and the next
  login reseeds. `SpellCooldown.spell_id` widened to u32.
- Consumer contract: `.scratch/project-b/handoffs/2026-10-27-oc-native-state-consumer-contract.md`
  (anchors, expiration formula, cooldown semantics, AURA_UPDATE_ALL decision, Wrath-host fixture
  correction). One test-only id cast in Astra's `ui_spellbook` case followed the widened field;
  no UI behavior was written.

## Evidence actually obtained

- `cargo check` for `benilla-protocol-wrath`, `benilla-protocol`, `benilla-app --lib`,
  `benilla-wm-driver` (combined worktree, including Astra's uncommitted UI WIP; her files were
  not staged).
- `cargo test -p benilla-app --lib cooldown` 71 passed; `wrath` 12 passed; `ui_spellbook`
  6 passed. Existing tests only; no new test file or case.
- The aura receipt parser/edges, the initial-cooldown tail and the anchor stamping have **no
  dedicated test**; they are source+compile evidence. A G/L run and a coordinated test grant
  remain the gates.

## Remaining reference/permission gates

- AURA_UPDATE_ALL replacement vs merge: still open; pinned writer evidence recorded in the
  claim; do not guess.
- Wrath-host fixture: exact correction handed to Astra; not executed (no test-edit grant, her
  file). A Wrath-host pass remains UNKNOWN until it uses `with_target(Wrath335)` and explicit
  data admission.
- Northshire run card: unchanged, ungranted. The lab server was up at claim time; credentials are
  owner-held.
- WM v2.3 wire unchanged; receipt clocks are client-internal by decision.