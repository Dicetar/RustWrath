# OpenCode lead checkpoint Ã¢â‚¬â€ O1 repair + first world source path

Date: 2026-10-25. Authoritative checkout: `D:/Projects/project-b-client`, branch `wotlk`.
Adoption base: clean `4c718693`. This checkpoint is uncommitted.

## Implemented

### R8 Ã¢â‚¬â€ reachable Wrath application path

- `WrathNetPlugin` is registered by `GamePlugins`; `WOW_WRATH` explicitly selects it.
- In Wrath mode the stock 1.12 `NetPlugin` is built with `connect = false`, so one session owner
  exists. `WOW_WRATH_AUTH`, `WOW_WRATH_WORLD`, `WOW_WRATH_DWELL` and existing protected
  `WOW_USER`/`WOW_PASS`/`WM_NATIVE_OBSERVATION_TOKEN` supply the session.
- Wrath mode starts at `ClientState::InWorld`; no second glue/session owner is created.

### R9 Ã¢â‚¬â€ terminal and transfer invalidation

- PreUpdate joins a finished worker but no longer takes/drops the feed, so the queued
  `SessionEnded` reaches the Net drain.
- `SessionEnded` (clean or failed) and terminal worker exit clear `WrathWorldView`, self identity,
  player guid and all `WrathStreamed` entities in the Net stage before Input.
- `WorldTransfer` clears the same scope before Input and resets `entered_world`; new-scope entities
  are then created from the next digest.
- `AppExit` signals a cancellation lease and joins the worker for up to 2 s; a still-blocked
  worker is detached rather than freezing application shutdown.

### R10 Ã¢â‚¬â€ producer health lease

- `DEFAULT_PRODUCER_HEALTH_TTL = 15 s`, configurable by `with_producer_health_ttl`.
- `publish_health` records a monotonic renewal instant. The publisher stops sending cached source
  health once the producer stops calling it; the receiver's 15 s health age then fails closed.
- Receiver monotonic age and quiet-health semantics are unchanged. Position/state timestamps are
  never renewed.

### R13 Ã¢â‚¬â€ presentation independent of publication

- Added `SessionState::current_self_snapshot`, built from authoritative session state, not the
  producer `unsent`/ACK/retry slot. `build_digest` now reads it.

### S1 Ã¢â‚¬â€ credential-bearing probe

- `tools/wm_receiver_probe.py` has no literal account, password, GUID or token.
- It requires `WM_NATIVE_SRC`, `WM_NATIVE_OBSERVATION_TOKEN`, `WM_PROBE_GUID`, `WOW_USER`,
  `WOW_PASS`; the driver also accepts credentials/dwell from that environment instead of argv.
- The post-logout read is asserted `None`; the previous non-asserting print is gone.

### S2 Ã¢â‚¬â€ shared lifecycle policy

- `benilla_wm_native::{replace_publisher, retire_publisher}` is the one retire/replacement policy.
  The renderless driver and graphical `run_session` both use it; transfer retires the old publisher
  and orders the replacement after the old worker stops.

### First rendered-world source path

- Wrath digests now translate into `GuidIndex`, `Guid`, `NetEntity`, `ObjectStore`, `UnitSpeeds`,
  `Transform`, `SelfPlayer`, `SelfGuid`, `CurrentMap` and `Equipment`.
- Full GUID is the map key. `WrathGeneration` forces a despawn/recreate when a GUID's generation
  changes. Old `WrathStreamed` entities are retired on transfer/terminal.
- Object fields map display, scale, health/max-health, level, target, race/class/sex and player
  bytes into the existing display/UI path. Player kind uses a settled empty equipment set for the
  first join; item equipment remains a later slice.
- `WorldEntryCascade` and `EnteredWorld` are raised once per scope after the player exists.
- Profile-backed catalogs: `open_profile_chain` opens the manifest archive set as a
  `benilla_formats::Chain`; `open_world_assets` uses it, so Map/AreaTable/Light/Creature/Item and
  other DBC reads no longer auto-discover 1.12 archives.
