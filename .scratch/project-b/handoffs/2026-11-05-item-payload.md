# S3 item payload handoff — 2026-11-05

Worker A (`call_9d78b5cfb24346358c89becbce086ae7`), branch `feat/wrath-item-payload`.
Base `39e3c62a43f8967beb74c660fe3a76971d24924a` (integration/release-p0-inventory-session-2026-11-05).
Reference pin `e67b6ffef23655dc7da061cb2d36b7963f6ef71a` (read-only, not modified).
Peer `inventory-readers` (Worker B) boundary agreed by AutoSwarm message
`msg_968b99adcd8346d5befd38bbfc502ab3` / reply `msg_0d5c1f6d65064218bb3a7c1987fff327`.

## Scope

Lossless Wrath item instance payload through tooltip, bind, visual and item-text consumers.
Source-only. No Cargo build/check/test/clippy/fmt, no client, server, DB or live session.

## Exact paths and functions

- `crates/benilla-app/src/wrath.rs`
  - `WrathEntity.item_fields`: `Option<Vec<u32>>` → `Option<WrathFieldWindow>` over
    `ITEM_END`/`CONTAINER_END` (`build_digest`).
  - `push_item_fields`: delivers-only projection; donor 22..42 and 58..61 → 43,44,46,47 unchanged;
    Wrath 43..57 and 62 never written; container 64/66..137 → 48/50.. unchanged.
  - `apply_item_world`: builds/refreshes the `WrathItemRow` on the same entity and returns the old
    and new row; writes only on change so `Changed<WrathItemRow>` is real.
  - `wrath_apply_world`: row diffs feed a synthetic `ItemNotice::Enchantment` for Wrath slots
    8..12 (fields 43..57) only, after the donor 22..42 notices, with ownership checked through
    `ItemRead::owner`.
- `crates/benilla-app/src/wrath_item.rs` (new)
  - `WrathEnchant`, `WrathItemRow`, `ItemRead`, `ITEM_FLAG_HAS_TEXT` (`0x200`).
  - Accessors: 12 enchant slots (id/sign, duration, charges), property seed, random-properties id,
    durability, max durability, create-played-time, stack count, duration, five spell charges,
    flags, owner/contained/creator/gift creator, container num slots + 36 slot pairs.
- `crates/benilla-app/src/items.rs`
  - `already_bound_item`, `live_enchant_item`, `gift_creator_name_item`, `creator_guids_item`.
  - `ItemMoved` now also watches `Changed<WrathItemRow>`.
  - Donor wrappers `already_bound`/`live_enchant`/`gift_creator_name`/`creator_guids` retained.
- `crates/benilla-app/src/net.rs`
  - `Objects::item_row(guid)` and `Objects::item_read(guid)`, additive; no object/store/count/walk
    behaviour changed.
- Tooltip/UI: `ui_items/feed.rs` (`resolve_slot`, `resolve_slot_in_target`, `worn_cooldowns`,
  `ask_item_build_lookups`), `ui_items/mod.rs` (`object_name_item`, `object_link_item`,
  `object_link_numbers_item`, `object_link_in_target_item`; donor wrappers kept),
  `ui_char.rs` (`inventory_slot`, `weapon_enchant`), `ui_bind_confirm.rs` (`use_binds`),
  `spell/targeting/item.rs` (`gate_item`), `ui_merchant/mod.rs` (buyback/repair).
- Visual: `entities/equipment/resolve.rs` (`enchant_fold`, per-slot read/glow), `entities/item_glow.rs`
  unchanged (it already folds an iterator; the caller now supplies 12 ids for our own Wrath item).
- Item text: `ui_item_text.rs` (`WrathItemTexts`, `ItemTextReply`, `apply_wrath_item_text`,
  `retire_wrath_item_texts`, `ReadSource::ItemText`, `read_item_text`, `begin_item_text`,
  `feed_item_text`, plugin registration), `ui_items/drain.rs` (`dispatch_fork` `0x200` bit and
  the Wrath bag-read branch).
- Docs: `docs/architecture/rfc-lossless-wrath-inventory.md` section 7 + acceptance;
  `docs/architecture/architecture-traceability.md` C11 and INV-13..18.

## Typed model and presence

`WrathItemRow` is a Bevy component on the existing item/container entity; it does not reparent or
own contents. Its window keeps the reducer's delivered mask:

- never delivered = `None`;
- delivered zero = `Some(0)` (an observed clear);
- a digest that does not mention a field retains the last delivered value (the reducer mask is
  cumulative for the object generation);
- `ItemRead` prefers a delivered row field, else the donor projection (donor 7 / donor 43..47).
  A delivered row zero wins over the donor value.

