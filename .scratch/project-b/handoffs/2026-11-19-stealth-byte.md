# Stealth byte boundary

Status: landed
Owner: Astra / OpenAI
Branch: fix/astra-stealth-byte
Issue: none
Next: Owner checks hover, stealth and crouch in game.

## What changed
Verified 12340 reference pin e67b6ffef23655dc7da061cb2d36b7963f6ef71a:
Unit_C.cpp:5128/5130 registers visibility at byte 2 and animation tier at byte 3;
:9970/9971 treats animation tier 2 as hover.
The Wrath bridge already translates byte 2 into donor byte 3 (27d3b09d).
Preserved that correct boundary and every donor reader, including nameplates;
changing unit_is_stealthed to byte 2 would instead read the donor shapeshift form.
Named the conversion, clarified the accessor, added hover/CREEP projection cases,
and removed the stale roadmap item. Searched all old-byte readers with git grep.

## CI run id and result
44432259005: Linux app tests pass, including both projection cases; two inherited UI test failures were subsequently fixed on main.
44434902356: Windows workspace check and UI/protocol-wrath tests pass; Linux tests pass.
Only untouched player-clippy debt remains red. No local Cargo was run.

## What is not proven in game
Hovering, stealth/crouch poses, nameplates and local translation overlays were not exercised in game.
