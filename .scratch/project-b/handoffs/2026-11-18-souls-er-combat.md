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

## To see it (owner)

Build and launch as usual, enter the world, press `F10`: a gold chip and a stamina bar appear.
Then `V` (tap and hold while moving), `F9`, `F8` (tap and hold), `F7`, middle mouse on a mob.
Press `F10` again: the HUD goes and stock combat is back. To turn a feature off, write
`souls.toml` and press `F10` twice:

```powershell
Set-Content D:\Projects\wow\client\benilla-config\combat\souls.toml "[features]`nroot_motion = false"
```

## Not done, not proven

- Not compiled, not run. New unit cases: `souls/config.rs` (1), `souls/input.rs` (1),
  `souls/drive.rs` (1), `souls/targets.rs` (3), `souls.rs` (1). CI result: see the branch's run.
- Damage is the stock auto-attack the hit engages; Souls hit damage, authoritative i-frames and
  guard, sprint speed and gating stock auto-attack need the server slice (roadmap).
- The clip does not pause during hit-stop; WoW stand-in clips still play (no retarget).
- Turn-rate windows, parry, crouch/jump/left-hand attacks, poise: not done.
