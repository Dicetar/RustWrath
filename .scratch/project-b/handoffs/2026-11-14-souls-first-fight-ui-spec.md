# Handoff Spec: Souls first-fight UI and presentation

Written 2026-11-14 for the next session. Scope is the UI needed by the first Souls fight
(owner decision 17: Firna, 1H sword + shield, one Northshire kobold). Every number below is a
**proposed starting value**, not an accepted design and not taken from any game; tune in play.
Nothing here is implemented. Design of record: `docs/architecture/souls-combat-client-authority.md`.

## Overview

Five pieces, in build order:

1. **Strike timer bar** on the enemy nameplate: shows the enemy's windup so the roll can be timed.
2. **Stamina bar**: the client-only resource that gates roll and swing (decision 15).
3. **Mode indicator**: whether Souls mode is on.
4. **Refusal feedback**: why a press did nothing.
5. **Debug hit markers** (developer build only): proves the sword collider before the server exists.

Stack: Rust + Bevy 0.18. Nameplates are a 2-D overlay whose widgets live in
`crates/benilla-ui/src/script/nameplate.rs` and whose world side is
`crates/benilla-app/src/vplates.rs`. Player frames are stock FrameXML (Lua). The install is
multi-locale (`enus`, `ruru`, `dede`, `eses`, `frfr`): all text must come from a string table.

## Prerequisite (in progress)

Enemy plates could not be switched on in a Wrath session: the 12340 gate reads the CVars
`nameplateShowEnemies` / `nameplateShowFriends` (`Unit_C.cpp:20459`, `:20463` in the pinned
reference), but RustWrath registered both as inert and only the 1.12 verbs wrote the mode.
Fix is on branch `fix/wrath-nameplate-cvars` (worktree `D:\Projects\rw-worktrees\fix-wrath-nameplate-cvars`),
**source only, not compiled or run**. Also note the plate widget is the 1.12 one: it has **no
cast bar** (`nameplate.rs:5`), so the timer bar is a new child, not a restyle.

## Layout

Plates are sized in fractions of the damped screen-diagonal basis ("gx units"), outside uiScale.
Existing constants (`vplates.rs:102-116`): plate 0.100 × 0.025; health bar 0.0804 × 0.007025 at
offset (0.0031, 0.003125) from the plate's bottom-left.

| Element | Anchor | Size (gx units) | Offset |
|---|---|---|---|
| Strike timer bar | Plate bottom-left, hangs **below** the plate | 0.0804 × 0.0045 (health-bar width, ~64 % of its height) | x 0.0031; gap 0.0015 under the plate's bottom edge |
| Stamina bar | Screen bottom-centre | 0.160 × 0.006 | bottom edge 14 % of screen height above the screen bottom (clears the main action bar) |
| Mode indicator | Left end of the stamina bar, above it | text, cap height 0.008 | 0.002 above the bar |
| Refusal text | Stock `UIErrorsFrame` position | stock | stock |

No breakpoints: everything scales with the diagonal basis like the plates do. On ultra-wide
screens the stamina bar stays centred; it does not stretch.

## Responsive behaviour

| Window | Changes |
|---|---|
| 1024×768 (smallest supported) | Timer bar may fall under 3 physical pixels tall: clamp its height to 3 px and keep the width |
| Up to the 1280-diagonal knee | Everything scales linearly with the diagonal, as plates do |
| Past the knee (1440p, 4K) | Growth is damped by the existing plate rule (`PLATE_DIAG_KNEE`, `PLATE_GROWTH_DAMP` in `vplates.rs`); the stamina bar uses the same damped basis so it matches the plates |
| Ultra-wide (21:9, 32:9) | Stamina bar stays centred at its fixed width; plates are unaffected |
| uiScale changed by the player | No effect: these elements sit outside uiScale, like the plates |

## Design tokens

| Token | Value (linear RGBA) | Usage |
|---|---|---|
| `souls-bar-bg` | 0, 0, 0, 0.60 | Background of timer and stamina bars |
| `souls-bar-border` | 0, 0, 0, 1.0; one logical pixel | Bar outline |
| `souls-strike-early` | 1.00, 0.70, 0.00 | Timer fill, first 70 % of the windup |
| `souls-strike-late` | 1.00, 0.25, 0.10 | Timer fill, last 30 % |
| `souls-strike-dodged` | 1.00, 1.00, 1.00 | Flash when the strike was avoided |
| `souls-strike-void` | 0.50, 0.50, 0.50 | Bar after the enemy was staggered or died mid-windup |
| `souls-stamina` | 0.35, 0.75, 0.25 | Stamina fill |
| `souls-stamina-ghost` | 0.80, 0.90, 0.60 | The amount just spent, fading |
| `souls-stamina-empty` | 0.90, 0.15, 0.10 | Frame flash at zero stamina |
| `souls-mode` | 1.00, 0.70, 0.00 | Mode text |

## Components

| Component | Data it needs | Source | Notes |
|---|---|---|---|
| `PlateStrike` (new child of a plate) | `progress` 0..1, `phase` | Cast start of the generic strike spell (duration = windup), then cast end / failure; `spell/net.rs::on_spell_start` already receives cast starts | Add `strike: Option<PlateStrike>` to `PlateState` (`nameplate.rs:65`); build in `create`, seat in `lay_out`, update in `drive` |
| `StaminaBar` (new native overlay) | current, max, regen-delayed flag | `CombatPlaybackState.engine` in `combat_playback.rs` (`stamina()`), max from the stamina config | Native overlay like the plates; do not build it in FrameXML |
| `ModeIndicator` | requested / effective mode | `CombatModeView` in `combat_mode.rs` | Text from the string table |
| Refusal line | last refusal | `CombatPlaybackState.last_refusal` | Route through the existing UI error line |
| Debug markers | blade segment, bone capsules, contacts, strike cone | collider slice | Bevy gizmos, `dev` feature only |

