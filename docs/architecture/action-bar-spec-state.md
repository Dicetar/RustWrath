# Wrath action-bar spec changes

Source fix for D07 in the [replacement inventory](../../.scratch/project-b/issues/18-stock-replacement-inventory.md), based on `a83a4d0439781dcd9df75f23c38cd8086e171870`.
`SMSG_ACTION_BUTTONS` now retains the state byte and reads exactly 144 packed slots for states
0/1. A truncated data-bearing packet is rejected instead of replacing the bar with a partial
set; trailing data cannot produce slots above 143. Only state 1 reaches the existing
`SessionEvent::ActionButtons` consumer, which replaces `ActionSlots` and marks it dirty.
State 2 and unknown states no longer turn into empty bar replacements. State 0 is decoded
but not applied to the visible bar.

Evidence: preserved reference pin `e67b6ffef23655dc7da061cb2d36b7963f6ef71a`,
`WoW/Source/Object/ObjectClient/Player_C.cpp::Packet_SMSG_ACTION_BUTTONS` (OFFSET
`0x006D8750`): state 2 sets a pending flag, state >1 returns, state 0 reads the array,
and state 1 reads and applies it. The reconstruction includes a separate HoT change to
spell validation; it was not used for this fix. The local AzerothCore
`Entities/Player/Player.cpp::SendActionButtons` emits state 2 before a spec switch and
state 1 with the complete set afterward; its comment says state 2 clears the bar, which
differs from the reference body. The old client-side pending flag and state-0 staged array
are not yet exposed to consumers here. This change preserves the current bar, but does not
claim the interaction lock or other pending-state behavior is complete.

No tests, build, client or server session were run. Follow up with a focused decoder and
session/UI test for state 2 followed by state 1, state 0, unknown states, truncated and
oversized data, empty and populated slots 0/143; then have the owner exercise a real spec
switch and reconnect. Existing action-button persistence and concurrent action edits need
separate acceptance. No server or player data migration is introduced.
