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
lock_targets = true  # the lock is the WoW target; Tab or a click onto a foe moves the lock
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
server = true
enemy_strikes = true
retarget = true
retarget_locomotion = true
er_controls = true   # the Elden Ring layout below overrides the WoW bindings it names
free_camera = true   # the camera orbits freely, movement is camera-relative

[binds]  # A-Z, 0-9, F1-F12 (not F10), Space, Tab, Backquote, Escape, Enter, LeftShift,
         # LeftCtrl, LeftAlt, CapsLock, MouseLeft, MouseRight, MouseMiddle, Mouse4, Mouse5;
         # the combat binds (light to parry) take a "Shift+" prefix
light = "MouseLeft"
heavy = "Shift+MouseLeft"
guard = "MouseRight"
parry = "Shift+MouseRight"
left = "Mouse5"
dodge = "Space"
lock_on = "Q"
lock_on_alt = "MouseMiddle"
crouch = "X"
jump = "F"           # runs JUMP
interact = "E"       # the selection in reach, else the nearest usable thing in front
use_item = "R"       # runs [controls] quick_item
walk = "LeftAlt"     # hold to walk
cursor = "LeftCtrl"  # tap: cursor up for the action bars and panels; tap again: back

[controls]
quick_item = "ACTIONBUTTON12"
cursor_keys = ["Escape", "B", "C", "P", "M", "L", "N", "O", "I", "K", "U", "H", "J"]
```

## The Elden Ring layout (`er_controls`)

- **Orbit.** With the mode on the cursor is hidden and the mouse orbits the camera. Left click is
  the light attack, Shift+left the heavy, right click the guard, Shift+right the parry. The world
  neither selects nor turns with those buttons while the camera orbits.
- **Cursor.** A tap of `LeftCtrl` alone brings the cursor up: both buttons are the stock ones
  again, so the action bars, the panels and world clicks work as ever. Another tap puts it away.
  A panel key (`cursor_keys`) or an interaction that opens a window brings it up too, and the
  next movement key puts it away.
- **Keys.** A key the layout names runs no WoW binding of its own (Space no longer jumps, Q and E
  no longer strafe). `F`, `R` and `LeftAlt` run `JUMP`, the quick item and `TOGGLERUN` instead.
  Every other key keeps its binding, so `1`-`=` cast from the hotbar without the cursor.
- **Free camera** (`free_camera`). Movement is camera-relative: W/A/S/D and the turn keys name a
  bearing off the camera, the body turns toward it at 720°/s and runs. An action's first 3 kit
  frames snap to the held bearing (a roll or a swing goes where the stick points). Lock-on picks
  what the camera looks at and swings the camera behind the body toward the target. Off, the
  orbit turns the body as right-button mouselook does.
- **F10 off**, or `er_controls = false`: nothing is claimed, and the cursor, the buttons and every
  binding are stock.

## Features

| ER feature | Status | Where | Switch |
|---|---|---|---|
| Equipped main-hand kit (dagger, swords, axes, maces, polearm, staff, fist, spear), re-picked on weapon change; missing file → player.kit → stock | done; owner runs `scripts/import-kits.ps1` | `combat_kit.rs`, `scripts/combat-kits.csv` | — |
| Light chain (Light1–5), roll/backstep follow-ups | done (kit slice) | `combat_kit::resolve_press` | — |
| Dodge on release, direction from held keys, backstep standing | done | `souls/input.rs` | — |
| Sprint (hold dodge while moving), stamina drain | done; ×1.4 run speed from the server script | `souls/input.rs` | `sprint` |
| Running light/heavy (RunLight/RunHeavy) | done | `souls/input.rs` | `running_attacks` |
| Heavy chain (Heavy1→Heavy2) | done | `souls/input.rs` | `heavy` |
| Charged heavy: hold charges, release in the window swaps to the uncharged swing (refunded) | done | `souls/input.rs` | `charge` |
| Root motion (rolls, steps, lunges move the body) | done, chased by the controller at stock speeds | `souls/drive.rs` | `root_motion` |
| Movement cancel of recovery | done | `souls/drive.rs` | `root_motion` |
| Turn lock in no-turn windows | done | `souls/drive.rs` | `turn_lock` |
| Lock-on (toggle, auto-pick, break at range/death), A/D circle | done | `souls/targets.rs`, `souls/drive.rs` | `lock_on` |
| Lock-on is the WoW target: a lock selects its unit (target frame, server selection); Tab or a click onto an attackable unit moves the lock, onto anything else releases it | done | `souls/targets.rs` (`lock_follows_target`), `target/by_name.rs` | `lock_targets` |
| I-frames: an incoming hit in a dodge's i-frames does not stagger | done; with the server script a strike in them is a dodge (no damage) | `combat_playback.rs` | `iframes` |
| Stagger: an incoming hit interrupts until the attack's first hit | done (kit slice) | `combat_playback.rs` | — |
| Guard: hold to block, stamina per guarded hit, guard break at zero | done; with the server script a guarded strike deals 30 % | `souls/input.rs` | `guard` |
| Guard counter: heavy right after a guarded hit | done | `souls/input.rs` | `guard_counter` |
| Hit detection from the kit's blade (reach, arc, per-hit windows) | done (client test) | `souls/targets.rs` | `hits` |
| Hit-stop and camera jolt; the player's clips pause for the stop | done | `souls/targets.rs`, `camera_shake.rs` | `hit_stop` |
| Damage: weapon roll × motion value, cleave falloff, blob share | done by the server script; without it a landed hit starts stock melee | `souls/targets.rs`, `souls/server.rs`, `integrations/ale/rustwrath_souls.lua` | `server` (`engage_on_hit` without it) |
| Stock auto-attack off while the mode is on | done by the server script (blocks `CMSG_ATTACKSWING`) | `rustwrath_souls.lua` | `server` |
| Enemy telegraphs: sweep (90° half-cone), thrust (20°, +2 yd), slam (circle, ×1.5 windup), orange/blue/purple bars; model-authored attack sequences scaled to land their impact at windup end; creatures stop melee and wind up strikes (40 % of attack time, 0.6–1.5 s), a windup bar over them, the client judges reach/cone/i-frames/parry/guard | done (server script + client) | `souls/server.rs`, `souls/hud.rs`, `rustwrath_souls.lua` | `enemy_strikes`, `server` |
| Enemy stance break: hits cost the creature stance (30 + 2/level, ×2 elite; 10 per 100 % motion value), at zero it is stunned 3 s, its strike and casts cancelled, and a ×3 riposte opens | done (server script; the client shows the gold riposte marker) | `rustwrath_souls.lua`, `souls/server.rs` | `server` |
| Stance-break critical | light from the front in melee range plays Critical or Heavy1; the server consumes the opening once and applies defended critical damage | `souls/targets.rs`, `souls/server.rs`, `rustwrath_souls.lua` | `server` |
| Backstab | light in melee reach from the rear 90° of a creature not targeting the player; server rechecks the opening | `souls/targets.rs`, `rustwrath_souls.lua` | `server` |
| Creature hit reactions: light flinches, heavy stance damage interrupts a windup, stance break takes priority | per-creature cooldowns; interrupted strikes deal no damage | `rustwrath_souls.lua` | `server` |
| Other Souls players' swings, parries and dodges use the local kit's ER clips; unknown ids or missing bake/rig fall back to generic | done; at most 16 bodies, 512 bones each | `souls/server.rs`, `souls/retarget.rs` | `server` |
| Stamina bar, mode chip (teal while the server answers), lock-on marker (gold while a riposte is open) | done | `souls/hud.rs` | `hud` |
| Turn-rate caps (`turn` windows, degrees a second) | done | `souls/drive.rs`, `player/controller.rs` | `turn_rate` |
| Poise: each hit taken costs poise and does not interrupt; at zero it breaks and staggers; refills 30 s after the last hit | done (prediction only, authored numbers) | `souls/input.rs`, `combat_playback.rs` | `poise` |
| Hyperarmor: inside a heavy's windup and swing a hit costs half the poise | done (prediction only) | `souls/input.rs` | `poise` |
| Crouch (toggle) and the crouch attack (light from a crouch) | done; the stealth crouch pose (StealthStand/StealthWalk) on our body, ×0.5 run speed and the pose for others from the server script | `souls/input.rs` | `crouch` |
| Jump attacks (light/heavy in the air) | done; needs `JumpLight`/`JumpHeavy` rows, which the owner's kit lacks | `souls/input.rs` | `jump_attacks` |
| Left-hand chain | done; needs `LeftLight1…` or `PairedLight1…` rows (the importer's opt-in) | `souls/input.rs` | `left_hand` |
| Parry: the `Parry` row's `iframes` window deflects a hit | done; needs a `Parry` row; with the server script a parried strike opens a ×3 riposte on that creature for 1.5 s | `souls/input.rs` | `parry` |
| Clip retarget: each kit action plays the ER clip it was read from on our own rig (spine, head, arms, legs by role; pelvis height) | done (local and relayed player actions); needs `benilla-config/combat/player_anims.bin` | `souls/retarget.rs` | `retarget` |
| ER locomotion: idle, guard, walk/run/sprint by direction, crouch idle/walk/run, airborne | done (locomotion stays local; relayed actions retarget too; clip rates are estimates) | `souls/retarget.rs` | `retarget_locomotion` |
| ER control layout: hidden-cursor orbit, mouse attacks and guard, `LeftCtrl` cursor toggle, claimed keys override their WoW bindings, jump/use item/walk remaps | done | `souls/controls.rs`, `bindings.rs`, `player/camera.rs`, `ui_script/input.rs` | `er_controls` |
| Interact key: the selection in reach, else the nearest usable thing in front (corpse loot, chest, door, mailbox, friendly NPC) | done | `souls/controls.rs` | `er_controls` |
| Free camera: camera-relative movement, the body faces where it runs, actions snap to the held bearing, camera-picked lock-on | done | `souls/drive.rs`, `player/controller.rs` | `free_camera` |

## The server half (ALE Lua, no rebuild)

`integrations/ale/rustwrath_souls.lua`, copied into the server's `run\lua_scripts` (ALE reloads it
by itself; deleting it turns it off). The client talks to it over addon self-whispers with the
prefix `RWSOULS` (`souls/server.rs`): `HELLO`, `MODE 1|0` every 10 s while F10 is on, `SPRINT`,
`CROUCH`, `HIT <mv%> <guids>`, `SWING <action>`, `VERDICT <id> <outcome>`; the server answers
`STRIKE <id> <creature> <windup-ms> <base-reach> <mine> [sweep|thrust|slam]`, `RESOLVE`, `PARRIED`, `BROKEN`, `SWING`. The optional trailing shape preserves old-client parsing; absent or unknown means the legacy 60° half-cone. Entry policy is in `STRIKE_KIND_BY_ENTRY`; unspecified creatures sweep. F10 off sends `MODE 0` and the server hands every creature back to
stock AI. Without the script the client stays on the stock-melee fallback (`engage_on_hit`).
Install and side effects: `.scratch/project-b/server-rebuild-batch.md` item 2.

## Still open

- Retarget checks in game: the bone roles are found from the bind pivots and attachments, not
  checked against the HumanMale skeleton; hand roll and the weapon's angle come from the forearm
  aim; the locomotion clip speeds are estimates. Remote locomotion still uses stock clips; relayed actions use the ER bake.
- Kit rows for jumps, the left hand and the parry (`kit_import` options), then tuning.