- 3.3.5 DBC compatibility: trailing-column parsing for Map, AreaTable, Light, GameObjectDisplayInfo,
  ItemDisplayInfo, CreatureModelData, CreatureSoundData, CharSections/races/classes and related
  tables; dedicated 16-column Wrath CreatureDisplayInfo; trailing LightSkybox.

### Basic movement source

- `wrath_stream_movement` samples the existing `Player` controller after Input and sends Wrath
  `MSG_MOVE_START/STOP/JUMP/FALL_LAND/HEARTBEAT` with packed GUID, flags, time, position and
  orientation.
- 1.12 swimming/transport bits are translated to the 3.3.5 positions; transport-local motion is
  deliberately skipped until its block is implemented.
- Authoritative self pose is adopted at rest (server flags and local flags both non-moving) through
  `Player::apply_server_pose`; local motion is not overwritten mid-step.

## Checks actually executed

```text
cargo check -p benilla-app -p benilla-world -p benilla-assets -p benilla-formats \
            -p benilla-protocol-wrath -p benilla-wm-native -p benilla-wm-driver
  PASS

cargo test -p benilla-formats -p benilla-protocol-wrath -p benilla-wm-native --lib
  543 + 15 + 0 PASS

WotLK base manifest, real install:
  chain locale=enUS, 199,375 listed files
  CreatureDisplayInfo=24,262 rows, Extra=15,475
  Map=135, AreaTable=2,307, Light catalog loaded

cargo test -p benilla-app --lib \
  ui_script::wrath_ui_slice_tests::stock_player_frame_host_smoke_test
  1 PASS (host smoke only; not session/action/persistence acceptance)

cargo test -p benilla-app --lib \
  wrath::tests::terminal_scope_loss_clears_ecs_and_input_without_a_digest
  1 PASS
```

No live service, socket, receiver, WM, database, game, visual or route run was performed.

## Correction pass 2

- `wrath_apply_world` now performs scope teardown before any `player_guid`/phase guard: old
  `WrathStreamed` entities, `GuidIndex`, `SelfGuid`, active `Player`, control/stand input state and
  the bounded outbound-action queue are cleared on terminal/transfer scope loss without needing a
  following digest. A focused test covers terminal cleanup with `player_guid == 0`.
- `WrathOutbox` replaces the unbounded outbound channel: app actions are bounded to 64 and the
  queue is cleared by scope teardown.
- Server-response health: the session loop stamps every successful world read on a monotonic
  `Instant`. A 60 s silent window publishes a `Disconnected` source health and ends the session;
  repeated 200 ms socket-idle returns are not treated as health evidence. A quiet server that
  answers the 10 s ping stays healthy.

## Remaining / UNKNOWN

- **RENDER** remains UNKNOWN: no application screenshot/video, no actual player/creature visible.
- WotLK catalog/chain reads are proven; Bevy loader runtime for the full route is not.
- Movement packet acceptance, correction quality, jump arc, water, WMO/doodad route collision and
  transfer/reconnect are unexecuted.
- R11/Phase D remains open: health/max-health field edges now feed the stock UI, but no permitted
  production action was implemented. The existing CVar host already persists a diff and reloads it
  at boot; the Wrath slice test is only a same-VM readback and does not prove a recreated host.
- The player uses an empty settled equipment set on first join; equipment/appearance completion is
  a later UI/character slice.

## Exact source hashes

