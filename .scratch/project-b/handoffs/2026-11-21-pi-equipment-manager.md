# Equipment manager: saved gear sets end to end

Status: ready to merge
Owner: Claude Code / Claude Opus 5.5 (run from the brief written for pi)
Branch: pi/equipment-manager
Issue: none
Next: coordinator reviews and merges; owner checks the gear-manager dialog in game

## What landed

- Protocol (`benilla-protocol-wrath`):
  - `opcode.rs` gains the six opcodes.
  - `gameplay/equipment_set.rs` decodes `SMSG_LOAD_EQUIPMENT_SET` (a count of at most 10, then
    per set: packed guid, `u32` id, name, icon, 19 packed slot guids), `SMSG_EQUIPMENT_SET_ID`
    (`u32` id, packed guid) and `SMSG_USE_EQUIPMENT_SET_RESULT` (`u8`).
  - It encodes save (packed guid, id, name, icon, 19 packed guids), delete (packed guid) and use
    (19 × packed guid, bag byte, slot byte).
  - The decoder rejects a truncated body, trailing bytes and an over-long count.
  - `ServerEvent` gains `EquipmentSetList`, `EquipmentSetSaved` and `EquipmentSetUseResult`.
- Opcode ledger: a new "Equipment manager" table in `docs/baseline/opcode-ledger-v0.md`.
- App owner (`benilla-app/src/wrath_equipment.rs`): `EquipmentManager` holds each set (id, name,
  icon, 19 slot guids with `1` as the ignored marker, `is_new`), `m_loaded`, `m_swapPending`
  and `m_pendingSetID`.
  - Filled by the login list through `WrathFeedEvent::EquipmentSet` in `wrath_net_drain`, with
    the scope check.
  - A repeat list of the same count is skipped (`0x5AF490`).
  - The saved-id reply settles a set, or drops it when the guid is 0.
  - The use result clears the pending swap.
  - The session end clears everything (`ShutdownGame`); a world transfer resets the swap
    (`EnterWorld`).
- Ignored slots: the ignored-slot mask lives in the VM, which owns the four ignore natives; each
  save carries a copy of the mask.
- `EquipmentSteps` is an `Appliers` member, so it is lent alone and avoids the host's B0002
  access conflict.
  - It applies `ScriptCall::EquipmentSet`:
    - save or create: lowest free id, worn guids or `1`, the send, `EQUIPMENT_SETS_CHANGED`;
    - delete;
    - rename;
    - use: each slot's item, bag and slot per `0x5ADA20`, only the weapons in combat, the
      combat and missing-item warnings, the send, the pending swap, `EQUIPMENT_SWAP_PENDING`.
  - It answers the new host query `equipment_item(guid)` (entry and place).
- `feed` (`VmWith`) copies the owner into the VM when it changes. It signals
  `EQUIPMENT_SWAP_FINISHED(ok, name)`, and `UI_ERROR_MESSAGE`
  `EQUIPMENT_MANAGER_BAGS_FULL` for result 4.
- Natives (`benilla-ui/src/script/equipment.rs`, Wrath VM only), each following its Script_*:
  `GetNumEquipmentSets`, `GetEquipmentSetInfo`, `GetEquipmentSetInfoByName`,
  `GetEquipmentSetItemIDs`, `GetEquipmentSetLocations`, `SaveEquipmentSet`,
  `DeleteEquipmentSet`, `RenameEquipmentSet`, `UseEquipmentSet`, `CanUseEquipmentSets`,
  `EquipmentSetContainsLockedItems`, `EquipmentManagerIgnoreSlotForSave`,
  `EquipmentManagerUnignoreSlotForSave`, `EquipmentManagerIsSlotIgnoredForSave` and
  `EquipmentManagerClearIgnoredSlotsForSave`.
  - `SaveEquipmentSet` scrubs the name as `ScrubString(.., 32, .., 1)` (`0x7E1830`), raises
    `Too many sets!` past ten, and takes a macro icon or a worn slot's art.
- CVar: `equipmentManager` moves from the inert table to `REGISTERED`. `CanUseEquipmentSets`
  (`0x5AD6B0`) reads it, and `UseEquipmentSet` refuses while it is 0, with
  `EQUIPMENT_MANAGER_IS_DISABLED` (or silently while a swap is pending).