## States and interactions

| Element | State | Behaviour |
|---|---|---|
| Timer bar | No strike | Hidden; takes no space |
| Timer bar | Winding up | Fills left to right, linear, reaching full exactly when the cast ends. Colour switches from early to late at 70 % |
| Timer bar | Strike resolved, you were hit | Full bar flashes `souls-strike-late` at full brightness for 150 ms, then hides |
| Timer bar | Strike resolved, avoided (dodge, out of area) | Flashes `souls-strike-dodged` for 150 ms, then hides |
| Timer bar | Enemy staggered or died mid-windup | Fill freezes, turns `souls-strike-void`, fades out |
| Timer bar | Plate not shown (over 20 yd, off screen, occluded) | No bar; no off-screen indicator in the first version |
| Stamina bar | Full, out of combat for 3 s | Fades out |
| Stamina bar | Spend | Appears at once if hidden; fill drops immediately; ghost segment shows the spent amount |
| Stamina bar | Regeneration delayed | Fill at 70 % brightness until regeneration resumes |
| Stamina bar | Zero | Frame flashes `souls-stamina-empty` twice |
| Mode indicator | Off | Hidden |
| Mode indicator | Requested, server has not acknowledged | Grey text, slow pulse |
| Mode indicator | Active | `souls-mode` text; also one system chat line on each change |
| Refusal | Press refused | Error line plus the stamina-bar flash when the cause is stamina; same message at most once per second |

No mouse interaction on any of these. The plate itself keeps its existing hover and click.

## Content

| String key | English | Limit |
|---|---|---|
| `SOULS_MODE_ON` | Souls combat enabled | 40 chars |
| `SOULS_MODE_OFF` | Souls combat disabled | 40 chars |
| `SOULS_MODE_LABEL` | SOULS | 12 chars |
| `SOULS_ERR_STAMINA` | Not enough stamina | 40 chars |
| `SOULS_ERR_DEAD` | You are dead | 40 chars |
| `SOULS_ERR_NO_ACTION` | That action is not available | 40 chars |

Limits allow for Russian and German, which run longer than English. No truncation: if a
translation exceeds the limit, shorten the translation.

## Edge cases

- **Several enemies winding up**: each plate shows its own bar. No global bar.
- **Windup clamp**: the shortest windup is 0.6 s (decision 16); the bar must still show at least
  the late colour for 180 ms.
- **Strike starts while the plate is hidden, then the plate appears**: the bar appears already
  part-filled; never restart it from zero.
- **Enemy plates switched off by the player**: open question below.
- **Cast start arrives late** (network hitch): fill from the server's elapsed time, not from zero.
- **Weapon swap or death during a refusal flash**: clear the flash.
- **Stock creature casts** (real spells) are unaffected: the timer bar is only for the strike spell.

## Animation and motion

| Element | Trigger | Animation | Duration | Easing |
|---|---|---|---|---|
| Timer fill | Windup | Width 0 → 100 % | The windup (0.6–1.5 s) | Linear |
| Timer colour | 70 % of windup | Early → late | Instant | — |
| Timer flash | Strike resolved | Brightness peak then hide | 150 ms | Ease-out |
| Timer void | Stagger or death | Alpha 1 → 0 | 200 ms | Linear |
| Stamina ghost | Spend | Alpha 1 → 0 | 400 ms | Ease-out |
| Stamina empty | Reaching zero | Two flashes | 2 × 120 ms | Linear |
| Stamina show | Spend while hidden | Alpha 0 → 1 | 150 ms | Linear |
| Stamina hide | 3 s full and out of combat | Alpha 1 → 0 | 500 ms | Linear |
| Mode pulse | Requested, not acknowledged | Alpha 0.4 ↔ 1.0 | 1 s period | Sine |

## Accessibility

- **Not colour alone**: the early/late switch is also a brightness step, and the resolved states
  differ by flash versus fade. Check the amber/red pair against common colour-vision deficiencies
  before accepting the tokens.
- **Motion**: provide one setting that turns off the pulses and flashes (fills stay).
- **Legibility**: the timer bar must stay at least 3 physical pixels tall at the smallest
  supported window; raise its height if the basis makes it thinner.
- **Keyboard**: actions go through the binding engine (`bindings.rs`), which already has the
  typing gate; no UI element here takes keyboard focus.

## Debug hit markers (developer build)

| Marker | Look | Lifetime |
|---|---|---|
| Blade segment | Cyan line, hand to tip | Only inside a hit window |
| Enemy bone capsules | Orange wireframe, within 20 yd | While the overlay is on |
| Contact point | Magenta cross, 0.15 yd | 0.5 s |
| Enemy strike area | Red arc on the ground, 120°, radius = melee reach | During the windup |
| Log line | `would hit <name>, bone <n>, tick <t>` | — |

One toggle for the whole overlay; off by default; compiled out of the player build.

## Open questions for the owner

1. With Souls mode on, should enemy plates be forced visible for enemies fighting you, even if
   enemy nameplates are switched off? Recommended: yes, otherwise there is no telegraph.
2. Should the timer bar show the "roll now" zone (the span of the bar in which a roll's i-frames
   cover the strike)? It makes timing teachable but also easier. Recommended: not in the first fight.
3. Stamina bar position: bottom-centre as proposed, or under the player frame?
