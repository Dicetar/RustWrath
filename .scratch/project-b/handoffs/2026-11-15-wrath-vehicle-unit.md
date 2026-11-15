# Wrath local vehicle identity

Status: ready to merge
Owner: Codex
Branch: `fix/wrath-vehicle-unit`
Issue: none
Next: Include in the manual Wrath launch batch and check enter, seat change, exit, and transfer.

The reducer preserves the passenger's transport GUID and seat from living movement
blocks and monster movement packets. It only resolves a vehicle token for a held
base with vehicle data. That data comes from the update block's VEHICLE tail or
`SMSG_PLAYER_VEHICLE_DATA`, whose zero ID removes the kit. A transport-local
spline does not overwrite the passenger's world position.

The world digest carries the local vehicle GUID to the app token resolver and the
Lua unit and scoped command feeds. Compare `UnitGUID("vehicle")` and
`UnitInVehicle("player")` before boarding, after changing seats, and after exiting
or transferring. An ordinary boat should leave both vehicle checks empty.

12340 reference (preserved revision `e67b6ffef23655dc7da061cb2d36b7963f6ef71a`):
`ScriptEvents.cpp` resolves the token from the seated player's transport;
`Unit_C.cpp` checks passenger state at `0x004F6250`, attaches the mover at
`0x0073F590`/`0x0073C8E0`, and creates or removes vehicle data at `0x00716DB0`.
This slice follows the wire association. Seat UI permissions, exact animation
transition timing, and the associated Lua events remain separate work.

Source review and `git diff --check` only. No build, tests, or client launch.
