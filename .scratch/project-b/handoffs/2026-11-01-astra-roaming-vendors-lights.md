# New visible defects: source repairs and next handover

Canonical repository: https://github.com/Dicetar/RustWrath, shared `wotlk` checkout.
The owner reports coherent rig/no giant planes, frozen roaming models and selection circles,
camera-dependent magenta lighting, no Space jump, swimming in dry interiors, a horizontal
pose persisting outdoors until RMB movement, and vendors raising a hearthstone binding prompt.
Combat, action bars, inventory and loot are still broken; OpenCode retains those consumers
and the cast-state gate in spellbook.rs/ui_cast regardless of earlier authorship.

## Binary correction

Both human runs used SHA-256
5F4BD0DABE7B1252D9E67A62CD00BCC9013D3936112164C2661A67F6CD46B50A.
It already included 476cbb96 and 6edea9f5. The later run's metadata pins 9c9fb301, and
`astra-playable-graphics-build-fixed.log` records the successful client build. These UI
failures must be reproduced through the real receiver/click path, not attributed to an old
binary. 3dec692e is preserved; the earlier loader code had already compiled successfully.

## Astra source changes

1. Server movement: the Wrath gameplay decoder discarded opcode 0x0DD. A focused real-layout
   stop packet reproduced None before the fix. New typed parsing handles the Wrath movement
   byte, transport seat, facing, optional animation/parabola tails and bounded point arrays.
   Linear packed offsets use the start/end midpoint, not the donor's endpoint origin.
   Ordinary remote ground paths now feed the existing SessionEvent::MonsterMove spline owner.
   Curved/cyclic/ballistic/frozen/fixed-orientation paths and self rides remain unsupported by
   this translation; decoded raw flags remain available. Do not claim full Wrath spline parity.
   A replicated-position stamp also prevents unrelated descriptor refreshes from resetting a
   completed/in-progress spline to its old create position. Scale survives pose reseating.

2. Services: raw Wrath NPC flags were copied directly to the 1.12 descriptor. Wrath VENDOR
   0x80 became donor INNKEEPER 0x80. The projection now translates the established service
   bits explicitly; typed Wrath state retains the original flags. No target/click.rs edits.

3. Lighting: the production parser decoded 37,415 lights from the actual HD HumanFemale M2
   whose v264 light table declares zero. It read donor header offsets as an unrelated table.
   The reader now uses v264 lights @0x108/0x10c, 156-byte records, seven 20-byte nested tracks,
   and the correct bone/packed-quaternion layout for a directional light basis. It does not
   interpret external .anim offsets as inline light keys. Non-finite light records are rejected.
   The same real file now reports declared=0 decoded=0. Magenta improvement still needs a
   rendered check; this source/data repair does not establish that all lighting is correct.

## Evidence and limits

Two focused monster-movement cases passed, including the previously discarded stop packet,
midpoint waypoint decoding, truncated payloads and an oversized point count. Three focused
record cases passed (two Wrath lights plus the existing donor bone-axis case).
Evidence remains ignored under `.scratch/project-b/verification/2026-10-28-oc-alpha/`:
`astra-monster-move-{before,after}.log`, `astra-wrath-lights-tests.log`,
`astra-light-source-{before,after}.log`, `astra-roaming-vendor-build.log`.
The roaming/vendor build and combined lighting build both passed. Consult
`astra-roaming-vendor-lights-build.log` and active-ownership.md before taking the build slot.
The combined binary SHA-256 is
42C19FF18A16418C7A498B4969C0A7851E9FEC20D49CA8A49BDCF80AA75CC995.
No client/cargo/rustc process remains; the slot is released. In-game acceptance is pending.

No new client has been launched after the owner objected to repeated unchanged launches.
The next visible check should specifically compare wandering model/circle motion, a vendor
opening the merchant flow, and magenta while rotating the same camera. Do not repeat an
unchanged build and call that a rerun of these fixes. False swimming, horizontal pose and
Space jump remain open for Astra's movement/water-state diagnosis; no repair is claimed.
No automated combat, server/DB/install/credential changes, or playable-alpha acceptance.

## References inspected

Local AC MovementPacketBuilder.cpp, MoveSplineInit.cpp, MoveSplineFlag.h, Opcodes.h,
UnitDefines.h. Preserved 12340 reference pin e67b6ffef23655dc7da061cb2d36b7963f6ef71a:
Engine/Source/Model2/M2Data.h M2Light/M2Data layouts; M2Shared.cpp M2Init track relocation;
WoW/Source/Object/CMoveSpline.cpp extraction (0x004F4B50), whose create-spline shape is not
the same as the monster-move packet writer. Existing donor net/objects.rs and net/motion/spline.rs
are reused for the supported ordinary ground path. No decompiled implementation copied.
