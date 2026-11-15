# Wrath movement wire: carry the shared streamer's packets and one client clock

Status: ready to merge
Owner: Claude Code (Fable 5.2)
Branch: fix/wrath-movement-wire
Issue: none
Next: source review, then the owner's batch build; run the 8 focused cases and check fall damage, swimming and a standing mouse turn in game

Matrix row F04 (input and movement). Source only: nothing here was compiled, no test was run and
nothing was seen in game.

## What was wrong

Two systems wrote self-movement on a Wrath session. `movement_net::stream_self_movement` (the
controller's own streamer) produced a complete message for every edge, and the Wrath command tap
dropped all of them. `wrath_stream_movement` then rebuilt a packet from the flag word alone.
Compared with AzerothCore's `MovementHandler.cpp` / `Player.cpp` and the 12340 reference
(`Movement.cpp`, pin `e67b6ffe`), that second writer was wrong in these ways:

| # | Sent | Effect on the server |
|---|---|---|
| 1 | `fallTime` always 0 | `Player::UpdateFallInformationIfNeed` resets the fall origin whenever the reported fall time does not grow, so the fall is measured from the last packet and fall damage is under-applied or missed |
| 2 | Time-sync reply with client time 0, moves on a private clock, acks with time 0 | `HandleTimeSyncResp` derives the clock delta from the reply and adds it to every move's time; the three did not share a clock |
| 3 | Jump tail all zeros | Observers get no launch to extrapolate |
| 4 | Swim pitch always 0 | Observers see a level swimmer; the pitch never reaches the server |
| 5 | `MSG_MOVE_JUMP` for a walk-off | The reference sends a jump only for a jump |
| 6 | No `START_SWIM`/`STOP_SWIM`, `SET_WALK_MODE`/`SET_RUN_MODE`, pitch opcodes | `START_SWIM` is what removes landing-interrupt auras; walk mode never reached observers as an edge |
| 7 | `SET_FACING` every frame of a standing turn; none while running | The reference throttles at 0.1 rad and sends it while moving too; facing while running rode the 500 ms heartbeat |
| 8 | Nothing while on a transport | No pose at all aboard a boat or zeppelin |

It also named the player's own guid as the mover while a possessed unit was driven.

## What changed

- `wrath_tap_commands` translates `ClientCommand::Move` into the 12340 packet
  (`wrath_move_packet`: packed mover guid, then `gameplay::write_movement_info`). The mover is
  `Player::foreign_mover` when set, else the session's player.
- `wrath_stream_movement` and its local state are removed. The shared streamer is the one writer.
- The tap runs a second time in `WorldStage::Stream`, after the controller, so a frame's moves and
  acks reach the outbox that frame and in the order they were sent.
- `wrath_client_ms` is one monotonic clock for the time in every move, in the five movement acks
  the tap builds (they sent 0) and in the `CMSG_TIME_SYNC_RESP` answer (it sent 0).
- Movement keeps its own admission (`admit_wrath_movement`): at the 64-entry cap a new move
  displaces the oldest queued pose report (`HEARTBEAT`, `SET_FACING`, `SET_PITCH`), then the oldest
  queued edge, and never an action. Before, the oldest move went regardless of kind, which could
  drop a `FALL_LAND` in favour of a heartbeat.
- `gameplay::write_movement_info` now owns the rule that a tail travels with its flag. It already
  set the transport and falling bits when a tail was supplied; it now also clears them when none
  is. `move_mode_ack`, `move_spline_done` and `not_active_mover` pass the live flag word with no
  tail, so an ack answered mid-air or aboard a transport announced a tail it did not write and the
  server read the following fields from the wrong offset.
- Comments in `player/controller.rs` and `benilla-hotpatch/src/lib.rs`, the F04 path in the release
  matrix and the C08 row of the traceability table name the current writer.

## Deviations and limits

- A fall that starts without a jump sends nothing on that frame (1.12 behaviour of the shared
  streamer). 12340 sends a heartbeat (`Movement.cpp:4847`). The falling bit rides the next packet,
  at most 500 ms later. Fall damage is unaffected: the origin stays at the highest reported point.
- The shared flag word is 1.12's and cannot say `CAN_FLY` or `FLYING`. Flying mounts are not
  streamed correctly; nothing in this change addresses that.
- The transport block's time and seat are written as 0.
- Possession needs `CMSG_SET_ACTIVE_MOVER`, which is still sent only on self create. Naming the
  possessed unit in the move is necessary and not sufficient.
- During a settle hold the shared streamer runs (it reports the held time on release). The removed
  writer sent nothing while settling. Whether a move goes out during a hold was not traced.
- The heartbeat runs while any sent flag is set, walk mode included; the reference's runs only for
  the directions and falling.
- Whether `Player::ride` is populated on a Wrath session was not checked, so item 8 is repaired at
  the wire only.
- The second tap pass also translates UI commands issued during the Input stage one stage earlier
  than before, against the same frame's view.

## Focused cases to run (8)

- `benilla-app`, `wrath::command_tap_tests`: `stock_commands_become_the_335_messages` (changed: the
  move is now the seventh entry), `an_airborne_move_carries_the_fall_time_and_the_jump_tail`,
  `a_swimming_move_carries_the_pitch`, `a_boarded_move_carries_the_transport_block`,
  `a_flag_without_its_tail_is_cleared`, `each_shared_move_has_its_12340_opcode`,
  `a_move_under_pressure_displaces_motion_and_never_an_action`.
- `benilla-protocol-wrath`, `gameplay::tests::an_ack_without_a_tail_does_not_announce_one`.

## Not done, not proven

- Not compiled. `wrath.rs` lost a `use` (`bevy_to_wow`) and a constant with the removed writer;
  both were checked by search only.
- In game, with the owner driving: fall damage from a height above 14.57 yd, a jump seen by a
  second client, swimming up and down, a standing mouse turn, walk mode, boarding a boat.
- The traceability table still records C08 as a gap; this note does not change that status.
