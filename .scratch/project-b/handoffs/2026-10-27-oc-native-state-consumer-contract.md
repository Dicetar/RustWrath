# Read-only contract — native aura receipts, UI expiration and initial cooldowns

From: OpenCode (native protocol/session/app producer owner). To: Astra (UI/consumer owner).
Delivery: `6feb5d4d` plus the doc commit that adds this file. Nothing here changes a UI file;
the consumer edits below are yours to make. Source references are symbols; line numbers move.

## Aura authority

- `benilla_protocol_wrath::reducer::AuraSlot` (`reducer.rs`): wire fields, unchanged: `slot`,
  `spell_id`, `flags`, `level`, `charges`, `caster` (0 = `AFLAG_CASTER` own-aura marker),
  `max_duration_ms`, `duration_ms` (remaining at the packet). `AFLAG_DURATION` = 0x20.
- `AuraReceipt { revision: u64, received_at_unix_ms: i64 }` (`reducer.rs`): one per held slot in
  `ObjectEntry::aura_receipts`. `revision` is a world-monotonic receipt sequence (first receipt
  is 1; never 0). `received_at_unix_ms` is the packet-arrival wall clock supplied by the session
  owner; 0 means a clock-less reducer caller (tests), not a real receipt time.
- Every add/update block gets a fresh receipt even when every wire field is identical (a valid
  identical delta is still a refresh); removal drops the receipt; `auras_observed` separately
  marks the empty-set boundary, so unknown ≠ observed empty. An `SMSG_AURA_UPDATE_ALL` is
  always an observation edge; a GUID-only full snapshot is observed-empty.
- `WorldState` derives `PartialEq`; receipts participate. Object create clears slots, coverage
  and receipts. World transfer replaces the world; session end and reconnect create a new one.
- `WorldState::reduce_changed_at(op, payload, received_at_unix_ms)` is the clock-carrying form
  the session uses; `reduce_changed` remains the clock-less form (receipt clock 0). Reduction
  stays replayable; no parser or reducer reads a system clock.

## Presentation anchors (the read-only contract)

- `crate::wrath::WrathAuraAnchors` (`wrath.rs`): Resource, initialized by `WrathNetPlugin`.
  - `ui_received_at(guid, slot) -> Option<f64>`: `GetTime()` **seconds** at which the current
    receipt revision was first observed by the presentation owner. `None` when the object/slot
    is not present in the adopted world generation.
  - `ui_receipt(guid, slot) -> Option<(u64 /* revision */, f64 /* GetTime seconds */)>`.
- Stamping: `stamp_aura_anchors` runs in the exclusive world-apply owner after digest entities
  are adopted. One anchor per `(guid, object generation, slot, receipt revision)`; a newer
  revision re-anchors. Anchors are pruned when a slot/object disappears and cleared on transfer,
  session end and the early no-player path. The UI clock domain is the same `GetTime()` domain
  `UnitAura` callers use; the wall/server clocks never reach the UI.
- Digest: `WrathEntity.aura_receipts: BTreeMap<u8, AuraReceipt>` (`wrath.rs:390`) carries the
  authority's receipts beside `auras` and `auras_observed`. `WrathWorldView::auras(guid)` still
  returns the slot list; it does not expose receipts itself.

### Expiration conversion (your `UnitAura` query)

For a slot with `AFLAG_DURATION` set:

