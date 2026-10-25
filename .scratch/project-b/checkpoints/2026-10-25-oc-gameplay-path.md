# OpenCode lead checkpoint - first ordinary gameplay path (source)

Date: 2026-10-25
Owner: OpenCode (sole implementation/integration lead)
Checkout: `wotlk`, adopted at `4c718693`; current HEAD includes the gameplay commits below and
Astra/procgen commits made by other workers. Worktree clean at this checkpoint.
Scope: work package 02, Phases 1-7 source path. No live/graphical execution.

## Committed source

- `720c83b6` Add Wrath gameplay command and reply path. Contains the action tap, 3.3.5 gameplay
  codec, replicated-field translation, logout edge, and the first adapter-semantic repairs.
- `2f61b877` Preserve Wrath creature racial-leader metadata.
- `9a1e365e` is a later procgen worker commit. It includes previously dirty catalog/bridge/WM
  files and all checkpoint files. Do not treat that commit as OpenCode gameplay evidence; do not
  rewrite it.

## 1. Stock action/UI commands now have a Wrath wire tap

`NetPlugin` still runs with `connect=false` in Wrath mode. `spawn_net` retains the command
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

## 2. Typed 3.3.5 gameplay codec

`crates/benilla-protocol-wrath/src/gameplay.rs`:

- Client builders with layouts checked against the local AzerothCore 12340 sources.
- Typed replies for combat, cast result/start/go/failure, initial spells/action buttons,
  gossip/NPC text, questgiver panels/templates/progress/rewards, loot, item push, name,
  creature, game-object and item queries, logout.
- Opcodes added from the local `Opcodes.h`.
- 3.3.5-only details handled explicitly: accept-quest divider word, action-button state byte,
  packed name-query GUID, packed actor GUIDs, item randomSuffix before randomProperty,
  questgiver flags/repeatable bytes, and quest-query reward/choice widths.

`crates/benilla-protocol-wrath/src/cast_failure.rs` maps all 188 3.3.5 `SpellCastResult` values
by `SPELL_FAILED_*` name onto the 1.12 reason table. Unsupported values are explicit
`Unsupported` results; the app logs them and uses 1.12 `SPELL_FAILED_ERROR` instead of passing a
foreign numeric byte through.

## 3. World view/field translation

`wrath_apply_world` maps fields the existing stock frames read:

- entry, display, health/max-health, level, faction, unit flags, mount, bytes, NPC/dynamic
  flags, target GUID;
- player power/max-power, inventory and backpack GUID words, XP, coinage;
- 3.3.5 player quest-log slots into the 1.12 three-dword slot layout (id, packed 4x6-bit
  counters + state byte, timer).

Only the player carries the 1.17k-field raw window; other entities keep the compact digest.

## 4. Reply-adapter semantic repairs

- **Gossip delivery:** the special menu-id feed event no longer swallows the reply. The
  translated `SessionEvent::GossipMenu` is delivered to the existing gossip UI while the
  `WrathWorldView` keeps `(npc, menu_id)` for `CMSG_GOSSIP_SELECT_OPTION`.
- **Cast failures:** 3.3.5 reason bytes are mapped by meaning, not by number. Required 1.12
  arguments are read from the 3.3.5 packet for area, focus, equipped class, exotic ammo,
  reagents, totems and mechanic. Unsupported reasons are logged and replaced with the generic
  1.12 error; they are never passed through raw.
- **Interruptions:** `SMSG_SPELL_FAILURE` now becomes `SessionEvent::SpellFailedOther` with the
  original caster GUID. It never manufactures a player `SMSG_CAST_RESULT`; remote cleanup stays
  caster-keyed. Our own cast failure still comes through `SMSG_CAST_FAILED`.
- **Questgiver status:** 3.3.5 status values shifted relative to 1.12. Status events and both
  gossip/greeting quest-row icons are mapped (e.g. 3.3.5 8 AVAILABLE -> 1.12 5, 3.3.5 5
  INCOMPLETE -> 1.12 3, 3.3.5 10 REWARD -> 1.12 6).
