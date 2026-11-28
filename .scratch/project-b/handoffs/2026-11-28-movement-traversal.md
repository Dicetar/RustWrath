# Game-wide movement, collision and traversal on the 12340 path

Status: landed on main; owner runtime matrix open
Owner: Claude Code (Opus 5.5)
Branch: fix/movement-traversal
Issue: 23, 24, 21 (traceability; this note supersedes the Northshire traversal assignment)
Next: the owner runs the six-row matrix below; the first open engineering item is flight

Base: `47e5e28b0`. Source investigation against the 12340 reference (pin `e67b6ffe`, clean) and
the WM_BridgeLab AzerothCore source. No Cargo, client, server, account or DB use. `rustfmt
--check` passes on the changed files; CI compiles the push.

## Who owns movement state

| State | Owner | Where ownership changes |
|---|---|---|
| Server position and flags | `benilla-protocol-wrath` session/reducer → `WorldDigest` → `wrath_apply_world`; mover orders as `SessionEvent`s → `net/session.rs` → `player/wire_in.rs` | login create (seize), worldport (Wrath resets `Player` whole, `wrath.rs` `world_transferred`, and re-seizes from the create), near teleport (snap, now), digest at-rest correction (`wrath.rs` self branch) |
| Local input and prediction | `player::controller::control` → `mover.rs`/`swim.rs` on `Player` | root, stun, control loss, possession (`foreign_mover`), settle hold |
| Collision | avian colliders from `benilla-world` terrain/WMO/doodad streaming; decks via `TransportQuery` | settle hold until the destination is resident (`world_stale`) |
| Rendered pose | `Player.pos` → `SelfPlayer` transform; remotes `net/motion` | ride carry (`player/ride.rs`), taxi (`server_riding`) |
| Active mover | `Player::foreign_mover` (`wire_in` control verdicts); the tap names it in every move | possession grant/restore, `SMSG_CLIENT_CONTROL_UPDATE`, world entry |
| Outgoing movement | `movement_net::stream_self_movement` → `ClientCommand::Move` → `wrath_tap_commands` → `admit_wrath_movement` → outbox → session thread | worldport and near-teleport acknowledgments, written by the session thread or, now, the tap |

## Most consequential finding

Two shared owners on the 12340 path never ran, and a third ran out of order:

1. **Transports never moved.** The create's `UPDATEFLAG_TRANSPORT` path clock was read and
   discarded (`updates.rs`), and no 12340 GameObject received the shared `TransportAnchor` or
   `ElevatorSeed`, so `transport::arm_transports` never armed one. Every lift, boat, zeppelin
   and tram stayed at its create pose. AzerothCore sends no later pose for a transport
   (`StaticTransport::BuildUpdate` sends fields only; a `MotionTransport` sends none), so the
   client must animate them. Player impact: Undercity and Thunder Bluff are entered by lift
   only; every boat and zeppelin route was dead.
2. **Near teleports disagreed with the server while a key was held.** The session thread
   acknowledged `MSG_MOVE_TELEPORT_ACK` on arrival; the app moved the avatar only through the
   digest's at-rest correction, which skips any pose while a direction, turn or falling bit is
   set. AzerothCore ignores moves until the acknowledgment and then places the player at the
   destination (`MovementHandler.cpp:301`, `:355`); it clears the mover's moving bits before
   sending (`Player.cpp:1449`), so only our own held keys blocked the snap. The next move from
   the old pose walked the server back. Affects every same-map teleport: Blink while running,
   graveyard release on the same map, instance and raid teleporters, `.tele`.
3. **Old-map moves after a worldport.** The session thread acknowledges `SMSG_NEW_WORLD` as
   soon as it is processed, before the app retires its outbox one stage later; AzerothCore
   applies any later move to the new map and kills a player reported under that map's floor
   (`MovementHandler.cpp:488`).

## Fixes: `cf56dd051`

- Near teleport: the order (`session::TeleportOrder`, counter and destination) becomes the shared
  `SessionEvent::Teleport`, so `wire_in::snap_near_teleport` places, settles and deboards the
  mover and asks for `ClientCommand::TeleportAck`, which the tap now writes
  (packed guid, counter, client time) in the outbox behind the moves already queued. An order
  without a pose or without the app's reply queue is still acknowledged at once.
- Transports: `MovementSnapshot::transport_progress` → `WrathEntity::transport_progress`;
  `wrath_transport_seed`/`arm_wrath_transport` give a type-11/15 create the anchor and, for a
  lift, the seed keyed by template entry, based at the stationary position and rotated by
  `PARENTROTATION` (AzerothCore's path rotation, `Transport.cpp:930-939`, already mapped onto
  the 1.12 rotation words). A recreate's new clock re-anchors; a repeated digest does not.
- Worldport: `WrathOutbound::world_generation` names the world a move was composed in; the
  session thread drops a move from a world it has left.

Focused cases (4): `updates::tests::a_transport_create_keeps_its_path_clock`,
`tests/session.rs::a_teleport_order_carries_its_counter_and_destination`,
`wrath::command_tap_tests::a_teleport_ack_keeps_its_place_among_the_moves`,
`wrath::command_tap_tests::a_wrath_transport_create_arms_the_shared_tick`.

## CI

- 46152653040 (`60fb1db42`): failed to compile; a `world_generation` tag landed in a nested
  receipt literal. Fixed by `f5303ff5f`.
- 46153378662 (`f5303ff5f`): `windows` passed (6873 `benilla-app` tests, both new tap cases);
  `gates` failed `player-clippy` on one `type_complexity` (the lift tuple). Fixed by a type
  alias in the commit after it; the run on that commit is the task's result.

