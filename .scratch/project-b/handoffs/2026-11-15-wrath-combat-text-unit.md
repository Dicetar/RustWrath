# Route Wrath combat text to the unit selected by stock Lua

Status: ready to merge
Owner: Codex
Branch: fix/wrath-combat-text-unit
Issue: none
Next: include in the owner's build batch and enable stock scrolling combat text in a manual Wrath session

Stock Blizzard_CombatText tries to set the native combat-text unit when
enabled and on a vehicle transition. The missing native interrupted that
script before it registered its combat-text event handlers.

The Wrath VM now resolves the requested token to a GUID as the reference
does. A nil or unresolved token clears the selected GUID. The existing
packet formatters carry the combat-text recipient through the chat queue.
The dispatch compares that GUID with the current selection before firing
`COMBAT_TEXT_UPDATE`. Chat text still follows its own formatting and
routing rules. The 1.12 formatters keep their existing player-only test.

The reference pin is `e67b6ffef23655dc7da061cb2d36b7963f6ef71a`.
`UnitCombatLog_C_Scripts.cpp::Script_CombatTextSetActiveUnit` at
`0x0074D580` captures a GUID via `GetGuidByKeyword`. The missed-spell
consumer at `0x00750500` compares the destination GUID directly.

This covers the words produced by the app's combat packet chat formatters.
The raw combat-log history and separate aura watcher are outside the change.
Live vehicle switching still needs the session to populate the vehicle token
and emit the seat transition events. The regression case injects a vehicle
identity and covers changing the selected unit and clearing it without
affecting the underlying chat line. Source review and `git diff --check`
passed. No build, tests, or client launch ran.
