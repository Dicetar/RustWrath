# Wrath local vehicle identity

Status: ready to merge
Owner: Codex
Branch: `fix/wrath-vehicle-unit`
Issue: none
Next: Include in the manual Wrath launch batch and compare driver and passenger seats across seat changes, exit, and transfer.

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
The local seat now resolves through `Vehicle.dbc` (40 columns; seat IDs at
columns 6–13) and `VehicleSeat.dbc` (58 columns; flags at column 1), using the
active asset chain. `UnitHasVehicleUI` and `UnitInVehicleControlSeat` use their
own flags (`0x00613740` and `0x00613700`). The exit and switch gates use
`CGInputControl::IsVehicleSeatFlagSet` (`0x005FB560`), with the numeric/nil
returns from `0x005FB9C0` and `0x005FBA10` and the boolean switch query from
`0x00608580`. Missing rows do not grant controls.

This follows the local wire association. Exact animation timing, its Lua events,
remote/roster seat queries, and nested vehicle seat routing remain separate work.

Source review and `git diff --check` only. No build, tests, or client launch.