```text
crates/benilla-app/src/game_plugins.rs                  019b932ced0cb2ce6962624a531d048bfc45de7bf4faf3268a7e548ed1421403
crates/benilla-app/src/lib.rs                           4fb791768ab4e367e9dc9a3ebc89454025a3dd0e1d254781769f78aa147dcca8
crates/benilla-app/src/run_mode.rs                      3053ba81223c508964c84ffcd55064d0aff358494caf3630ee7aa597c6ad865c
crates/benilla-app/src/wrath.rs                         2103c875fc9425de0cd0fadc21c23667386dd9c47ca06a78c30b6f64b460fa10
crates/benilla-app/src/player/state.rs                  c48f924f6664f69bf262ee3e3bae28e72dbd29184cb6866f3c97b89ceb90bf83
crates/benilla-app/src/ui_script/wrath_ui_slice_tests.rs 4dd727b51714310c3bab97623abde11445dcd61378a904afd9d8ea949f7e7375
crates/benilla-assets/src/lib.rs                        c876066267f1adfac71bc15bf93dae9e9f22b7bfbf52d96d27454c5d3077d15d
crates/benilla-assets/src/profile_source.rs             0d71195db1f06378bd9ffa89b3cffdf3c3761734a4aab0fbadbced019c2431f4
crates/benilla-formats/src/area_table.rs                f29fc726e9847172930265dc292d78a6a97fdbdb050e9c7edf43312eac90a43c
crates/benilla-formats/src/chain.rs                     cbbc8b0f15accee6066566c707464d87ddc31420fa64431cdb87ee89fd4e7d21
crates/benilla-formats/src/chr_classes.rs               04e08365b16844af119e3af8ca4cb74ffeca01ed6eda9037a1362dc13703bfca
crates/benilla-formats/src/chr_races.rs                 c598e14142725ac98131cdc95a34230aeb39e73c87b522b3bdbce186b6cf2e0b
crates/benilla-formats/src/creatures.rs                 787d1944b38cce69ff746809957a79050c6cf36ab6230ff2968b4c912d84a11d
crates/benilla-formats/src/dbc.rs                       521ea534eabd579fa9d757739f9711cff456c5ac9b088abb96d9957cc08ac900
crates/benilla-formats/src/factions.rs                  8815cbde4e506f809556704fbaee6dd856184efc0614bd400b8d7d5eb8076aba
crates/benilla-formats/src/gameobjects.rs               c5877949b62e065ed25429450074a821a112ec7a29ab5b1c9be96e621b1fc314
crates/benilla-formats/src/items.rs                     41e8cb3945217b2c6ca1a38417c6cb8aa1f1bd1552e8f8db107fe0d121f89e3e
crates/benilla-formats/src/languages.rs                 8b30886b2e70df54d1edceaa9c9916079eb13357251815d245c1e5fe7b62212f
crates/benilla-formats/src/light.rs                     e97d3e864d789a2b502d6735d3a3841405febb92320b0c8a1eea3ee5aee4c816
crates/benilla-formats/src/maps.rs                      6fc3ae4b35ab592e71ec33ce708302cf5df5b79527840b87e24bc4d61ae7f79d
crates/benilla-protocol-wrath/src/opcode.rs             eb548cad3d4b3f4ba2696aef863f87da1665eb08376499296545baa47d365d4a
crates/benilla-protocol-wrath/src/session.rs            f784bf8063565a989e98e7a0944ef691a3045babe4dc203b2d87635ed010f44b
crates/benilla-wm-driver/src/main.rs                    e5e618a5447f9dda6222bdd5120dc613dea1d64d100ce669c60ada3fdaab110e
crates/benilla-wm-native/src/lib.rs                     1c0653c91c5833014eeeac1ce14b7842883a9c50e6d2776b08b228c1ad2679f3
crates/benilla-wm-native/src/publisher.rs               c0622e30731934fc9102f61b8f852a6c6eaccc8d1f4de9f48a7c1a9e9d7d0116
crates/benilla-world/src/assets/mod.rs                  cdfd7b7eebb9eebf247821e9e5040b907eec91d5843180a86a7db23dff65c267
tools/wm_receiver_probe.py                              a3938193a00e57a8af0300226a12494f6ac84a7d6584587ff5815e0ae3239e92
```

Next concrete dependency: an authorized graphical lab run with the WotLK profile manifest,
`WOW_WRATH=1`, protected credentials and a lab character, to establish RENDER or pinpoint the first
runtime defect. Then repair that defect before O4's stock-UI/action/persistence slice.
