# A1 — Wrath death, release, resurrection

Branch `fix/wrath-death-cycle`, worktree `D:\Projects\rw-worktrees\a1-death-cycle`, based on
`origin/main` 804264e0. Uncommitted by instruction: the overseer compiles, runs the tests and
commits; this note is the review surface.

Reference: AzerothCore 3.3.5 source at
`D:/WOW/Azerothcore_WoTLK_Rebuild/source/azerothcore` and the 12340 decomp at
`D:/Projects/wowdecomp-reference` (pin e67b6ffe), both read-only. Stock 3.3.5 FrameXML read from
`F:/AI/tmp/opencode/335ui`. Nothing was copied; the AC/decomp references are named where the
behavior they describe is implemented.

Verification level: **source only**. No Cargo, no client, no `cargo fmt` (per brief), no server
run. Every changed file parses (`rustfmt --check`, no `error` lines). Tests are written but not
run; compile and test status are **not run**.

## 1. What already existed (audited, unchanged)

The 1.12 donor death cycle was already ported and working through the 3.3.5 seam; A1 audited it
rather than rebuilding it.

- Death state: `crates/benilla-app/src/death/mod.rs` reads `UNIT_FIELD_HEALTH` 0 and
  `PLAYER_FLAGS` bit `0x10` off the projected self descriptor (`feed_death`, `DeathView`).
  12340's `PLAYER_FLAGS` (150) is projected to the donor slot 190 by
  `crates/benilla-app/src/wrath.rs::push_player_unit_fields`, so the ghost bit is live on Wrath.
- Release: `RepopMe()` → `DeathAction::Repop` → `CMSG_REPOP_REQUEST`.
  `benilla-protocol-wrath/src/gameplay.rs::repop_request` writes the one byte
  `HandleRepopRequestOpcode` skips (AC `MiscHandler.cpp:58-62`).
- Ghost form: `PLAYER_FLAGS` ghost drives `FfxDeathFade` (`drive_death_look`), water-walk is
  mirrored, and `PLAYER_UNGHOST` fires on the ghost-bit clear (`field_watch`,
  `crates/benilla-app/src/field_watch.rs`).
- Corpse query, reclaim delay and request: `MSG_CORPSE_QUERY`, `SMSG_CORPSE_RECLAIM_DELAY` and
  `SMSG_RESURRECT_REQUEST` were already decoded on Wrath
  (`benilla-protocol-wrath/src/gameplay.rs`, `crates/benilla-app/src/wrath.rs`
  `translate_gameplay`), and `death/net.rs` already fed `DeathNet`, `GetCorpseMapPosition`
  (`benilla-ui/src/script/worldmap.rs`), the minimap corpse blip
  (`crates/benilla-app/src/minimap/blips/mod.rs::corpse_landmark`) and `CORPSE_IN_RANGE` /
  `CORPSE_IN_INSTANCE` / `CORPSE_OUT_OF_RANGE`.
- Resurrect offer and answer: `SMSG_RESURRECT_REQUEST` → `DeathNet.resurrect` →
  `RESURRECT_REQUEST` → the stock three popup variants; `AcceptResurrect`/`DeclineResurrect`
  → `CMSG_RESURRECT_RESPONSE`. AC reads that as a raw 8-byte guid + byte
  (`ObjectGuid.cpp:75-79`), which `resurrect_response` already matches.
- Spirit healer activate: `AcceptXPLoss` → `CMSG_SPIRIT_HEALER_ACTIVATE`; the right-click
  spirit-healer arm fires `CONFIRM_XP_LOSS` locally with no packet
  (`crates/benilla-app/src/target/click.rs:1128-1131`), which is exactly 12340's
  `CGPlayer_C::TalkToSpiritHealer(guid)` (`Player_C.cpp:6212-6248`).
- Durability death line: `SMSG_DURABILITY_DAMAGE_DEATH` (0x2BD) is in
  `wrath.rs::is_shared_12340_server_opcode` and decodes through the 1.12 parser into
  `SessionEvent::DurabilityDamageDeath`; `death/net.rs` already turns it into the red
  `COMBAT_MISC_INFO` line.
