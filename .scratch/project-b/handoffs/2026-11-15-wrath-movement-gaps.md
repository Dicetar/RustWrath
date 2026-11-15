# Wrath movement gaps: the server's mover orders reach the shared controller

Status: ready to merge
Owner: Claude Code (Opus 5.5)
Branch: fix/wrath-movement-gaps
Issue: none
Next: land `fix/wrath-movement-wire` first (this branch is cut from it at `5a647637`), then source review and the owner's batch build; run the 5 focused cases and check root, knockback and possession in game

Matrix row F04 (input and movement). Source only: nothing here was compiled, no test was run and
nothing was seen in game.

## What was wrong

The Wrath decoder read one mover order, `SMSG_FORCE_RUN_SPEED_CHANGE`. Everything else the server
says to or about a mover fell through to `None`:

- the other forced speeds (walk, run back, swim, swim back, turn rate);
- root/unroot, water/land walk, feather/normal fall, hover;
- `SMSG_MOVE_KNOCK_BACK` and `SMSG_CLIENT_CONTROL_UPDATE`;
- every observer broadcast, `SMSG_SPLINE_SET_*_SPEED` and `SMSG_SPLINE_MOVE_*`;
- the login compound `SMSG_MULTIPLE_MOVES` (`Player.cpp:11602`: root, feather fall, water walk,
  hover), so a character logging in rooted or with feather fall never had it applied.

The ack side and the shared consumers (`death/net.rs` `on_move_mode`, `net/objects.rs`
`on_speed`/`spline_move_mode`, `net/session.rs` knockback and client control, `player/wire_in.rs`)
were already there; the Wrath path never fed them. That also covers the earlier "no
`CMSG_SET_ACTIVE_MOVER` for possession" item: the grant consumer sends it, it just never received a
grant.

## What changed

- `world.rs`: `WorldConnection::recv` splits `SMSG_MULTIPLE_MOVES`/`SMSG_COMPRESSED_MOVES` into
  their sub-packets and hands them out in order before the next wire read, as 12340's
  `UnpackAndProcessOpcodes` does (`0x00716AF0`, `0x00716AD0`). Each part goes through the normal
  reduce/parse path. The compressed size is bounded at 1 MiB and must inflate exactly.
- `gameplay/mover_control.rs` (new): decodes all nine forced speeds (run carries AzerothCore's
  extra `u8`), the nine spline speeds, the twelve mover modes (including can-fly and gravity), the
  sixteen spline modes (including flying and gravity), knockback and client control. Every reader
  refuses trailing bytes.
- `gameplay.rs`: six events replace `ForceRunSpeedChange`; unmatched opcodes fall to
  `mover_control::parse`.
- `wrath.rs` `translate_gameplay`: the shared kinds and modes map onto `SessionEvent`s; knockback
  becomes the shared `JumpInfo` (`-speedZ` is already down-positive).
- `gameplay::write_movement_info`: a boarded move sends seat `0xFF`, not `0`. 12340 resets
  `m_transportSeat` to `0xFF` (`CMovementStatus.cpp:50`); only vehicle boarding sets one;
  AzerothCore's default is `-1` (`Object.h:303`).

## Named unsupported (decoded, logged, not applied, not acked)

- Flight, flight-back and pitch-rate speeds (`warn` for ours, `debug` for others).
- `SMSG_MOVE_SET_CAN_FLY`/`UNSET`, `SMSG_MOVE_GRAVITY_DISABLE`/`ENABLE` (`warn`), and the spline
  flying/gravity modes (`debug`). The shared mover has no flight controller. Acking an order we
  do not apply would tell the server something false; AzerothCore does not kick for a missing
  can-fly ack. Missing consumer: a flight mode in the shared controller.

## Not done

- Transport time is still `0` on boarded moves. 12340 sends the transport's own path clock
  (`OnSetActiveMover` `0x006EE870`, `GetTransportUpdateTime`), which this client does not track.
  AzerothCore only relays it to observers. Missing consumer: a per-transport path clock, which
  also needs `Player::ride` on Wrath.
- A walk-off fall stays silent on its first frame and heartbeats at the 500 ms deadline; 12340
  heartbeats at once (`Movement.cpp` `PMOVE_FALL`). Kept deliberately: the silence stops a chair's
  stalled collider frames standing a seated player up (`a047a31a`), and AzerothCore's moving mask
  includes FALLING. Fall damage is unaffected: AzerothCore re-anchors the fall start on every
  grounded packet (`PlayerUpdates.cpp:2182`). Observers see up to 500 ms of hang at the ledge.
- The periodic heartbeat runs while any sent flag is set (walk mode, swimming at rest), wider than
  12340's `flags & 0xC0100F`. Shared with the 1.12 path and harmless traffic; left alone.
- Recording stores a compound packet as it arrived, not split.

## Focused cases (not run)

- `world::tests::compound_moves_split_into_their_orders_plain_or_inflated`
- `gameplay::tests::forced_speeds_carry_the_run_only_byte_and_all_nine_kinds`
- `gameplay::tests::mover_orders_and_unit_modes_name_their_direction`
- `gameplay::tests::knockback_and_client_control_keep_azerothcore_field_order`
- `wrath::tests::a_boarded_move_carries_the_transport_block` (now pins seat `0xFF`)