- **Spell-go misses:** the reflect byte after a miss entry is consumed and carried as
  `reflect_result`; it was previously misread as the next target GUID.
- **Item push:** the signed random-property id is preserved by bit cast instead of clamped to
  zero (negative values are random suffixes).
- **GameObject templates:** 3.3.5 data columns are wider and indexed differently from 1.12.
  The adapter keeps name/display but reports the 24 columns as absent, rather than letting the
  1.12 lock/page/meeting-stone readers consume a foreign layout.
- **Creature metadata:** the response tail is read through the racial-leader byte and carried
  into the creature-name event. The 1.12 civilian byte has no 3.3.5 wire source and stays false.

Audit results for values verified as compatible and left unchanged: `LootType`, `LootSlotType`,
`LootError`, `SpellMissInfo` (0..11), `ActionButtonType` kind bytes, `SpellCastFlags` bits used
by the 1.12 consumers, 1.12 target-mask bits, and the low quest flags. Fields not claimed as
translated remain explicit gaps below.

## Evidence

Source-only evidence:

- Commit `720c83b6` contains the gameplay path and repairs 1-4 (gossip, cast failure, caster
  interruption, quest status, reflect, item sign, GameObject columns).
- Commit `2f61b877` contains the racial-leader tail read.
- `git diff --check` clean; worktree clean after those commits.

Reported checks, not current acceptance:

- Earlier in the session `cargo check -p benilla-app -p benilla-protocol-wrath` passed and
  `cargo test -p benilla-protocol-wrath gameplay::tests` reported 7 passed; the command-tap test
  reported 1 passed. These predate the adapter-semantic repairs and do not prove the current
  commit compiles or passes.
- No build, test, renderer, WM, socket, database or live server was run for `2f61b877` or the
  final repair state. Under the active owner restriction they are UNKNOWN until explicitly
  authorized.

## Current hashes (SHA-256)

```
09921D170383B4E9154E4E4D0E255F6871299FC0D218215ACDC04437779A0FCC  crates/benilla-app/src/wrath.rs
4ADB1DBE14BAB84FC2D76AF2418DDF0EA0887587D0FD9345794A53333F8A8E92  crates/benilla-app/src/net.rs
48CEF4EC1B5E680D17FDFB8D05F39C6EB35E3392D808C5A266FBB151BA85C1CC  crates/benilla-app/src/net/io.rs
8B7340E95EB3FA9A44A0826521915A72451D2C4383734CD5860E641F0475E565  crates/benilla-protocol-wrath/src/gameplay.rs
0C287754E2A913A6489DCD3B84C8380F0B787CF14389685A203518B3ED9CD447  crates/benilla-protocol-wrath/src/cast_failure.rs
4AD3A5E0B7D57411EB89205BE57285491B4E792075B41FD258B4AB421BD234F4  crates/benilla-protocol-wrath/src/opcode.rs
520E73E7EA495CEDB2C12C74A8E6F496BE7728E897AA1A77D37E3DA40B6FAC6E  crates/benilla-protocol-wrath/src/lib.rs
```

Hashes change when files change; use the commit IDs for provenance.

## Remaining gaps

- Runtime/graphical acceptance is unchanged and unauthorized. All of the above is source only.
- `ItemInfo` currently carries the early static fields only; stats, damage, spells, bonding,
  description, durability and the later 3.3.5 columns are defaults. Tooltips are incomplete.
- `SpellStart`/`SpellGo` do not decode the full target block beyond the mask/hit list; source and
  destination points, game-object targets and missile visuals remain unproven.
- Cooldowns, aura deltas, action-button server updates, item object creation, bag contents, loot
  rolls, transfer/death/release and WMO/doodad route behavior remain open.
- Automatic reconnect after logout remains a separate task; the worker ends cleanly, the app
  sees `LoggedOut` then `Disconnected { end: LoggedOut }`.
- `WrathWorldView` has no explicit epoch tag on translated `SessionEvent`s; stale replies are
  bounded by the session lifecycle but not rejected per event.
- No commit, stash, reset or merge of pre-existing bridge/assets/WM work was performed by this
  checkpoint; those were committed by another worker in `9a1e365e`.