- Resurrection sickness text: `GetResSicknessDuration` uses the level table in
  `death/mod.rs::sickness_duration`. AC's `Death.SicknessLevel` default is 11
  (`WorldConfig.cpp:399`) and its per-level duration (`Player.cpp:4433-4453`) is the same curve
  as `SpellDuration.dbc` row 427 (`-600000 + 60000/level`, capped 600000), which 12340's
  `GetResSicknessDuration` (`ScriptEvents_Scripts.cpp:3895-3930`) reads. Equivalent; the
  existing DEVIATIONS row stands.

## 2. The gaps A1 found on Wrath, and what changed

### 2a. The stock 3.3.5 death popups called five globals that did not exist

3.3.5 `StaticPopup.lua` calls these from the DEATH / RESURRECT / XP_LOSS dialogs, and none was
registered, so each was a Lua error on the death path:

| call | 3.3.5 site | 12340 source |
|---|---|---|
| `CannotBeResurrected()` | `StaticPopup.lua:1148`, `:1165` | `Spell_C_Scripts.cpp:56-68` (`0x802270`): aura effect mask bit 314 |
| `IsFalling()` | `StaticPopup.lua:1171` | `ScriptEvents_Scripts.cpp:1454-1469` (`0x612430`): `FALLING` && !`ROOT` |
| `IsOutOfBounds()` | `StaticPopup.lua:1171` | `ScriptEvents_Scripts.cpp:1440-1452` (`0x6123c0`): `PLAYER_FLAGS & 0x4000` |
| `UnitIsControlling("player")` | `StaticPopup.lua:1205`, `:1231`, `:1255` | `ScriptEvents_Scripts.cpp:1897-1915` (`0x613c90`): `UNIT_FIELD_CHARM`/`_SUMMON` |
| `GetDeathReleasePosition()` | `WorldMapFrame.lua:908` | `WorldMap_Scripts.cpp:902-914` (`0x5459c0`) |

Changes:

- `crates/benilla-ui/src/script/death.rs:195` — new `install_wrath`, registering
  `CannotBeResurrected`, `IsFalling`, `IsOutOfBounds` and `UnitIsControlling` as 1/nil
  flags off the pushed snapshot (the 3.3.5 answer shape). It is installed **only** for
  `UiTarget::Wrath335`, from `crates/benilla-ui/src/script/mod.rs:877`, so the 1.12 global
  surface and `tests/vm_parity_surface.golden` are untouched.
- `crates/benilla-ui/src/script/death.rs:34-50` — `DeathUiState` gains
  `cannot_be_resurrected`, `falling`, `out_of_bounds`, `controlling`.
- `crates/benilla-app/src/death/mod.rs:249-263` — the snapshot fills them:
  `cannot_be_resurrected` from the player's live auras against `Spell.dbc` effect
  `EffectApplyAuraName` 314 (`cannot_be_resurrected`, `death/mod.rs:477`), `falling` from
  the local mover's streamed flags, `out_of_bounds` from `PLAYER_FLAGS & 0x4000`,
  `controlling` from `unit_charm()`/`unit_summon()`.
- `crates/benilla-app/src/wrath.rs:997-1002` — `WrathEntity` gains `charm`/`summon`/`
  corpse_fields`; `wrath.rs:12926-12935` fills them. `wrath.rs:9843-9858` projects
  `UNIT_FIELD_CHARM`/`_SUMMON` (6/7, 8/9 — the same indices in 1.12) into the donor
  descriptor, which previously dropped them, so `UnitIsControlling` (and the attack-charm
  refusal) can answer at all.

### 2b. `SMSG_DEATH_RELEASE_LOC` (0x378) was not decoded: no release position at all

AC sends it at `Player::RepopAtGraveyard` (`Player.cpp:4860-4872`, map + x/y/z) and clears it
with map `-1` at `ResurrectPlayer` (`Player.cpp:4380-4385`). 12340's handler stores it and
hands it to `MinimapSetSpiritHealerPOI` (`Player_C.cpp:9720-9736`, `GameUI.cpp:3451-3459`).

- `benilla-protocol-wrath/src/opcode.rs:142`, `gameplay.rs:2213/2756/4293` — the constant,
  `ServerEvent::DeathReleaseLoc { map_id, position }`, the parse arm and decoder (signed map, so
  `-1` survives).
