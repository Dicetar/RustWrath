# OpenCode checkpoint - movement bits and command admission repair

Date: 2026-10-26
Owner: OpenCode (client/gameplay integration lead)
Base HEAD: `3c2ab435` (corrected O5 pins; Astra's review `23ec4589` read)
Work package: O5 follow-up blockers from Astra's source review; pre-O6 source repair.
Purpose: correct the Wrath movement-flag translation and stop replaceable movement from
evicting critical commands. Source-only; no build, test, formatter, renderer or live run.

## File claim

Editing:

- `crates/benilla-app/src/wrath.rs`
- this checkpoint

Not editing: Astra's procgen/compatibility/architecture paths, the player controller's own
1.12 flags, or any stock net/UI owner.

## Movement flags

The shared controller stores the **1.12 client's** movement bits
(`crates/benilla-app/src/creature_anim/select.rs:64-101`); 12340 defines them differently
(`UnitDefines.h:373-407`). The adapter previously rewrote only swimming (and to the wrong bit)
and read `0x0200_0000` as transport (that is 3.3.5 FLYING). Repaired mapping, applied by the new
`wrath_move_flags` at both Wrath sites (the streamer and the self-pose correction):

| Meaning | 1.12 controller bit | 12340 bit | Handling |
|---|---|---|---|
| ROOT | 0x1000 | 0x0800 | rewritten |
| FALLING | 0x2000 | 0x1000 | rewritten; drives JUMP/FALL_LAND edges and the fall payload |
| FALLING_FAR | 0x4000 | 0x2000 | rewritten |
| ON_TRANSPORT | 0x02000000 | 0x0200 | rewritten; the streamer still skips a transport-local pose |
| SWIMMING | 0x200000 | 0x200000 | unchanged; drives the swim pitch block |
| walking, steering, levitation/disable-gravity, water-walking, safe-fall, hover | — | same bits | unchanged |

The fall-edge detection compares the translated 3.3.5 bits on both sides, so a jump starts on
3.3.5 FALLING and lands when it clears, and a rooted player is no longer sent as falling.

## Command admission

`WrathOutbound` now records whether an entry is replaceable motion. The movement streamer's
queue cap (64) evicts only the **oldest movement entry** and refuses its own pose when the queue
holds none; it can no longer `pop_front` a cast, quest, loot, death or logout command that the
action tap admitted. The tap keeps its 256 cap for real actions and now logs a warning when it
must refuse one, instead of dropping it silently. Refused critical commands remain a bounded,
visible loss until O6's typed action admission replaces this seat; unmapped commands are still
not admitted at all (the existing bounded surface).

## Pending

- Compilation and any focused tests for the flag map and queue behavior are NOT run (execution
  restriction). The queue logic is source-reviewed only.
- The transport-local pose is still refused (the transport block is a later route child).
- Astra's R01/R03 are addressed in source; receiver/live evidence remains O6/lab work.

## Evidence level

Source and 12340-header reads only. G/C/L/W gates unchanged: UNKNOWN.
