# Wrath loot window owner rows

Status: ready to merge
Owner: Codex
Branch: fix/wrath-loot-owner-rows
Issue: none
Next: During the Northshire manual launch, compare manual and automatic pickup of a solo corpse's owner rows and check a locked group row.

The donor sweep took only type-0 rows, silently skipping the type-4 owner rows
AzerothCore sends for solo loot. The Wrath sweep now takes both types. The manual
click refuses locked rows with the reference error, waits for the item record,
and skips binding prompts for owner and multi-drop rows. The sweep uses the same
binding rule. There is no change to the donor's row rules.

Reference: `CGLootInfo::DoAutoLoot` (`0x00588F90`), `CGLootInfo::LootSlot` in
`Ui/LootFrame.cpp`, and AzerothCore `LootMgr.h::LootSlotType`. The manual
non-exempt binding path still leaves unique-item validation to the server.

Source review and `git diff --check` only. No build, tests, or client launch.