- `benilla-protocol/src/events.rs:1059-1063` — `SessionEvent::DeathReleaseLoc`.
- `crates/benilla-app/src/wrath.rs:3790-3792` — translation.
- `crates/benilla-app/src/death/net.rs:18/71-85/215` — handler and `death_release_loc`:
  it follows 12340's handler exactly — the location is kept only while the active player is **not**
  a ghost; the ghost leg (and a missing player) is the `SetDeathReleaseLocation(-1, zero)` clear
  (`Player_C.cpp:9720-9736`, `GameUI.cpp:3451-3459`). Because AC sends the packet right after the
  ghost aura (`Player.cpp:4860-4872`) and sends `-1` at `ResurrectPlayer` while the flag is still
  set (`Player.cpp:4380-4392`), the marker is normally cleared on both edges — exactly what the
  reference does against this server; the stored leg exists for the not-a-ghost case and is
  unit-tested. `DeathNet.release_location` is `death/mod.rs:32-40/87`.
- `crates/benilla-app/src/ui_world_map.rs:544/610-618/716` — projects the release location onto
  the displayed map; `benilla-ui/src/script/worldmap.rs:575` (`set_death_release_uv`) and
  `worldmap.rs:791-800` — `GetDeathReleasePosition()` now answers the projection (the existing
  Wrath-only stub that always answered (0,0) was replaced) instead of hiding the marker forever.

### 2c. `SMSG_PRE_RESURRECT` (0x494) was not decoded: missiles kept flying at the ghost

AC sends the body's packed guid at `BuildPlayerRepop` (`Player.cpp:4327-4331`); 12340 calls
`CMissile::ClearInFlightMissilesWithTarget(guid)` (`Player_C.cpp:22246-22252`).

- `opcode.rs:144`, `gameplay.rs:2218/2757/4305`, `events.rs:1064-1066`,
  `wrath.rs:3793-3795` — decoded and translated.
- `crates/benilla-app/src/death/net.rs:87-95` — handler writes `ClearMissilesAt`.
- `crates/benilla-app/src/entities/missile.rs:782-810` — the message and
  `clear_missiles_at`; `crates/benilla-app/src/entities.rs:669/710-711` registers the message
  and runs the clear before the frame's flight, despawning every `Aim::Unit` missile homing on
  the released guid and stopping its flight sound.

### 2d. `SMSG_AREA_SPIRIT_HEALER_TIME` (0x2E4) was not decoded: no area-healer wave on Wrath

AC answers `CMSG_AREA_SPIRIT_HEALER_QUERY` with `guid + u32 ms`
(`BattlegroundMgr.cpp:667-674`, `Battlefield.cpp:617-625`). The app already had a handler for
`SessionEvent::AreaSpiritHealerTime` (`ui_dialog_verbs.rs:967/1018`), but the Wrath session
never produced the event, so `GetAreaSpiritHealerTime` stayed 0 and
`AREA_SPIRIT_HEALER_IN_RANGE` never fired in a battleground.

- `opcode.rs:149`, `gameplay.rs:2221/2758/4315`, `wrath.rs:3796-3798` — decoded and
  translated. No app change was needed: the existing handler consumes it.

### 2e. `SMSG_AREA_TRIGGER_NO_CORPSE` (0x506) was not decoded

AC's `MapMgr.cpp:207` refuses a ghost whose corpse is in another instance; 12340 shows
`ERR_CORPSE_IS_NOT_IN_INSTANCE` (`GameUI.cpp:6821-6824`; the 12340 opcode name is
`SMSG_AREA_TRIGGER_NO_CORPSE`, `Types.h:1294`).

- `opcode.rs:147`, `gameplay.rs:2223/2759`, `events.rs:1068-1069`, `wrath.rs:3799` —
  decoded and translated.
- `crates/benilla-app/src/death/net.rs:98-106` — queues `UiError::key(
  "ERR_CORPSE_IS_NOT_IN_INSTANCE")` through the existing `DisplayError` sink.

### 2f. The 12340 corpse object block was never projected: our corpse rendered as nothing

12340 keeps a corpse's display, item slots and appearance in its own block
(`CORPSE_FIELD_DISPLAY_ID` 10, `UpdateFields.h:420-430`), while the 1.12 owners read display 12
and items from 13. `wrath_apply_world` copied no corpse field, so a fresh corpse had no
`display_id` (`NetEntity.display_id` came from `UNIT_FIELD_DISPLAYID`, which a 3.3.5 corpse
does not carry) and the corpse-guid latch `note_corpse` (owner == me) never armed.

- `wrath.rs:9231` — `push_corpse_fields`: 6..=9 identity, 10..=35 shifted by two, delivered
  dwords only.
