# OpenCode lead checkpoint — first ordinary gameplay path (source)

Date: 2026-10-25
Owner: OpenCode (sole implementation/integration lead)
Branch/checkout: `wotlk` at adoption base `4c718693`, dirty uncommitted tree.
Scope: work package 02, Phases 1-7 source path. No live/graphical execution.

## What changed

### 1. Stock action/UI commands now have a Wrath wire tap

`NetPlugin` still runs with `connect=false` in Wrath mode. `spawn_net` now retains the command
receiver as `NetCommandTap` and clones the inbound event sender as `NetEventSink`.

- `crates/benilla-app/src/net/io.rs`: `NetHandles` carries `events_out` and `command_tap`.
- `crates/benilla-app/src/net.rs`: inserts `NetEventSink` and `NetCommandTap` beside the
  existing `NetEvents`/`NetCommands`.
- `crates/benilla-app/src/wrath.rs`: `wrath_tap_commands` drains the stock `ClientCommand`
  stream and queues translated `WrathOutbound` packets in the existing `WrathInput` outbox.
  `WrathControl` carries the event sender; server replies are translated back into the
  established `SessionEvent` handlers.

Supported commands: selection, attack start/stop, cast, gossip hello/select, NPC text query,
questgiver status/hello/query/accept/complete/request-reward/choose-reward, quest query,
loot/autostore/loot-money/release, game-object use, item/creature/game-object/name query,
set-active-mover, set-action-button, logout/cancel. `Move` is deliberately not duplicated by
the tap: `wrath_stream_movement` remains the single movement writer for this slice.

### 2. Typed 3.3.5 gameplay codec

New `crates/benilla-protocol-wrath/src/gameplay.rs`:

- Client builders with layouts checked against the local AzerothCore sources.
- Typed replies for combat, cast result/start/go/failure, initial spells/action buttons,
  gossip/NPC text, questgiver panels/templates/progress/rewards, loot, item push, name,
  creature, game-object and item queries, logout.
- Opcodes added from the local 12340 `Opcodes.h`.

The 3.3.5-only layouts are handled explicitly: accept-quest divider word, action-button
state byte, packed name-query GUID, packed actor GUIDs in attack/cast replies, item
randomSuffix before randomProperty, questgiver flags/repeatable bytes, and quest-query
reward/choice widths.

### 3. World view/field translation

`wrath_apply_world` now maps the fields the existing stock frames read:

- entry, display, health/max-health, level, faction, unit flags, mount, bytes, NPC/dynamic
  flags, target GUID;
- player power/max-power, inventory and backpack GUID words, XP, coinage;
- 3.3.5 player quest-log slots into the 1.12 three-dword slot layout (id, packed 4x6-bit
  counters + state byte, timer).

Only the player carries the 1.17k-field raw window; other entities keep the compact digest.

### 4. Logout edge

`Logout`/`LogoutCancel` translate to the 3.3.5 messages. `SMSG_LOGOUT_RESPONSE`,
`SMSG_LOGOUT_COMPLETE` and `SMSG_LOGOUT_CANCEL_ACK` translate back. On `LOGOUT_COMPLETE` the
worker retires its publisher and returns cleanly; the app receives `LoggedOut` and then the
terminal `Disconnected { end: LoggedOut }`.

## Evidence

- `cargo check -p benilla-app -p benilla-protocol-wrath` — pass (pre-existing warnings only).
- `cargo test -p benilla-protocol-wrath gameplay::tests` — 7 passed.
- `cargo test -p benilla-app command_tap_tests` — 1 passed, 6592 filtered.
- `git status` confirms all prior dirty work plus this path is uncommitted; no commit made.
- No game, renderer, receiver, WM, socket, database or live server was run.

## Current hashes (SHA-256)

```
FCDA27490C30EFDFAC67FB658E6646C3EE914B8C8C3C3FD3060C73B989E4BDF6  crates/benilla-app/src/wrath.rs
4ADB1DBE14BAB84FC2D76AF2418DDF0EA0887587D0FD9345794A53333F8A8E92  crates/benilla-app/src/net.rs
48CEF4EC1B5E680D17FDFB8D05F39C6EB35E3392D808C5A266FBB151BA85C1CC  crates/benilla-app/src/net/io.rs
01341D3CAE95354D041DF2FB7913399323546EF6FB77AD4668D70D9B5AA20AD9  crates/benilla-protocol-wrath/src/gameplay.rs
4AD3A5E0B7D57411EB89205BE57285491B4E792075B41FD258B4AB421BD234F4  crates/benilla-protocol-wrath/src/opcode.rs
2198D1E1B6EBF3A0F58FCA31E17ED475E4724168B91B193476218319A98EDE1F  crates/benilla-protocol-wrath/src/lib.rs
```

Hashes change when the files change; do not treat them as a commit identity.

## Remaining gaps

- Runtime/graphical acceptance is unchanged and unauthorized. All of the above is source +
  checks only.
- Target/attack/cast/quest/loot packet acceptance, object lifecycle and exact results are not
  proven against a live 3.3.5 server.
- Automatic reconnect after logout is not implemented; the Wrath worker ends and the app sees
  a clean logout edge.
- Aura/cooldown/action-button server deltas, item object creation and bag contents, loot rolls,
  object query edge cases, transfer/death/release and WMO/doodad route behavior remain open.
- `WrathWorldView` has no explicit epoch tag on translated `SessionEvent`s; stale replies are
  bounded by the session lifecycle but not yet rejected per event.
- No commit, stash, reset or merge was performed.
