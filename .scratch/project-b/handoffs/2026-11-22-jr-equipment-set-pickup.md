# Equipment sets on the cursor and the action bar

Status: ready to merge
Owner: Claude Code / Claude Opus 5.5
Branch: jr/equipment-set-pickup
Issue: none
Next: coordinator reviews and merges; owner drags a set onto the bar and clicks it in game

## What landed

Follows `pi/equipment-manager` (handoff `2026-11-21-pi-equipment-manager.md`), which recorded the two
pickups as unsupported.

- Cursor (`benilla-ui/src/script/cursor.rs`): `CursorPayload::EquipmentSet(CursorEquipmentSet { id,
  texture })`, 12340's `UICURSOR_EQUIPMENT_SET`. Set through `SetCursorEquipmentSet` (`0x520DC0`):
  refused before its clear for an unknown id, then `ClearCursor(1,1)`, the set, the action-bar
  grid (the macro setter's tail). The clear's arm writes id -1 (`GameUI.cpp:2553`). On the world
  it drops like a macro. The app shows its icon at the mouse (`benilla-app/src/cursor.rs`, the
  capture overlay) and plays the generic grab/drop pair.
- `GetCursorInfo` (`wrath_natives.rs`): `"equipmentset", name` (`GameUI_Scripts.cpp:2921-2931`),
  nothing once the set is gone.
- Natives (`equipment.rs`): `PickupEquipmentSet(index)` (`0x5AF2C0`) and
  `PickupEquipmentSetByName(name)` (`0x5AF250`) replace the unsupported records.
- Action bar (`cursor/bar.rs`, `action.rs`):
  - A set slot is kind `0x20`, word `0x20000000 | id` (`PutActionInSlot` `0x5AB120`,
    `GetEquipmentSetID` `0x5A7950`). AzerothCore takes it as `ACTION_BUTTON_EQSET` (`Player.h:224`;
    `HandleSetActionButtonOpcode`, `MiscHandler.cpp:908-933`), so `PlaceAction` sends
    `CMSG_SET_ACTION_BUTTON` with that word through the existing slot write.
  - `PlaceAction` with a set held stores it; the same set over its own slot clears the cursor
    and writes nothing. `PickupAction` lifts a set slot while its set exists
    (`IsEquipmentSetValid` `0x5AE4F0`) and clears the slot either way (`0x5ABE70`).
  - Read live from the VM's set list, as macros are: `GetActionTexture` (the set's
    `GetFullIconPath`, `""` once gone), `GetActionInfo` (`"equipmentset", name`),
    `GetActionText` (the name). `IsEquippedAction`, `GetActionCount`, `IsConsumableAction`,
    `IsStackableAction` answer nil/0, as 12340 does for a slot with no item.
  - `IsUsableAction`: the set gates of `UpdateUsableAction` (`0x5A9E20`) in the VM
    (`CanUseEquipmentSets`, the set listed, `SetContainsLockedItems`, `SetUsesLockedSlots`), AND the
    app's pushed state (`ui_action/state.rs`): usable while in control or stunned (`0x100000`).
    No cooldown query.
  - `UseAction` on a set slot is `EquipSet` (`0x5AE660`): `WEAR_EQUIPMENT_SET` with the name,
    signalled in the call; no press reaches the app. The stock `EquipmentManager.lua` handler then
    calls `UseEquipmentSet`, the existing path.
- Rename and delete:
  - `DeleteEquipmentSet` first clears every slot of the set, lowest first, each sent as 0 with
    `ACTIONBAR_SLOT_CHANGED` (`RemoveEquipmentSet` `0x5AAE20` → `RemoveAction` `0x5AAA90`), then
    the existing delete.
  - A save of a set the server has numbered (rename included, `ModifyEquipmentSet` → `SaveSet`)
    signals each slot holding it (`UpdateEquipmentSet` `0x5AAA00`) after the copy and before the
    send (`wrath_equipment.rs` `save_set`, `UiScript::signal_equipment_set_slots`).

## Reference addresses

Pin e67b6ffef23655dc7da061cb2d36b7963f6ef71a.

- `EquipmentManager_Scripts.cpp`: `Script_PickupEquipmentSetByName` `0x5AF250`,
  `Script_PickupEquipmentSet` `0x5AF2C0`.
- `EquipmentManager.cpp`: `DeleteEquipmentSet` `0x5AE260`, `SetUsesLockedSlots` `0x5AE040`,
  `EquipSet` `0x5AE660`, `GetFullIconPath` `0x5AE6B0`, `SaveSet` (`UpdateEquipmentSet` call,
  lines 651-653).
- `GameUI.cpp`: `SetCursorEquipmentSet` `0x520DC0`, `ClearCursor`'s set arm (lines 2552-2555);
  `GameUI_Scripts.cpp` `GetCursorInfo` (lines 2921-2931).
- `ActionBarFrame.cpp`: `IsEquipmentSet` `0x5A78F0`, `GetEquipmentSetID` `0x5A7950`,
  `UpdateUsableAction` `0x5A9E20`, `GetTexture`'s set arm (line 851), `RemoveEquipmentSet`
  `0x5AAE20`, `PutActionInSlot` `0x5AB120`, `UseAction` `0x5ABBC0` (line 1387),
  `UpdateEquipmentSet` `0x5AAA00`, `IsEquipmentSetValid` `0x5AE4F0`, `PickupAction` `0x5ABE70`,
  `IsEquippedAction` `0x5A88B0`.
- `ActionBarFrame_Scripts.cpp`: `GetActionInfo` (lines 130-142), `GetActionText` `0x5A7D90`.
- AzerothCore: `ACTION_BUTTON_EQSET = 0x20` (`Player.h:224`), `HandleSetActionButtonOpcode`
  (`MiscHandler.cpp:908`).

## Not modelled, with reason

- `CanPerformAction(14)`, the tainted-call refusal in `PickupEquipmentSetByName`: the existing
  `PickupAction`/`PlaceAction` verbs, which share it in 12340, do not model it either.
- The login button load's drop of a set slot whose set is not listed (`Player_C.cpp:4113`):
  such a slot stays, reads as empty (`""` texture, no info) and does nothing when used.
- `CGActionBar::UpdateUsable` when `CanUseEquipmentSets` changes: `IsUsableAction` reads the set
  gates live, so no `ACTIONBAR_UPDATE_USABLE` fires on a swap start or end.
- The stock `WEAR_EQUIPMENT_SET` handler was not read here (no extracted FrameXML on this
  machine); the claim that it calls `UseEquipmentSet` is from the 3.3.5 `EquipmentManager.lua`.

## Checks

- `rustfmt` on the touched files. No Cargo run locally.
- Six new cases in `benilla-app/src/wrath_equipment/tests.rs`, narrow `FnHost`: pickup to cursor,
  place and the bar queries, use → `WEAR_EQUIPMENT_SET`, pickup off the bar, rename repaint and
  delete clear, the usable gates.
- CI 45003577207 on 67166c84f: compiles; tests pass. Both jobs passed:
  - `gates`: fmt, clippy, the `changed` step's tests (benilla-app 6923 passed, benilla-ui 3553
    passed, 0 failed), `player-clippy`, and `player-tests` (benilla-app `--no-default-features`:
    6857 passed; benilla-formats: 591 passed). The six new cases appear by name in both test runs.
  - `windows`: check, test-build, test.

## Unproven in game

- No client or server was run.
- Dragging a set from the gear-manager dialog onto a bar, the icon at the mouse and on the button.
- The server keeping the button across a relog (`SMSG_ACTION_BUTTONS` with a `0x20` word).
- Clicking the button equipping the set through the stock handler.
