# Wrath private alpha integration candidate

Status: in progress
Owner: Codex
Branch: integration/wrath-alpha
Issue: none
Next: Build and run the candidate against the existing Northshire run card; preserve the first failure and its exact revision.

This isolated branch now includes the mainline batch through `b700d540`.
Outside this note, its tree differs by the character-login refusal and rename
fix, published for mainline review as `fix/wrath-character-refusal-codes`.
The owner’s current checkout is separate. Build and launch remain the owner's
steps. The candidate is not evidence of a completed playthrough.

The integration includes login and world entry, movement and mover orders,
transfer recovery, quest log seating and watch changes, solo and group loot,
local vehicle seat permissions, combat text, Lua errors, chat load timing,
auction categories, the rune owner fix, and headless command fixture repairs.
It also takes the server's batched quest marker refreshes, preserves all 128
Wrath reputation slots, and accepts the 12340 chat filter tables.
The overlapping UI changes retain the target-specific button text layers, the
region registry fix, and the installed-font diagnostics. Both the vehicle and
item limit DBC catalogs load at startup.

Source audit corrected the character-login refusal map against the 12340
`WowSvcsClient/Types.h` enumeration and AzerothCore's `ResponseCodes`; the
queued branch used values from the character-creation range. Rename now accepts
the reply layout AzerothCore actually emits and reports a malformed reply as a
failed rename without dropping character selection.

Do not switch the user's dirty checkout to build this branch. The candidate
worktree can be built in isolation, using the existing installed profile. The
login/roster, Firna quest acceptance, combat, solo loot, turn-in, and same-character
re-entry steps are the private alpha gate. Compilation, tests and runtime
acceptance have not been run here.
