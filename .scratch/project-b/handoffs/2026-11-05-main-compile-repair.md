# 2026-11-05 — main compile repair

Follow-up to the item-payload/readers integration. Three read-only Beam Free audits
(`opencode-go/beam-free`, after the free-provider quota block) found source-level defects on
`main`; two DeepSeek Go workers (`opencode-go/deepseek-v4.1-flash`) and the coordinator repaired
them. Source-only except for the explicitly authorized `cargo check`/focused tests below.

## Branch

| Ref | SHA |
|---|---|
| base `origin/main` | `46430054d72d659ce816cc072d7c0835f0bd9278` |
| `fix/release-main-compile-2026-11-05` | see remote HEAD |

Commits: `4dc3de48` hotpatch, `6a5613ce` wrath, `7a523692` inventory, `bff4d0b1` docs.

## Verified findings and repairs

### Hotpatch (from `e01226de`, blocked compilation)

- `crates/benilla-hotpatch/src/lib.rs` called the nonexistent `RigSpan::valid`; the ABI `RigSpan`
  is `{ entity, first, count }`. Replaced with a checked `first + count <= bones.len()` before
  `pending.take()`; the error text is unchanged.
- Test fixtures used `first_bone`/`bone_count` and omitted `BoneLocal::parent`; corrected to the
  ABI fields and `parent: -1`.
- Rollback now skips a retained predecessor whose digest equals the failed active digest, when
  both are encodable. Retention/consumption policy is otherwise unchanged and remains a design
  question.

### Wrath commands and WMC1 (from `e01226de`, blocked compilation)

- Both `SMSG_MULTICLASS_REPLY` handlers treated the decoded `MulticlassReply` as raw bytes.
  They now use `reply.op/status/serial/body`; the 14-byte reparse is gone.
- `wrath_tap_commands` aligned with the real `ClientCommand` enum: `PetCastSpell` `caster_guid`,
  `PetNameQuery` `guid`, `ItemNameQuery` `entry`, unit `TogglePvp`, `Emote` `emote`,
  `TextEmote` `text_id/target` (wire `emote_num = 0`), `SetFactionAtWar` `at_war`,
  `SetWatchedFaction` `rep_list_id`, `TutorialFlag` `id`, `SetRaidTarget` `guid`, plus `Whois`,
  `GroupSwapSubGroup`, `ReadyCheckAnswer`, `LootMasterGive`, the taxi variants, `AcceptTrade`,
  `SetTradeGold`, the channel commands and `GuildInfoRequest`/`GuildRosterRequest`.
- `MulticlassRequest` serial now starts at `1` (was `2`); the stored next value is `2`.
- WMC1 replies are no longer drained from `WrathWorldView` when no `UiScript` exists; the view
  scope clears still retire them on transition.

### Additional compile paths (found by the first checks)

These were already broken on `main`:

- `cursor/bag_verbs.rs`: `pickup_bag_from_slot` referenced an undefined `model` before the
  `accepts` gate and passed an `AppDataRef` where `&Model` was required; the gate now runs inside
  the existing model block.
- `channel.rs`: `GetNumDisplayChannels` added an integer literal to `f64`; now `3.0`.
- `cursor/swap.rs`: `bag_object` used the runtime local `last` in a range pattern; now matches
  `1..=11 if bag <= last`, so donor bag 11 still falls through to `false`.
- `glue_select.rs` / `wrath_natives.rs`: the WMC1 request read nonexistent `Model.player_guid`
  and `GlueCore.characters`/`selected_character`; it now uses `unit_guids.player` and a new
  `GlueRoster::selected_guid()` accessor. The round-trip test used the nonexistent
  `UiScript::new_glue_with_target`; corrected to `glue_with_target`.
- Six read-only consumer systems had `///` doc comments on Bevy system parameters; changed to
  `//` (`ui_action/feed`, `ui_craft`, `ui_mail`, `ui_shapeshift`, `ui_tooltip/spell_feed`,
  `ui_tradeskill`).
- `wrath.rs`: qualified `Ask::Read` and `ITEM_FLAG_CHARTER`; the WMC1 bridge now unwraps the
  nested `Option` correctly (`opt.as_mut().as_mut()?`); the container-identity test calls the
  sibling `command_tap_tests::view` helper.
- `ui_char.rs` / `ui_merchant/mod.rs`: read `inv.changes.moved()` before the player view to keep
  the mutable change counter out of the view borrow.
- `ui_items/calls.rs`: replaced the `player_inv` closure with an explicit-lifetime nested
  function so the returned `PlayerInventory` can borrow both the store and the typed owner.
- `item_watch.rs`: the bag-cache test literal now has the 11 Wrath bag slots.
- `wrath_item.rs`: the creator-clear test's local `row` binding is `observed`, so the helper
  function stays callable.

### Inventory and docs (R06 / release review)

- `WrathInventory::apply` no longer emits an `InventoryChange` for a first observation of a
  delivered zero; later observations still report clears. This makes
  `a_first_empty_observation_is_not_a_clear_event` consistent with the code.
- `ItemSteps::container_space` keyring uses `inv.keyring_capacity()` (donor 16, Wrath 32) rather
  than the Wrath constant unconditionally.
- Traceability: the economy row no longer calls the item payload deferred; the player-window
  follow-up records `0..PLAYER_END` (1326). Release review/plan/matrix no longer claim the
  multiclass bindings are absent; they are source-added and unverified.

## Checks performed

- `cargo check -p benilla-app -p benilla-hotpatch -p benilla-ui --all-targets`: **passed**
  (`EXIT=0`, warning-only) on the repair branch.
- `cargo build -p benilla` (debug): **passed** (`EXIT=0`, 5m48s); produced
  `target\debug\benilla.exe` (157,670,400 bytes).
- Focused tests (all 1 test, 0 failed):
  `benilla-hotpatch::tests::host_evaluate_rejects_malformed_rig_spans_and_non_finite_bones_before_module_dispatch`,
  `benilla-ui::script::wrath_natives::tests::wrath_multiclass_request_and_poll_round_trip_in_world_and_glue`,
  `benilla-app::wrath_inventory::tests::a_first_empty_observation_is_not_a_clear_event`.
- `git diff --check`: clean; explicit-path staging only; no reset/clean/rebase/force-push.
- No client, server, DB or live session. Receiver behavior and visible gameplay remain UNRUN.

## Remaining acceptance

- Run a full-workspace check/build (the authorized slot covered
  `benilla-app`/`benilla-hotpatch`/`benilla-ui` and the `benilla` debug binary).
- Runtime: one multiclass request/reply via the installed UI; one malformed pose batch; one bank
  slot-25/bank-bag-7 clear/reload to confirm no spurious slot-change event.
- The uncommitted recorder/pose work in `D:\Projects\rustwrath-main-integration` is untouched and
  not part of this branch.