- Removed: only the `GetNumEquipmentSets` constant-0 stub in `wrath_natives.rs`. The neighbouring
  `GetBackpackCurrencyInfo` stub is untouched (claudeJr's); if the merge conflicts there, keep both
  removals.

## Unsupported, with reason

- `PickupEquipmentSet` and `PickupEquipmentSetByName` (`0x5AF2C0`, `0x5AF250`) are registered
  and recorded with `record_unsupported_native_once`; they do nothing. Reason: the cursor has no
  equipment-set payload (`CGGameUI::SetCursorEquipmentSet`) and no action slot can hold a set.
  Dragging a set to the action bar does nothing.
- Action-bar set buttons (`CGActionBar::UpdateEquipmentSet`, `RemoveEquipmentSet`) and the
  usable refresh `CanUseEquipmentSets`/the use result run (`CGActionBar::UpdateUsable`) are not
  modelled, for the same reason.
- `WEAR_EQUIPMENT_SET` (`CGEquipmentManager::EquipSet`, `0x5AE660`) is not fired: its only
  caller is the action bar's set button.

## Reference addresses

Pin e67b6ffe, `WoW/Source/Ui/EquipmentManager.cpp` and `EquipmentManager_Scripts.cpp`.

- Packets: `Packet_SMSG_EQUIPMENT_SET_LIST` `0x5AF490`, `_SAVED` `0x5AF6C0`, `_USE_RESULT`
  `0x5AF710`.
- Manager: `SaveSet` `0x5ADE50`, `DeleteEquipmentSet` `0x5AE260`, `UseEquipmentSet` `0x5ADA20`,
  `CreateEquipmentSet` `0x5AF7E0`, `SaveSetByName` `0x5AF8E0`, `ModifyEquipmentSet` `0x5AF400`,
  `CanCreateSet` `0x5AF460`, `GetNextFreeSetID` `0x5AE530`, `OnEquipmentSetSaved` `0x5AE760`,
  `CanUseEquipmentSets` `0x5AD6B0`, `SetContainsLockedItems` `0x5AD940`, `EnterWorld`
  `0x5AD690`, `ShutdownGame` `0x5AE1B0`.
- Script functions: `0x5AD770`, `0x5AD830`, `0x5AD7C0`, `0x5AD820`, `0x5AD880`, `0x5AE800`,
  `0x5AE860`, `0x5AEF90`, `0x5AF0D0`, `0x5AF120`, `0x5AF1D0`, `0x5AF320`, `0x5AF380`, `0x5AF910`,
  `0x5AF9C0`.
- `ScrubString` `0x7E1830`.
- Opcodes `Net/Types.h:319-1246`, cross-checked against AzerothCore `Opcodes.h:341-1268`.
- AzerothCore writers and handlers: `Player::SendEquipmentSetList` `Player.cpp:14780`;
  `HandleEquipmentSetSave`/`Delete`/`Use` `CharacterHandler.cpp:1746-1900`.

## Unverified or deviating

- The location getter's order: `GetEquipmentSetLocations` searches equipped slots, then the
  backpack, bags and bank, removing each found slot from its pending list. Here each item's place
  is asked once by guid, through the same owners. The answer differs only if one guid fills two
  set slots.
- A set's guid whose low 32 bits are 0 reads 0 in `GetEquipmentSetItemIDs`, the reference's
  `(uint32_t)guid` test, kept as is.
- The slot icon of `SaveEquipmentSet(name, -n)` comes from the visible item's template display,
  else the worn item object's entry template. The reference reads the visible item's display
  directly (`GetDisplayID`).
- `UseEquipmentSet`'s answer is read back as "the swap is now pending", since a script call
  returns no value. That is the same condition the reference returns true on.

## CI

- CI 44948925092 on 534d4210e: compiles; tests pass. Both jobs passed:
  - `gates`: fmt, clippy, the `changed` step testing benilla-app, benilla-protocol-wrath and
    benilla-ui (benilla-app: 6900 passed, 0 failed), `player-clippy`, and `player-tests`
    (benilla-app `--no-default-features`: 6834 passed; benilla-formats: 590 passed);
  - `windows`.
- Locally (own target dir `worktrees/pi-equipment-manager/target`, sccache):
  - `cargo check -p benilla-app --tests`;
  - `cargo test -p benilla-protocol-wrath --lib equipment_set` (3 passed);
  - `cargo test -p benilla-ui --lib equipment` (the new scrub test passed);
  - `cargo test -p benilla-app --lib wrath_equipment` (10 passed);
  - `cargo fmt`;
  - `cargo clippy -p benilla-protocol-wrath -p benilla-ui -p benilla-app --lib
    --no-default-features -D warnings` (clean).
- 14 new test cases in total.

## Unproven in game

- No client or server was run.
- A real login list from AzerothCore.
- The stock PaperDoll gear-manager dialog (GearManagerDialog) populating, saving, deleting and
  equipping.
- The swap's server result and the bag-full path.
- Bank-bag lookups with a banker open.
- The `equipmentManager` option (Interface Options) turning the button on.
