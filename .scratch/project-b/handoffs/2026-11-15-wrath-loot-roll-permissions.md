# Wrath group loot permissions

Status: ready to merge
Owner: Codex
Branch: fix/wrath-loot-roll-permissions
Issue: none
Next: Include in the manual launch batch and compare two eligible players with different Need or Disenchant permissions.

AzerothCore sends the stack count and recipient vote mask in
`Group::SendLootStartRoll`/`SendLootStartRollToPlayer`. The existing 12340 parser
already decoded both; the translation into the app discarded them. The app now
keeps both with the live roll and exposes the count and the Need, Greed and
Disenchant flags through `GetLootRollItemInfo`. Lua and the app reject votes that
the server mask disallows without closing the roll. Rare or better disenchant
votes request the Wrath confirmation event before submission. The 1.12 roll
continues to use its own result and confirmation shape.

12340 reference: `LootFrame_Scripts.cpp::Script_GetLootRollItemInfo` at
`0x005889D0`, and `Object/ObjectClient/LootRoll.cpp::LootRoll` at `0x006FBC70`.
The stock query's trailing four tooltip values (three local refusal reasons and
the required enchanting skill) are outside this slice.

The loot chat path now keeps the Wrath auto-pass flag, displays disenchant votes,
dice, and winners using their distinct strings, and formats Wrath's strings with
one complete item link. The player's own auto-pass stays visible with
`showLootSpam` off. See `LootRoll.cpp::RolledStatsAndName` (`0x006FA840`) and
`WonStatsAndName` (`0x006FB700`).

Source review and `git diff --check` only. No build, tests, or client launch.
