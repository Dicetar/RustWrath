# Souls combat: the ER feature set, hot-togglable (2026-11-18)

Branch `feat/souls-er-combat`, cut from main `ce34ef95` ([the kit load](2026-11-18-souls-kit-load.md)).
The owner asked for every ER combat feature now, each hot-togglable, without their input; the
choices made are below. Status table: [the roadmap](../../../docs/architecture/souls-er-combat-roadmap.md).

## What it does

- `F10` is the global switch (requested mode). Off, every Souls system resets the same frame and
  the stock path returns. On, it re-reads `player.kit` and the new `souls.toml`.
- `souls.toml` (`benilla-config/combat/souls.toml`, optional) has a switch per feature and the
  binds; the example is in the roadmap and in `souls/config.rs`.
- New `souls` module:
  - `input.rs`: dodge on release (hold while moving = sprint, draining stamina), light, heavy,
    charged heavy with release swap, running attacks, guard (stamina per guarded hit, break at
    zero), guard counter, and the i-frame/guard defense the playback's interrupt rule reads.
  - `drive.rs`: root motion as a debt the controller chases, movement cancel, no-turn windows,
    lock-on facing. Written to the new `Player::souls` (`SoulsDrive`), which `move_axes` and the
    controller's facing read.
  - `targets.rs`: lock-on, and the blade hit test with hit-stop, a camera jolt, and starting stock
    melee on the target (`CMSG_ATTACKSWING`) for damage until the server slice.
  - `hud.rs`: stamina bar, mode chip, lock-on marker.
- `combat_playback.rs`: hit-stop on the playback clock, the `Defense` read in the interrupt rule,
  `PlaybackSet`, `submit_and_play`; `F10` only toggles now (the presses moved to `souls`).

## Second slice (same day)

- Turn-rate caps from the kit's `turn` windows (`drive.rs` writes `SoulsDrive::turn_rate`; the
  controller clamps the frame's turn and takes the excess out of the camera's share only).
- Hit-stop also pauses the player's clips (`targets::hold_clip`; it leaves an aura freeze alone).
- Poise and hyperarmor: playback counts poised hits (they do not interrupt); `input.rs` charges
  them, half inside a heavy's windup, breaks and staggers at zero, refills 30 s after a hit.
- Crouch toggle (`F6`) and the crouch attack; jump attacks in the air; the left-hand chain (`F11`);
  the parry (`F12`), whose `Parry` row's `iframes` window deflects a hit. Jump, left-hand and parry
  need kit rows the owner's kit does not have; the presses do nothing until it does.
- New switches: `turn_rate`, `poise`, `crouch`, `jump_attacks`, `left_hand`, `parry`.
- Wrapped the souls files' over-long lines. CI's `fmt` step fails on main repo-wide (2,559 diffs
  in 422 files since at least `cd5afee5`), so clippy and tests are skipped there; not this task's.

## Third slice: the server half (same day)

- `integrations/ale/rustwrath_souls.lua`: the server slice as ALE Lua, no core edit and no
  rebuild. Install and side effects: `.scratch/project-b/server-rebuild-batch.md` item 2.
- `souls/server.rs`: the client half. Addon self-whispers with the prefix `RWSOULS`
  (`ui_chat/net.rs` hands them over before the chat log sees them). Sends `HELLO`, `MODE 1|0`
  (keepalive every 10 s), `SPRINT`, `SWING`, `HIT` and `VERDICT`; reads `STRIKE`, `RESOLVE`,
  `PARRIED` and `SWING`.
- Hits: with the script up, `targets.rs` reports each window's struck guids (nearest first) and
  the motion value instead of starting stock melee; the server deals the damage.
- Enemy strikes: the client judges its own strikes at the deadline (reach + 0.5 yd, 120° cone from
  the creature's facing, then parry > dodge i-frames > guard > hit) and answers the verdict;
  `hud.rs` draws a windup bar over each striking enemy (orange, red in the last fifth; grey when
  on someone else). A parry turns the lock marker gold for the riposte window.
- `creature_anim/net.rs`: while the script reports, the server's damage report for the player's
  hit (and another Souls player's) does not replay a stock swing; its impact fires at once so
  floats and blood still show. Other Souls players' kit actions arrive as `SWING` and play a
  generic attack, parry or dodge anim.
- New switches: `server` (off = the stock-melee fallback even with the script installed) and
  `enemy_strikes` (off = strikes are shown but the client sends no verdict; the server judges).
- Known: the creature swings twice per strike (the emote 350 ms before resolve, then the
  damage report's swing); the verdict must arrive within 500 ms of the deadline or the server
  judges by itself; pacified creatures also lose their physical spells while managed.

## Fourth slice: stance break, crouch pose (same day)

- Server: each managed creature has a stance (30 + 2 a level, doubled for an elite; ESTIMATE).
  A player hit costs 10 × motion value × cleave share (riposte ×3); 8 s without one refills it.
  At zero the strike in flight resolves `broken`, casts are interrupted, the creature takes the
  stun stance for 3 s and a ×3 riposte opens; every Souls player nearby gets `BROKEN`.
- Fixed: the strike swing emote was 35 (`ATTACK_UNARMED`); the one-hand attack is 36.
- Crouch: `SoulsState::crouched` poses our own body as the stealth crouch in the anim driver;
  `CROUCH 1|0` makes the server slow the run ×0.5 and set the CREEP stand flag for other players.
- Found, not fixed: `unit_is_stealthed` reads byte 3 of `UNIT_FIELD_BYTES_1`; AzerothCore's CREEP
  flag is byte 2 (roadmap, Still open).

## Choices made without the owner

- Default binds: light `F9`, heavy `F8`, dodge `V`, guard `F7`, lock-on middle mouse.
- Sprint after 0.35 s held, 8 kit points/s drain; guarded hit 25 kit points; guard break 1 s;
  guard counter window 1 s. All authored, all in `input.rs` constants.
- Lock-on: selection within 30 yd, else the nearest attackable within 20 yd and 70° of the facing;
  breaks past 40 yd.
- Hits: reach from the blade's farthest end plus the radius plus half the target's combat reach;
  arc from the widest blade bearing clamped to 30–100°; ±3 yd height.
- Root motion chases the kit's motion at stock run/strafe speed (no speed control without the
  server), 0.25 yd deadband.
- Poise 51, 20 a hit, hyperarmor halves it, 30 s refill. Crouch, left and parry on `F6`, `F11`,
  `F12`. Mid-air light/heavy with no jump row does nothing (ER has no ground swing mid-jump).

## To see it (owner)

Build and launch as usual, enter the world, press `F10`: a gold chip and a stamina bar appear.
Then `V` (tap and hold while moving), `F9`, `F8` (tap and hold), `F7`, middle mouse on a mob,
`F6` then `F9` (crouch attack).
Press `F10` again: the HUD goes and stock combat is back. To turn a feature off, write
`souls.toml` and press `F10` twice:

```powershell
Set-Content D:\Projects\wow\client\benilla-config\combat\souls.toml "[features]`nroot_motion = false"
```

## Not done, not proven

- Not compiled, not run. New unit cases: `souls/config.rs` (1), `souls/input.rs` (1),
  `souls/drive.rs` (1), `souls/targets.rs` (3), `souls.rs` (1). CI result: see the branch's run.
- The server script is not installed or run. Until the owner copies it into `run\lua_scripts`,
  damage is the stock auto-attack the hit engages. New unit cases in `souls/server.rs` (3).
- WoW stand-in clips still play (no retarget); the crouch has no pose.
- Jump, left-hand and parry presses wait for kit rows; poise numbers are authored stand-ins
  until the server sends poise damage.