- `wrath.rs:9860-9864` — the corpse branch in the projection;
  `wrath.rs:9974-9980` — `NetEntity.display_id` for a corpse comes from
  `corpse_display_id()`; `wrath.rs:9981-9990` — feeds `note_corpse` so
  `CMSG_RECLAIM_CORPSE` carries the real guid (AC ignores the guid and resolves the corpse
  through the player, `MiscHandler.cpp:642-651`, but the latch also drives the bones drop);
  `wrath.rs:9660-9666` — `forget_corpse` on destroy (a bone swap destroys the object).

## 3. Server messages decoded but not applied, and messages that are not sent

- `SMSG_DEATH_RELEASE_LOC`: **applied** with the reference's own gate (kept only while not
  a ghost) to the map marker and `GetDeathReleasePosition`. **Not drawn**: 12340 also feeds
  `MinimapSetSpiritHealerPOI` (spirit healer icon on the minimap); this client's minimap has only
  the corpse rim arrow (`minimap/blips/mod.rs::corpse_landmark`), so the spirit-healer minimap POI
  is named as unbuilt. The reference's local clear on unghost (`UpdateGhostStatus`,
  `Player_C.cpp:13139-13148`) is covered by the ghost gate above and AC's wire `-1`; no second
  local clear was added.
- `SMSG_PRE_RESURRECT`: applied only as the missile clear (its whole 12340 effect, `Player_C.cpp:22246-22252`).
- `SMSG_PLAYER_SKINNED` (0x2BC), `SMSG_RESURRECT_FAILED` (0x252),
  `SMSG_FORCED_DEATH_UPDATE` (0x37A): never sent by this AC build (only opcode-table rows,
  `Opcodes.cpp:725/831/1021`), so they are named here and deliberately left undecoded.
- `SMSG_DURABILITY_DAMAGE_DEATH`, `SMSG_SPIRIT_HEALER_CONFIRM`: decoded via the shared 1.12
  parser / the Wrath decoder and applied; no change needed.
- Client sends audited against AC's raw reads: `CMSG_REPOP_REQUEST` (1 byte),
  `CMSG_RECLAIM_CORPSE` (raw 8-byte guid), `CMSG_RESURRECT_RESPONSE` (raw guid + byte),
  `CMSG_SPIRIT_HEALER_ACTIVATE` (raw guid), `CMSG_SELF_RES` (empty),
  `CMSG_AREA_SPIRIT_HEALER_QUERY/QUEUE` (raw guid, `MiscHandler.cpp:1636-1680`). All match.

## 4. Tests added (source only, not run) — 12 focused cases

- `benilla-protocol-wrath/src/gameplay.rs:5184` `wrath_death_release_loc_reads_the_graveyard_and_the_clear`
- `gameplay.rs:5220` `wrath_pre_resurrect_reads_the_packed_ghost_guid`
- `gameplay.rs:5230` `wrath_area_spirit_healer_time_reads_guid_and_wave`
- `gameplay.rs:5249` `wrath_corpse_not_in_instance_is_an_empty_body`
- `gameplay.rs:5260` `wrath_resurrect_request_reads_sickness_and_the_timer_override`
- `crates/benilla-app/src/wrath.rs:13273` `a_corpse_block_shifts_by_two_after_party`
- `crates/benilla-app/src/death/net.rs:301` `the_reclaim_delay_anchors_and_re_announces`
- `death/net.rs:314` `a_resurrect_offer_is_held_and_announced_per_message`
- `death/net.rs:332` `a_release_location_is_kept_only_while_not_a_ghost`
- `crates/benilla-app/src/death/mod.rs:903` `cannot_be_resurrected_reads_the_effect_not_the_spell_id`
- `crates/benilla-app/src/ui_script/death_tests.rs:469` `the_death_popup_natives_read_the_pushed_state`
- `death_tests.rs:528` `death_release_position_binding`

## 5. Unproven / risks (named, not hidden)

1. **Not compiled and not run.** Every file parses; type/borrow errors are possible. The riskiest
   additions: `ClearMissilesAt`'s `MessageReader` system and its `.chain()` placement
   (`entities.rs:710`), and the `Option<Res<Player>>` added to `DeathView` (a `SystemParam`
   shared by `feed_death`/`signal_death`).