Container identity, `WrathGeneration`, `GuidIndex` and `CONTAINER_FIELD_SLOT_*` ownership stay on
the existing entity. A generation change still despawns and respawns that entity, row included.

## Unsupported / intentionally unmigrated

- `item_watch::kin`/`slot_count`, `ui_items::walk_inventory*`, `count_of/sum_of/action_count/
  stack_counts/find_item/collect_inventory`, `ui_tooltip/spell_feed.rs` (Worker B).
- `item_watch` donor 22..42 enchant notice; the row notice covers only 43..57.
- `ui_char::ammo_count`, `ui_trade` counters and other pure stack/count walks stay donor.
- Sockets/gems beyond enchant slots 2..4, `RandomSuffix` factor, scaling, item-limit columns:
  not carried by the 12340 item query response/item descriptor used here; not invented.
- `Countdowns` remains seven cells; row `duration` for slots 8..12 has no countdown lane.
- `ITEM_FIELD_CREATE_PLAYED_TIME` stored, no consumer.
- Raw player window (`WrathPlayerRaw`) untouched.

## Item-text path

Producer (existing, now consumed):
`WrathGameplayEvent::ItemText` → `WrathNativeFeedEvent::ItemText(WrathItemText)` → Bevy message
`WrathItemText` (native queue, not session-reply queue).

Consumer:
`apply_wrath_item_text` → `WrathItemTexts` cache keyed by item guid (`Text`/`NoText`) → `begin_item_text`
and `feed_item_text` for `ReadSource::ItemText`; `ItemTextOpen` drives the stock `ItemTextFrame`.
The read side sends `ClientCommand::ItemTextByGuid`, already encoded as
`CMSG_ITEM_TEXT_QUERY` at `wrath.rs:4198`. Reads are once per guid while in flight; the absent
form (`has_text == false`) retires the oldest in-flight guid as `NoText` and closes the pending
read. The readable bit is `ITEM_FIELD_FLAGS & 0x200` (12340 `CGItemText::SetItem` /
`Item_C.cpp`); 1.12 keeps its `ITEM_FIELD_ITEM_TEXT_ID` letter path.

## Tests added, not run

- `wrath_item::tests`: `absent_and_delivered_zero_are_distinct`,
  `all_twelve_enchant_slots_decode`, `create_played_time_is_stored_but_never_projected`,
  `the_container_slot_array_is_the_container_owners`, `donor_fallback_reads_the_unmigrated_fields`,
  `a_delivered_random_property_row_wins_over_the_donor_projection`,
  `a_delivered_row_zero_wins_over_the_donor_projection`,
  `the_has_text_bit_is_the_wrath_readable_signal`,
  `only_slots_eight_to_twelve_are_the_unprojected_band`.
- `wrath::tests`: `wrath_item_projection_never_folds_enchant_slots_eight_to_twelve_into_donor_43_to_47`,
  `an_absent_item_field_is_not_projected_as_a_delivered_zero`,
  `container_slots_project_only_for_container_objects`,
  `an_item_row_keeps_container_identity_and_generation_ownership`.
- `items::tests`: `a_binding_enchant_in_wrath_slot_eight_binds_only_through_the_typed_row`.
- `ui_items::drain::tests`: `a_335_instance_text_bit_forks_to_the_reader`.
- `ui_item_text::tests`: `wrath_item_texts_retire_present_absent_and_scope`,
  `a_wrath_item_text_reads_by_guid_and_refuses_no_text`.

All are unrun; no compilation is claimed. Existing tests were adjusted only for the new
`WrathItemTexts` resource in the reader harness.

## Unrun verification

No Cargo build/check/test/clippy/fmt, no type check, no client/server/DB/live session. Source
observation only. Runtime acceptance is unchecked.

## Runtime acceptance scenario (owner)

On a Wrath character: hover a bag item with enchants outside slots 1..7 and a random suffix;
compare tooltip enchant lines, suffix name, durability and charges against the link and paper
doll. Right-click an item with `ITEM_FIELD_FLAGS & 0x200`: the stock `ItemTextFrame` shows the
body or closes cleanly; a second read closes it. Reconnect or world-transfer and repeat: the
cache must not serve the old world's body. All acceptance boxes remain unchecked.

## Remaining gaps

- Source-only; no compiler feedback on the new module or the `Objects`/`ItemRead` changes.
- countdown cells for Wrath enchant slots 8..12.
- A lost item-text reply leaves that guid in flight until the scope changes; there is no timeout
  or re-ask by design in this pass.
- sockets/gems beyond slots 2..4, suffix factor, scaling, item-limit columns.
- A migrated `item_watch` enchant placement order for the synthetic 43..57 notice (appended after
  donor-order runs, not interleaved by `Place`).
- Raw player window length, separate INV follow-up.
