# Cast interruptibility tests

Status: in progress
Owner: Pi
Branch: pi/interruptibility-tests
Issue: none
Next: Add focused interruptibility tests and document reachable versus unavailable inputs.

What changed: Initial task note. The existing implementation is being checked against the pinned 12340 reference before tests are added.

Reference addresses: `CGUnit_C::ProcessCastNotInterruptible`, `Unit_C.cpp:11745` / `0x007262E0`; `CGSpellBook::UpdateSpellList`, `SpellBookFrame.cpp:780-810`; `CGUnit_C::SetCastImmunities`, `Unit_C.cpp:6112` / `0x00715AB0`; `CGUnit_C::SetCastSchoolImmunities`, `Unit_C.cpp:6107` / `0x00715AA0`.

CI run id and result: CI not run yet.

What is not proven in game: No local Cargo, client, server, login or in-game run is authorized; gameplay behavior remains unproven.