- `duration = max_duration_ms as f64 / 1000.0` (the stock tuple's duration field).
- `expiration = ui_received_at(guid, slot)? + duration_ms as f64 / 1000.0`.
- Read the anchor and the slot from the same adopted digest/receipt revision; `ui_receipt`
  gives the revision when you need to detect a refresh between two reads.
- A slot without `AFLAG_DURATION` has no defensible expiry: answer `nil` expiration, do not
  answer 0 and do not describe it as permanent. The wire not carrying a duration is not
  evidence that the aura has none.
- Never compute expiration from query time, a movement/revision timestamp or the wall clock; a
  fixed anchor is what makes the timer stable across `OnUpdate` calls and a UI reload.
- Reapplication/refresh replaces the receipt revision, so the next read re-anchors to the new
  receipt; removal drops the anchor. Your `unit_auras` (`ui_script/wrath_queries.rs`) currently
  answers `expiration = None` for timed auras and `Some(0.0)` for untimed ones; those are
  placeholder values and should be replaced by the rule above.
- `wrath_aura_feed` (`ui_script/wrath_aura_feed.rs`) currently diffs slot lists only. An
  identical-delta refresh changes `aura_receipts` but not the slot values, so compare receipt
  revisions (per slot) as part of the change key if you want `UNIT_AURA` on a refresh.

## Initial cooldowns

- `gameplay::InitialSpellCooldown` (`gameplay.rs:906`): `spell_id u32`, `item_id u16`,
  `category u16`, `spell_cd_ms u32`, `category_cd_ms u32`; exactly the pinned
  `Player::SendInitialSpells` tail. `category_cd_ms` bit 31 is the held/infinity mark and the low
  bits are the remaining time; a category spell carries `0` in `spell_cd_ms`.
- `ServerEvent::InitialSpells { spells, cooldowns, declared_cooldowns }`. The writer declares
  `m_spellCooldowns.size()` but skips entries it marks not client-visible; the flag is persisted
  per row (`character_spell_cooldown.needSend`), so a returning character can load false entries
  and hit the over-declaration. A larger declaration than entries is logged and the complete
  entries present are authoritative; a partial tail is a protocol error and the packet is
  refused. `wrath.rs` translates the entries into the existing `SessionEvent::SpellBook`, so the
  stock `Cooldowns::seed_initial` owner seeds spell and category legs with receipt-time
  anchoring; no second timer authority exists.
- `messages::SpellCooldown.spell_id` widened from `u16` to `u32` because 3.3.5 ids exceed 65535;
  the 1.12 reader widens its wire id. The two test-only id casts that followed (`spell/
  cooldowns_tests.rs`, your `ui_spellbook` initial-cooldown case) are mechanical.
- Live updates keep the existing path (`SMSG_SPELL_COOLDOWN`, `SMSG_COOLDOWN_EVENT`,
  `SMSG_CLEAR_COOLDOWN`, `SMSG_ITEM_COOLDOWN`). Session end clears the list; a reconnect's
  `SMSG_INITIAL_SPELLS` reseeds it. `GetSpellCooldown` behavior is unchanged; initial values now
  come from the wire instead of an empty list.

## WM (v2.3) decision

The observation schema is unchanged. `AurasObservation` carries `state` (unknown/known),
`complete` (export bound) and the slot durations; per-slot receipt revisions/clocks remain
client-internal because no receiver consumer needs them today. A coordinated v2.4 would be
required if WM must consume receipt timing. Both producer adapters read the same
`ObjectEntry` availability, so `unknown` vs observed-empty is consistent across app and driver.

## Open reference question (unchanged)

`SMSG_AURA_UPDATE_ALL` replacement vs merge: the pinned server writer
(`Player::GetAurasForTarget`, `Player.cpp:12048`) enumerates the current visible set and sends a
GUID-only packet only in the forced self case; it does not establish how the closed client treats
slots omitted from an `_ALL` packet. benilla keeps merge-with-coverage semantics and does not
drop omitted slots. If replacement is approved later, the full set must be validated before
commit. Do not encode a replacement policy in the UI.

## Wrath-host fixture correction (yours; not executed here)

`ui_script/wrath_ui_slice_tests.rs::player_frame_script` constructs `UiScript::new()`, which is
explicitly `UiTarget::Vanilla112`. Exact correction:

```rust
use benilla_ui::script::{ScriptValue, UiScript, UiTarget};

fn player_frame_script() -> UiScript {
    let mut s = UiScript::with_target(UiTarget::Wrath335).unwrap();
    assert_eq!(s.target(), UiTarget::Wrath335, "explicit Wrath host");
    // ... unchanged file list ...
}
```

and make data admission explicit instead of relying on the skip: run with
`BENILLA_REQUIRE_DATA=1` (the `wow_data_or_skip!` macro's requirement switch) or replace it
with an explicit profile/chain open assertion that reports which profile served the FrameXML.
Update the file header: it currently claims a 3.3.5 chain while the host is vanilla. A pass is
not Wrath-host evidence unless the target assertion and data admission both hold.