## Coverage

| Area | Production path | Disposition | Play it affects |
|---|---|---|---|
| 1. Ground movement, server agreement | shared streamer → tap (`wrath_move_packet`, 1.12↔12340 flag remap `wrath_move_flags`); forced speeds/modes/knockback/control as `SessionEvent`s; near teleport now through the shared snap | implemented; runtime evidence needed. Self-addressed server `MSG_MOVE_*` are dropped on Wrath (partial) | every character, every map |
| 2. Geometry and streaming | shared terrain/WMO/doodad colliders, settle hold on login, teleport and worldport | implemented, shared with 1.12; runtime evidence needed. GameObject world rotation is yaw-only on Wrath (partial) | slopes, stairs, interiors, doorways, bridges |
| 3. Water, flight, mode changes | shared swim and liquid; mount speed by forced speed | water implemented; **flight unsupported** (`SMSG_MOVE_SET_CAN_FLY`/gravity decoded, not applied, not acked: `wrath.rs` `wrath_move_mode`) | swimming everywhere; every flying mount and flight form |
| 4. Moving frames, control | `transport.rs` tick, `player/ride.rs` board by support; possession and client control in `wire_in` | transports **were unsupported, now armed** (runtime pending); state-paused lifts unsupported; transport time 0; vehicles partial | lifts, boats, zeppelins, trams, vehicles |
| 5. Map transitions | session `NEW_WORLD` → `Loading` → ack; app `WorldTransfer` teardown, `Player` reset, re-seize, settle | implemented; near teleport and stale-move defects fixed; cross-map ride re-boards only by standing on the deck (runtime) | hearth, portals, dungeon entry/exit, zeppelin and boat crossings |
| 6. Interaction after movement | range from the `SelfPlayer` transform (`target/cursor_mode.rs`, `ui_session/latch.rs`) | implemented; depends on fixes 1 and 2 for teleports and decks; runtime evidence needed | NPCs, objects, corpses after stops, on decks, inside buildings |

## Remaining defects, in priority order

1. **Flight.** No flight controller; the shared flag word is 1.12's, where `0x02000000` is
   `ON_TRANSPORT`, so it cannot hold 12340's `FLYING` (`0x02000000`) or `CAN_FLY`
   (`0x01000000`). Start: `wrath.rs` `wrath_move_flags` and `wrath_move_mode`,
   `creature_anim/select.rs` `move_flags`, `player/mover.rs`. Visible: a flying mount runs on
   the ground; the server keeps can-fly.
2. **State-paused lifts.** `transport.rs` `Drive::Lift` cycles continuously; AzerothCore's
   `StaticTransport::Update` holds a lift with `pauseAtTime` at its stop until the GO state
   changes. Start: `transport.rs` `Lift`, GO state from `GAMEOBJECT_BYTES_1`. Visible: a
   button-called lift keeps moving client-side.
3. **Cross-map ride.** On a worldport Wrath resets `Player` and re-seizes with `first` false, so
   `wire_in`'s create seat (`self_seat.filter(|_| first)`) never seats the rider; boarding resumes
   only if the deck's collider is there when gravity returns. Start: `wire_in.rs` seize,
   `wrath.rs` `!entered` block.
4. **Self-addressed server moves.** The session's relay branch skips our own guid, so no
   `SelfMoveMessage` is produced on Wrath. Start: `benilla-protocol-wrath/src/session.rs`
   relayed-movement arm, `net/objects.rs` self move.
5. **GameObject world rotation.** `UPDATEFLAG_ROTATION`'s packed quaternion is read and dropped;
   Wrath places a GameObject by yaw. Start: `updates.rs` rotation read, `wrath.rs` spawn
   transform.
6. Transport time and seat are written as 0 in boarded moves (relay only on AzerothCore).

## Runtime matrix (owner-driven; recorder and `WOW_MOVE_TRACE_TAGS=ride` on)

| Context | Do | Distinguishes |
|---|---|---|
| Outdoor terrain (Elwynn road and hill) | run, strafe, jump down a slope, stop | the streamer's edges and fall land against the server: no rubber-band after stopping |
| Same-map teleport while moving | as a mage hold W and Blink; or die and release to a same-map graveyard while holding W | snapped and stays at the destination (fixed) vs snap-back to the old spot (ack ordering) vs frozen (ack missing) |
| City lift (Undercity, Ruins of Lordaeron) | ride the lift down and up | car moves in step with the server (armed) vs frozen (anchor missing) vs drifting (clock or rotation wrong); rider stays aboard |
| Moving transport (Menethil–Theramore boat; Orgrimmar–Undercity zeppelin) | board at the dock, ride, cross the map seam | boat path on time, `ride` trace shows the deck, arrival on the far map aboard or re-boarded vs dropped into the sea (item 3) |
| Dungeon entrance (Deadmines portal) | walk in, walk out while holding W | loads, first move on the new map, no snap to the old map's pose (stale-move fix) |
| Open water and a flying area | swim, dive, surface; mount a flying mount in Northrend | swim edges; flight unsupported, visibly ground-bound |

## Coordination

`wrath_tap_commands` gained one arm (`TeleportAck`) and `WrathOutbound` one field: the
unpublished `wrath-addon-drop-in` clone touches `wrath_tap_commands`, so its next merge needs the
new field in any `WrathOutbound` literal it adds. Nothing here touches reconnect (ClaudeJr),
packaging (DeepSeek) or report attribution (Gemini).

## Not proven

Every runtime row above; compilation (CI); that a 12340 MO transport's local timetable matches
AzerothCore's `TransportMgr` path closely enough to keep passengers on the deck.
