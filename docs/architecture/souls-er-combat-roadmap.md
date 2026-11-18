# Souls combat: the Elden Ring feature set (roadmap and status)

The Souls mode ([RFC](rfc-modular-souls-combat.md), [profiles](souls-combat-action-profiles.md),
[authority](souls-combat-client-authority.md)) runs the owner's ER kit
(`benilla-config/combat/player.kit`) in the client's playback engine. This page lists every ER
combat feature, where it lives, and what still needs the server.

## The toggle

- **Global, at runtime.** `F10` flips the client's *requested* mode. On, every Souls system runs;
  off, each resets its own state the same frame, so stock movement, facing, targeting and
  combat return. Turning it on re-reads `player.kit` and `souls.toml`, so a changed kit or
  switch applies without a restart.
- **Per feature.** `benilla-config/combat/souls.toml` has one switch per feature and the binds.
  A missing file means every feature on and the default binds; a file that does not parse is
  ignored, with a warning naming the problem, and the previous configuration stays.

```toml
[features]
root_motion = true
turn_lock = true
lock_on = true
heavy = true
charge = true
sprint = true
running_attacks = true
guard = true
guard_counter = true
iframes = true
hits = true
hit_stop = true
engage_on_hit = true
hud = true
turn_rate = true
poise = true
crouch = true
jump_attacks = true
left_hand = true
parry = true

[binds]  # A-Z, 0-9, F1-F12 (not F10), Space, Tab, Backquote, LeftShift, LeftCtrl, LeftAlt,
         # CapsLock, MouseLeft, MouseRight, MouseMiddle, Mouse4, Mouse5
light = "F9"
heavy = "F8"
dodge = "V"
guard = "F7"
lock_on = "MouseMiddle"
crouch = "F6"
left = "F11"
parry = "F12"
```

## Features

| ER feature | Status | Where | Switch |
|---|---|---|---|
| Light chain (Light1–5), roll/backstep follow-ups | done (kit slice) | `combat_kit::resolve_press` | — |
| Dodge on release, direction from held keys, backstep standing | done | `souls/input.rs` | — |
| Sprint (hold dodge while moving), stamina drain | done; speed is stock run speed (server) | `souls/input.rs` | `sprint` |
| Running light/heavy (RunLight/RunHeavy) | done | `souls/input.rs` | `running_attacks` |
| Heavy chain (Heavy1→Heavy2) | done | `souls/input.rs` | `heavy` |
| Charged heavy: hold charges, release in the window swaps to the uncharged swing (refunded) | done | `souls/input.rs` | `charge` |
| Root motion (rolls, steps, lunges move the body) | done, chased by the controller at stock speeds | `souls/drive.rs` | `root_motion` |
| Movement cancel of recovery | done | `souls/drive.rs` | `root_motion` |
| Turn lock in no-turn windows | done | `souls/drive.rs` | `turn_lock` |
| Lock-on (toggle, auto-pick, break at range/death), A/D circle | done | `souls/targets.rs`, `souls/drive.rs` | `lock_on` |
| I-frames: an incoming hit in a dodge's i-frames does not stagger | done (prediction only) | `combat_playback.rs` | `iframes` |
| Stagger: an incoming hit interrupts until the attack's first hit | done (kit slice) | `combat_playback.rs` | — |
| Guard: hold to block, stamina per guarded hit, guard break at zero | done (prediction only) | `souls/input.rs` | `guard` |
| Guard counter: heavy right after a guarded hit | done | `souls/input.rs` | `guard_counter` |
| Hit detection from the kit's blade (reach, arc, per-hit windows) | done (client test) | `souls/targets.rs` | `hits` |
| Hit-stop and camera jolt; the player's clips pause for the stop | done | `souls/targets.rs`, `camera_shake.rs` | `hit_stop` |
| Damage | interim: a landed hit starts stock melee on the target | `souls/targets.rs` | `engage_on_hit` |
| Stamina bar, mode chip, lock-on marker | done | `souls/hud.rs` | `hud` |
| Turn-rate caps (`turn` windows, degrees a second) | done | `souls/drive.rs`, `player/controller.rs` | `turn_rate` |
| Poise: each hit taken costs poise and does not interrupt; at zero it breaks and staggers; refills 30 s after the last hit | done (prediction only, authored numbers) | `souls/input.rs`, `combat_playback.rs` | `poise` |
| Hyperarmor: inside a heavy's windup and swing a hit costs half the poise | done (prediction only) | `souls/input.rs` | `poise` |
| Crouch (toggle) and the crouch attack (light from a crouch) | done; no crouch pose or speed (clip and server) | `souls/input.rs` | `crouch` |
| Jump attacks (light/heavy in the air) | done; needs `JumpLight`/`JumpHeavy` rows, which the owner's kit lacks | `souls/input.rs` | `jump_attacks` |
| Left-hand chain | done; needs `LeftLight1…` or `PairedLight1…` rows (the importer's opt-in) | `souls/input.rs` | `left_hand` |
| Parry: the `Parry` row's `iframes` window deflects a hit | done (prediction only); needs a `Parry` row; riposte and attacker stagger need the server | `souls/input.rs` | `parry` |
| Clip retarget (ER motion on the WoW rig) | not done (step F) | — | — |

## Needs the server slice (queued, not rebuilt)

- Souls hits as damage: an opcode carrying the hit (action, hit index, target), resolved by the
  server engine with motion values and guard damage, and `effective` mode confirmation.
- Incoming i-frames and guard as authority: today they only shield the client's prediction; the
  server still lands the stock melee hit and its damage.
- Sprint speed, and stopping stock auto-attack while the mode is on (right-click `StartAttack`
  still swings).
- Enemy telegraphs (cast-time strikes) for the enemy brain.
- Poise and poise damage per attack, the parry's riposte and the attacker's parry stagger.

## Order of the next slices

1. Server mode channel and the hit opcode (rebuild batch).
2. Clip retarget (ER motion on the WoW rig), and a crouch pose.
3. Kit rows for jumps, the left hand and the parry (`kit_import` options), then tuning.
