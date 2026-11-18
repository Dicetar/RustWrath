# Souls kit loaded in the app (2026-11-18)

Branch `feat/souls-kit-load`, cut from main `764ba373`. Follows
[the ER kit converter](2026-11-15-souls-er-kit.md), whose "Next slice" this is. The owner was
away and asked for the recommended choices; the choices made are listed below.

## What it does

- `combat_kit.rs` (new): `PlayerKit` reads `benilla-config/combat/player.kit`
  (`local_state::player_kit_path`) at startup and again each time `F10` requests Experimental.
  A missing file logs info; a file that does not read or parse warns with the line.
- `fill_local_profiles` compiles the kit into the engine table when there is one, and rebuilds
  when the kit changes (its generation is part of `built_for`). A kit that does not compile warns
  and the stock dodge/swing table stands in.
- Presses resolve through the kit (`resolve_press`):
  - `F9` (light): the running action's `next_light`, else `Light1`. So Light1→…→Light5→Light1,
    and a roll or backstep followed by `F9` gives RollAttack or BackstepAttack.
  - `V` (dodge): RollFront/RollBack/RollLeft/RollRight from the held movement keys (forward and
    back win over a strafe), Backstep with no key held.
- Stamina: kit costs and bar ×20 (`STAMINA_SCALE`) so the reference's 45 points a second is a
  whole 9 per tick at 100 Hz. The bar is the kit's `stamina_max`. The regeneration delay stays the
  authored 100 ticks. A rebuild keeps the bar only if the stamina config did not change; a revive
  uses the kit's config.
- Clips are stand-ins: dodges play the rig's Dodge (30), attacks the rig's main-hand swing.
  `combat_profiles.rs` gained `rig_dodge_clip` / `rig_swing_clip`, which the stock compilers now
  use too.

## Choices made without the owner

- File location `benilla-config/combat/player.kit`, one kit, no weapon/grip selection.
- ×20 stamina scale and 45/s regeneration (the reference's own ESTIMATE, taken as policy).
- Roll direction from `MovementState.flags` without lock-on, as the donor rolls unlocked.
- Kit reload on entering Experimental rather than a file watcher.

## Local setup done

The owner's converter output was copied (not committed; `/benilla-config` is ignored):

```powershell
Copy-Item D:\Projects\wow\worktrees\feat-souls-er-kit\out\kits\longsword-1h-medium.kit D:\Projects\wow\client\benilla-config\combat\player.kit
```

## To see it (owner)

Build and launch as usual, enter the world, press `F10`. The log should show
`souls kit: …player.kit — Longsword, one-handed, medium load (20 actions)`. Then `V` with and
without movement keys, and `F9` repeatedly: actions run on the kit's timings with WoW clips.

## Not done, not proven

- Not compiled, not run. Two new unit cases in `combat_kit.rs` (press resolution; stamina
  scaling and bindings). CI result: see the branch's run.
- Root motion (`motion_at`) is not applied: rolls do not move the character beyond normal
  movement. Next slice.
- Not consumed: heavy presses (no bind), the charge window, hits after the first, per-hit stamina
  timing, turn locks, jump/move/left-hand cancels, RunLight/CrouchAttack openers.
- No stamina display; the bar only gates actions.
- Clip retarget (step F) untouched; `source` still only names the donor clip.
