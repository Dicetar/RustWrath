# World fidelity: 3.3.5 weather, the water ambient loop, and the attach table

Status: ready to merge
Owner: OpenCode / DeepSeek V4.1 Flash (overseer) with one AutoSwarm `opencode` child
Branch: fix/wrath-world-fidelity
Issue: none
Next: owner launch checks for the attach/sheath ceremony, an indoor pool and a zone change with weather

Based on `origin/main` `ad0a6a15`. Brief B3 in `rustwrath-agent-briefs-2026-11-15.md`; the F03
gate list in `docs/architecture/release-matrix-2026-11-05.md` is the scope.

## What exists now

### SMSG_WEATHER was read with the donor layout (fixed)

3.3.5 reads `u32 weatherId, f32 grade, u8 abrupt` (9 bytes) and takes the effect type, transition
and ambience kit from the `Weather.dbc` row it names (`CGGameUI::MessageHandler`, GameUI.cpp
`:6642`-`:6667`). The tree read a fourth `u32 soundId` (13 bytes), so a live packet ran past its
body and the event was dropped. Now:

- `crates/benilla-protocol/src/messages/parse.rs`: the 9-byte body; `SessionEvent::Weather`
  carries `weather_id, grade, instant`;
- `crates/benilla-formats/src/weather.rs` (new): `Weather.dbc` reader — `AmbienceID`,
  `EffectType`, `TransitionSkyBox`, `EffectColor[3]`, `EffectTexture`;
- `crates/benilla-world/src/weather/mod.rs` and `crates/benilla-app/src/net/world.rs`, `sound/weather.rs`,
  `world_point.rs`: the row is resolved once and drives the effect kind and the ambience kit.

### The water ambient loop follows 3.3.5 (fixed)

`crates/benilla-app/src/sound/liquid_loop.rs` was rewritten to `SI2::HandleWaterSounds`
(`SoundInterface2Water.cpp`, `0x4D2390`): one loop for the nearest liquid point within 60 yd, a
500 ms scan only after the player moved (`_OkToQueryLiquidSounds`, `0x4D2230`), 1 s fades, the
emitter following the player, and a hard stop when the listener is underwater. The old path left one
emitter per submersion behind; the new state keeps a single slot.

### Attach and sheath ids already matched 12340 (pinned, no behaviour change)

`entities/equipment` `attach_id` and `resolve::placement` were compared value by value against
`GEOCOMPONENTLINKS` (`ATTACH_SHIELD` 0, `HANDLEFT/RIGHT` 1/2, `SHEATH_MAINHAND` 26, `_OFFHAND` 27,
`_SHIELD` 28, `BACK` 30, `BACK_OFFHAND` 31, `HIP` 32, `HIP_OFFHAND` 33, nocked arrow 35),
`CCharacterComponent::GetSheatheLink` (`0x004E7940`) and `AddHandItem` (`0x004EACD0`): every leg
already agreed. The comments that still described 1.12's sheath names were corrected, and a test now
pins drawn melee/shield/bow/gun/thrown and every stowed `SheatheType` leg.

## Checks actually run

Pinned toolchain 1.98.1, own target dir, `CARGO_INCREMENTAL=0`.

| Check | Result |
|---|---|
| `cargo check -p benilla --features hotpatch` | Finished, no errors |
| `cargo test -p benilla-formats weather` | 1 passed |
| `cargo test -p benilla-protocol weather` | 1 passed |
| `cargo test -p benilla-app --lib liquid_loop` | 2 passed |
| `cargo test -p benilla-app --lib attach_table` | 1 passed |
| `cargo test -p benilla-world --lib weather` | 36 passed |

The branch also carries a compile fix for `crates/benilla-protocol/tests/items.rs`: two `ItemInfo`
fixtures predate the `wrath_roll` field, so `cargo test -p benilla-protocol` did not build on
`main`. It is a test-only fixture update, unrelated to world fidelity.

## Gaps recorded, not fixed

- **Cast stow exception.** 12340 stows while a cast runs unless `Spell.dbc` has
  `SPELL_ATTR0_DONT_AFFECT_SHEATH_STATE` (0x40000) (`UpdateHandItemAnimEffects`, `0x00738180`).
  The anim layer stows from the cast kit's `WeaponFlags` with no spell-attribute exception; the
  attribute is not plumbed into `creature_anim` (the spell crate is another owner).
- **Stowed ranged model after a draw cycle.** `ShowRangedWeapon` (`0x0072B7F0`) attaches the bow at
  its sheath link and leaves it there until a visible-item update; our slot-2 rule hides it as soon
  as the weapon mode leaves 2, so a hunter who draws and stows shows no bow on the back. Needs
  per-slot hysteresis, not a table change.
- **Interiors (F03 item 2).** No `WorldStateZoneSounds`/`WorldChunkSounds` rungs and no
  interior-class ambience selection were found to compare; the WMO interior lighting path was not
  changed.
- **Zone music/ambience (item 5).** The zone-music id path exists; the donor ghost ambience rung is
  absent. Recorded, not changed.
- **Spell-heavy scenes (item 6).** Particle instance cap 128 per emitter and the 256-light
  nearest-first table are the reference's limits; no change was made.

## Not proven

No client run. The weather fix is proven at the packet and row level, the water loop at its state
machine, the attach table at the value table; the visible ceremony, an indoor pool's sound and a
zone change with live weather are owner-launch checks.

## Owner check list

1. Zone change with weather (Elwynn → a rainy zone): rain renders, ambience starts, clears.
2. A pool/river: the loop fades in within a second and stops when you dive.
3. Draw and sheathe melee, a shield, a bow and a gun: the model moves to hand/back/hip correctly.
4. Northshire Abbey interior: no missing ambience or lighting regression.