2. **Base commit is broken outside this diff.** 804264e0 carries the literal-newline coin strings
   in `crates/benilla-ui/src/script/loot.rs` (introduced by 1eef78f6), fixed on origin/main by
   9c6f839a (`fix(build): restore the escaped newlines...`). My branch cannot build `benilla-ui`
   until origin/main is merged; `origin/main` is 9 commits ahead and also carries
   e8551634 (tooltip `ObjectLike` import).
3. **Merge overlap.** origin/main also edits `crates/benilla-app/src/wrath.rs` in the same files
   (WrathEntity 1001, translate_gameplay 3789, wrath_apply_world 10040/10305/10339, build_digest
   12872, the `command_tap_tests` literal) and `benilla-ui/src/script/mod.rs:1561`. My hunks are
   additive and in neighbouring regions; resolve by keeping both sides. Nothing else I touched
   changed upstream.
4. **Spirit-healer minimap POI** from `SMSG_DEATH_RELEASE_LOC` is not drawn (section 3).
5. **`CheckSpiritHealerDist` range**: the app still compares against the 1.12 constant 30.864
   yd²; 12340 uses `combat reach + 4` (`Player_C.cpp:1495-1512`). Wrath does not project
   `UNIT_FIELD_COMBATREACH`, so `unit_combat_reach()` would read the 1.5 default → 5.5 yd vs
   5.556 yd. Left as is; not a visible difference at that gap, and fixing it needs the field
   projected.
6. **Camera**: 12340 has no camera reset on resurrection (`UpdateGhostStatus` never touches the
   camera; the view state on the cycle is `CGWorldFrame::UpdateScreenEffect`, which this client
   mirrors in `drive_death_look`). What is unproven is our own camera across the release teleport
   (A3's controller): watch it on the owner check.
7. **Area spirit healer cache** (`ui_dialog_verbs::AreaSpiritHealer`, battleground only) is not
   cleared on the unghost edge; the reference clears it in `UpdateGhostStatus`. It is re-scanned
   on the next healer packet, so no dialog is left up; named, not changed (outside the personal
   spirit-healer path).
8. **New natives are Wrath-only**; a 1.12 session still lacks them (correct: 1.12 has no
   `CannotBeResurrected`/`IsFalling`/`IsOutOfBounds`/`UnitIsControlling`/
   `GetDeathReleasePosition`).

## 6. Owner check list

Prerequisite: the overseer merges `origin/main` into this branch and the batch build lands.

1. **Die → release → run back → reclaim.** In Northshire, let a kobold kill you. The DEATH popup
   appears with the 5-minute countdown; no Lua error frame (the five natives above answer). Click
   *Release Spirit*. You are a ghost at the graveyard, the death screen pass is on, and the world
   map (M) shows a corpse marker at the death spot. Run back within 40 yd: *Recover Corpse* shows;
   the corpse body (not an invisible space) is at the death spot and the minimap corpse arrow
   points at it. Click Accept: you are alive at the corpse, at half health/mana, no ghost pass, the
   corpse marker gone, the death-release marker hidden (it is hidden while a ghost and clears at
   the resurrection).
2. **Die again → spirit healer.** Die, release, then right-click the spirit healer at the
   graveyard. The XP-loss dialog appears (not a Lua error), names the sickness duration and warns
   about durability. Accept: alive, 25% durability lost, the "Your equipped items suffer a 10%
   durability loss" line appears in chat, Resurrection Sickness on the buff bar, and the dialog's
   timer matches `GetResSicknessDuration`.
3. **Die near a second character → their resurrection → accept.** With a second character (or an
   NPC res) offering a resurrection while you are dead-but-not-released: the RESURRECT popup names
   the caster (a player's name resolves; not blank), Accept raises you, Decline re-raises DEATH and
   sends the decline. No stale resurrection popup after the answer, no ghost pass, full health/power
   from the server's fields, camera where you left it.
4. **Extra, same run:** die while falling (jump off the Northshire cliff) — the DEATH buttons stay
   usable (the `IsFalling` gate) and no Lua error repeats; open the world map while dead and while
   a ghost: no Lua error from `GetDeathReleasePosition`, and the release marker stays hidden
   (the reference's handler clears it while the ghost flag is set). If a battleground is
   available, check the spirit healer's wave countdown.

Status: in progress


## Verification state (2026-11-16)

Not compiled and not run. Cargo runs were stopped at the owner's instruction; the owner's batch
build is the first execution of this source. Source-only at this commit.
