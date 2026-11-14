# RustWrath — Souls combat direction (2026-11-14, client-authority redesign)

Recorded from the 2026-11-14 review + owner decision interview. Supersedes the
server-authoritative assumptions in `docs/architecture/rfc-modular-souls-combat.md` and the
compile-from-stock-clips direction in `docs/architecture/souls-combat-action-profiles.md`.
Full interview transcript and source anchors: `.scratch/project-b/handoffs/2026-11-14-souls-client-authority-direction.md`.

## Why the change

The server is private (owner + trusted people), so the wallet/authority trade-offs of a
negotiated, receipt-driven protocol are not needed. The player's client runs the full Elden
Ring moveset, decides its own hits (weapon collider against the enemy model) and its own
dodge/guard outcomes, and reports results through a new opcode pair; the server applies damage
through stock functions so log, threat, loot and XP stay stock.

## The design in one page

- **Player kit**: Elden Ring clips **and timings** (from the owner's locally generated rewrite
  data) retargeted onto the WoW human rig. Stock-clip melee/dodge profiles for the player are
  retired. The enemy clip compiler stays (contact key from the creature's own rig).
- **Client decides, server applies**: `SwingStart{swing, move}` and `SwingHit{swing,
  targets[]}` client→server; server computes damage (cleave/blob/weight below) and relays swing
  starts to other players. Incoming strikes: the client judges (area, i-frames, guard, parry)
  and reports `StrikeVerdict`; the server falls back to a 120° cone at melee reach if no report
  arrives shortly after the cast ends.
- **Enemy melee becomes a server-side cast-time strike** while the mode is on: windup 40% of
  attack time clamped 0.6–1.5 s, one strike per attack time, movement locked and facing held
  for the cast, nameplate timer bar. Auto-attack is gated for players, not for playerbots.
- **Stamina is client-only**; `ActionEngine` is its owner. The server never sees it.
- **Cleave** (distance-ranked, closest full, target k takes `damage × c^k`, per-weapon table):
  dagger 0.25/2, fist 0.30/2, 1H mace 0.40/3, 1H sword/axe 0.50/3, staff 0.50/4, polearm
  0.60/4, 2H mace 0.65/4, 2H sword/axe 0.75/5; bows/crossbows/thrown/wands single-target line;
  gun 0.35/3 on a ~1 yd line.
- **Blob rule**: targets whose selection circles touch take `avg + (c/2)·(1 − avg)` where `avg`
  is the blob's mean normal share; each member stays below 100% and the blob total exceeds the
  spread total. Cap exemption undecided.
- **Weapon speed is weight** against a 2.0 s reference (`w = speed / 2.0`); a swing's motion
  value is `w` for now. Weight shifting cleave/stagger/stamina is proposed, not confirmed.
- **Global toggle stands** (2026-11-13 decision). Retire: `CombatCore`, the receipt/settlement
  staging in `combat_mode.rs`, `CombatModuleV1`, `EnemyBrain`, and the player stock-clip
  compilers. Keep and fix `ActionEngine`/`ActionTable`/`ActionSpec`, extending the spec toward
  the rewrite's `ActionDef` (per-input cancels, dodge input window, charge window, multiple
  hits, turn locks, root motion, hit-stop).
- **First fight**: Firna (1H sword + shield) vs one Northshire kobold — timer bar, roll through
  the strike, collider light attack, mistimed roll takes the hit, empty stamina refuses, stock
  loot/XP. Out of scope: guard, parry, heavy/charged, cleave/blobs, lock-on, other weapons,
  second player.

## Known defects in the merged foundation (fix in the engine slice)

Not step-invariant (`advance_to` evaluates only at `now`); `input_from_ticks`,
`interruptible_until`, `followup` ignored; per-frame `as_millis` clock drift; prediction never
hears the world (no `interrupt`/`die`, weapon swap resets stamina, swallowed compile errors);
raw binds bypass the binding/typing gate; `ActiveClosed` missing on interrupt; `HashMap`
iteration order; `ActionSpec` too small for the ER data.

## Open

Decision 19: build order after the stock run — recommendation: client-only slice first
(retargeting is the biggest unknown), then the server slice. Retirement of the listed code
waits for that confirmation; docs record the direction now.
