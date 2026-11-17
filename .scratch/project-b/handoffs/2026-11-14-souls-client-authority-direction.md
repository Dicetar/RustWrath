# RustWrath — Souls combat direction handoff (2026-11-14, second session)

Written by the Claude session that reviewed the project and then ran a decision interview
("grilling") with the owner. Audience: the next session. Read
`F:\AI\tmp\opencode\claude-onboarding-handoff.md` first (project orientation, hazards, rules),
then this file. Nothing in RustWrath was changed, built or committed by this session.

## 1. State in one paragraph

The owner changed the Souls combat design. The merged foundation (and its RFC) assumed a
server-authoritative model with receipts, snapshots and negotiated revisions. The server is only
for the owner and trusted people, so the new design is **client-decides, thin server module**:
the player's client runs the full Elden Ring player moveset, decides its own hits (weapon
collider against the enemy model) and its own dodge/guard outcomes, and reports results through
a new opcode; the server applies damage through stock functions. Enemy melee becomes a
server-side cast-time "strike" so there is a telegraph. Elden Ring data has been generated
locally from the owner's install and is ready to use.

## 2. Where things are

| Path | What |
|---|---|
| `D:\Projects\project-b-client` | Primary RustWrath checkout, `main` at `38f56810`, **9 commits behind `origin/main` (`7f124813`)**. Shared: do not pull/switch/build there without ownership. The Souls files exist only on `origin/main`. |
| `D:\Projects\elden-ring-combat-rewrite-reference` | Clone of `Funny-Bones/ELDEN-RING-Combat-Rewrite` at `177a3b5` (still upstream HEAD). MIT. **Data is generated**: `src/sim/extracted.rs` (action table) and `assets/player_anims.bin` (1,479 clips, 52 MB). Sound was skipped. All generated files and `er-files/` are gitignored. |
| `D:\Projects\er-unpack-tools` | UXM Selective Unpack, WitchyBND, and `finish_setup.py` (re-runs the generator). |
| `D:\SteamLibrary\steamapps\common\ELDEN RING\Game` | Owner's Elden Ring. Has ModEngine and Seamless Coop. `Game\chr` (unpacked copies) may be deleted by the owner. Never Patch/Restore with UXM. |
| `D:\WOW\WM_BridgeLab\src\azerothcore` | Live server's AzerothCore source, commit `5e741c3`. Read-only unless the owner assigns server work. Modules under `modules\` include `mod-multiclass`, `mod-wm-bridge`, `mod-wm-spells`, `mod-wm-prototypes`, `mod-playerbots`, `autobalance`. |
| `https://github.com/jackke88/wow-souls` | Reference only (not cloned). Single-player souls-like on benilla 1.12. Game code MIT/Apache; `crates/dsr-formats` and `crates/dsr-cache` are **GPL-3.0** — read for technique, do not copy into RustWrath. |

To read current RustWrath code without touching the checkout:
`git -C D:\Projects\project-b-client archive origin/main crates docs .scratch Cargo.toml | tar -x -C <scratch dir>`.

## 3. Owner decisions from this session (record these in `docs/baseline/owner-decisions.md`)

1. **Sequencing**: one owner-driven **stock** Northshire run comes before any Souls feel work,
   limited to what Souls also needs: login, world entry, movement, one creature fight, one spell
   cast. Enemy **nameplates are currently broken** — fix during this run (prerequisite for the
   timer bar).
2. **Source**: Elden Ring Combat Rewrite is the source. Nothing from Dark Souls Remastered;
   wow-souls/DSR are reference for retargeting technique only.
3. **Player moves**: Elden Ring clips **and** timings, extracted from the owner's install and
   retargeted onto the WoW rig. (This overrides "never an extracted Elden Ring number" and the
   compile-from-stock-clips direction, for the player kit.)
4. **Enemy attacks** are server-side cast-time spells replacing auto-attack while Souls mode is
   on. Movement must not cancel the player's own swings.
5. **Toggle stays global** (the 2026-11-13 decision stands; a per-player idea was withdrawn).
6. **Cleave**: targets ranked by distance; closest takes full damage; target k takes
   `damage × c^k` up to a per-weapon cap. Starting table (tune in play):

   | Weapon | `c` | Max targets |
   |---|---|---|
   | Dagger | 0.25 | 2 |
   | Fist / unarmed | 0.30 | 2 |
   | 1H mace | 0.40 | 3 |
   | 1H sword, axe | 0.50 | 3 |
   | Staff | 0.50 | 4 |
   | Polearm | 0.60 | 4 |
   | 2H mace | 0.65 | 4 |
   | 2H sword, axe | 0.75 | 5 |
   | Bow, crossbow, thrown, wand | — | 1 (first target on a line) |
   | Gun | 0.35 | 3 (line, ~1 yd wide) |

   Reach/arc columns were dropped for melee after decision 11 (blade and clip define them).
7. **Blob rule** (purpose: reward hitting enemies stacked on the tank): targets whose selection
   circles touch (centre distance ≤ both radii + ~0.5 yd) form a blob. Compute normal falloff
   first; every blob member then takes `avg + β × (1 − avg)` with `avg` = the blob's mean share
   and `β = c / 2`. Each member < 100 %, blob total > spread total. (Optional, undecided: a blob
   ignores the target cap.)
8. **Weapon speed is "weight"** against a **2.0 s** reference: `w = item speed / 2.0 s`.
9. **Enemy windup**: the enemy stops and its facing is locked from attack start until it resolves.
10. **Player damage**: motion value is calculated from weapon speed (`MV = w`), to be tuned
    later — in effect a swing does about the weapon's stock hit. *Assumption not confirmed:* a
    heavy attack keeps a multiplier over a light one. *Proposed, not confirmed:* weight also
    shifts cleave (`c_eff = clamp(c + 0.15·(w−1), 0.10, 0.90)`), stagger and stamina cost
    (`× (0.5 + 0.5·w)`); the type table then needs re-centring because every 2H is slower than 2 s.
11. **Player hits are decided on the client**: full Elden Ring movement/attacks; a hit is the
    weapon collider touching the enemy model, as wow-souls does.
12. **Hit reporting**: a new Souls server module with its own opcode pair. Client sends "swing
    started" (move) and "swing hit" (targets). Server computes and applies damage (cleave, blob)
    through stock damage functions and relays swing start to other players.
13. **Incoming strikes**: each player's own client judges (inside area, dodge i-frames, guard,
    parry) and reports the outcome; the server applies it. If no report arrives shortly after the
    cast ends, the server falls back to its own area check.
14. **Playerbots stay stock** (auto-attack and abilities). Enemies still telegraph against them;
    a strike against a bot uses the server area check, no dodge.
15. **Stamina is client-only**. The server never sees it; `ActionEngine` is its real owner.
16. **Enemy rule (starting values)**: windup = 40 % of the creature's attack time, clamped
    0.6–1.5 s; one strike per attack time; 120° frontal cone out to melee reach; normal melee
    damage; the client plays the creature's own attack clip so its hit key lands when the cast
    ends; a timer bar runs on the nameplate. Creature spells/abilities stay stock. Per-family
    overrides later.
17. **First Souls fight**: Firna (level 1 human warrior, GUID `0x1522`) with 1H sword + shield vs
    one Northshire kobold: timer bar, roll through the strike, collider-based light attack, a
    mistimed roll takes the hit, empty stamina refuses the roll, kill with stock loot/XP.
    Out of scope: guard, parry, heavy/charged, cleave/blobs, lock-on, other weapons, second player.
18. **Retire the old design**: keep and fix `ActionEngine`/`ActionTable`/`ActionSpec`; keep the
    enemy clip compiler (hit key from the rig clip). Delete `CombatCore` (the one-dodge fixture),
    the receipt/settlement staging in `combat_mode.rs`, `CombatModuleV1`
    (`benilla-hotpatch-abi/src/lib.rs` ~209–285), and `EnemyBrain`. Retire the stock-clip melee
    and dodge compilers for the player. Mark `rfc-modular-souls-combat.md` and
    `souls-combat-action-profiles.md` superseded and write one short design doc.

**Open**: decision 19, build order after the stock run. Recommendation given: client-only slice
first (retargeting is the biggest unknown and needs no server), then the server slice.
The owner said "do not act until we confirm shared understanding": **show the owner this decision
list for a final yes before deleting code or editing docs.**

**Parked**: bot damage is not balanced against stamina-limited players; each trusted player
needs their own Elden Ring copy (generated data is never committed or shared); whether the HD
profile replaces the human model (check at the stock run, also Firna's gender/model);
`autobalance` interplay with strike damage; enemy cleave against several players; guard/parry
rules.

## 4. Facts verified in source (with anchors)

AzerothCore `5e741c3`, `src/server/game`:
- Player casts are interrupted by movement only if the spell has `SPELL_INTERRUPT_FLAG_MOVEMENT`
  (`Spells/Spell.cpp:3517`, `:4375`) — a custom swing spell without it allows movement.
- Creatures cannot move during a normal cast: `Unit::IsMovementPreventedByCasting`
  (`Entities/Unit/Unit.cpp:4549`); only a channel with `SPELL_ATTR5_ALLOW_ACTION_DURING_CHANNEL`
  is exempt. Fits decision 9.
- `UnitAI::DoMeleeAttackIfReady` (`AI/CoreAI/UnitAI.cpp:40`) has no script hook and already
  returns while `UNIT_STATE_CASTING`. Replacing auto-attack needs a small core edit there →
  **one server rebuild and restart** (owner-coordinated).
- Spell script hooks exist for target list and per-target damage: `OnObjectAreaTargetSelect`,
  `BeforeHit`, `OnEffectHitTarget`, `SetHitDamage` (`Spells/SpellScript.h:336–435`).
- Custom spells load from the `spell_dbc` table (`DataStores/DBCStores.cpp:371`).
- Stock melee (`SMSG_ATTACKERSTATEUPDATE`, opcode `0x14A`) is sent when the swing lands — no
  advance notice, which is why the strike must become a cast.
- Existing custom opcode precedent: `mod-multiclass`, client `0x51b` / server `0x51a` (WMC1 v3);
  client side in `crates/benilla-protocol-wrath/src/multiclass.rs` and the worker dispatch in
  `crates/benilla-app/src/wrath.rs`; doc `docs/architecture/multiclass-host-2026-11-05.md`
  (runtime acceptance still open).

Elden Ring rewrite (`src/sim`): 60 Hz tick, timings in 30 fps animation frames (`data.rs:21-25`).
`ActionDef` (`data.rs`) has `total`, `input_from`, `input_dodge_from`, seven per-input cancel
windows (`cancel_light/heavy/dodge/jump/guard/move/left`), `iframes`, `hits[]` (each with
window, motion value, stamina, hit-stop, radius and per-frame **blade end positions**),
`charge`, `no_turn`, `turn`, per-frame root `motion`. Hit test = blade segment vs body capsule
(`mod.rs` `segment_distance`, `player_attacks`). Estimated values are marked `ESTIMATE`.

wow-souls (technique reference): `crates/dsr-cache/src/retarget.rs`, `bonemap.rs`, `monster.rs`
(retarget onto WoW skeletons); `crates/wow-souls/src/hitbody.rs` (one capsule per skinning bone
fitted from the mesh), `hits.rs` (blade tip→hand swept prev→now), `motion_clip.rs`,
`rig_view.rs` (side-by-side debug view), `lockon.rs`. Retarget formula in `docs/animations.md` §5:
`q_wow = q_wow_bind · (q_ref⁻¹ · q_anim)`, bones matched by role; root motion goes to the
movement controller, the clip stays in place.

## 5. Defects found in the merged Souls code (fix in the engine slice)

Paths are on `origin/main`.
- **Not step-invariant**: `ActionEngine::advance_to` (`benilla-combat-core/src/engine.rs:232–315`)
  evaluates only at `now`, checks the buffer TTL before the cancel start, and stamps events with
  `now`. A frame hitch drops an input that per-tick stepping would start. Fix: walk boundaries in
  order (or step per tick, bounded); add a test that one jump equals tick-by-tick for any trace.
  The app has **no `FixedUpdate` systems**; run the clock at the Elden Ring 60 Hz.
- **Three spec fields are ignored** by the engine: `input_from_ticks`, `interruptible_until`,
  `followup`. A swing can cancel into itself at contact end (`profile.rs:98`).
- **Clock drift**: `time.delta().as_millis() as u64` in `benilla-app/src/combat_playback.rs:180`
  truncates per frame (≈4 % slow at 60 fps, ≈14 % at 144). Accumulate a `Duration`.
- **Prediction never hears the world**: nothing calls `interrupt`/`die`; no dead/stunned/mounted
  gate; a weapon swap rebuilds the engine with full stamina (`combat_playback.rs:126–168`,
  keyed on entity + weapon only); compile errors are swallowed with `if let Ok(Some(..))`/`.ok()`.
- **Binds bypass the binding engine**: raw `just_pressed(F10/V/C)` (`combat_playback.rs:210`),
  so typing in chat triggers actions and stock binds collide. Register commands through
  `benilla-app/src/bindings.rs` (it has the typing gate).
- `ActiveClosed` is not emitted when an action is interrupted inside its window; `ActionTable`
  iterates a `HashMap` (non-deterministic order).
- `ActionSpec` is too small for decision 3: extend it toward `ActionDef` (per-input cancel
  windows, dodge input window, charge window, multiple hits, turn locks, root motion, hit-stop).

## 6. Work plan (what, where, how)

**A. Feel reference (now).** In `D:\Projects\elden-ring-combat-rewrite-reference` run
`cargo run` (Bevy 0.19, first build 10–20 min, target dir on D:). Check with the owner that no
RustWrath Cargo job is running first. Owner plays it; this is the target feel.

**B. Stock run (owner-driven).** Owner pulls the primary checkout to `origin/main`. Follow
`docs/agents/local-launch.md`. Owner enters credentials and drives. Record what works for:
login, world entry, movement, one fight, one spell cast. Diagnose nameplates
(`crates/benilla-app/src/nameplates.rs`, `vplates.rs`, `crates/benilla-ui/src/script/nameplate.rs`).
Note Firna's model and which asset profile (`base` or `wm-hd`) she is played on.

**C. Docs and retirement (after the owner's final yes on §3).** On a focused branch in a
worktree on D:. Commit the untracked RFC, mark it and the action-profiles plan superseded, add
`docs/architecture/souls-combat-client-authority.md` from §3, append §3 to owner decisions.
Delete per decision 18; `cargo check -p benilla-app` and `cargo test -p benilla-combat-core`.

**D. Engine slice.** Fix §5 in `benilla-combat-core` first (dependency-free; its tests run in
seconds), then `combat_playback.rs`. Stay within the repo's 20-case verification limit.

**E. Elden Ring data converter.** A small local tool that turns `src/sim/extracted.rs` into an
action-table file RustWrath loads at runtime from a gitignored local path (for example under
`benilla-config/`). First scope: 1H straight sword light chain, medium roll, backstep. The clip
format is defined by `tools/bake_anims.py` and read in the rewrite's `src/anim.rs`/`src/rig.rs`;
the skeleton comes from `tools/skel.py`. Never commit generated data.

**F. Retarget Elden Ring → WoW human rig (the biggest unknown).** Build a bone map by role from
the Elden Ring player skeleton to the human M2 (use M2 key-bone ids), apply the formula in §4,
strip root motion into a movement curve for the controller, fit the WoW sword to the Elden Ring
blade axis. Play through the existing one-shot path (`ActionAnim` in
`creature_anim/driver.rs`, `RigPose` in `benilla-world/src/rig_anim/pose.rs`,
`build_animation_clip` in `benilla-assets/src/model.rs`). Build a side-by-side debug view first.

**G. Collider.** One capsule per skinning bone fitted from the creature mesh; sweep the blade
segment (from the Elden Ring per-frame blade ends) between frames during hit windows; one hit
per swing per target. Show debug markers before any server work.

**H. Server slice (owner-coordinated; needs the rebuild).** New module beside `mod-multiclass`.
- Messages: C→S `SwingStart{swing, move}`, `SwingHit{swing, targets[]}`,
  `StrikeVerdict{caster, cast, outcome}`; S→C swing relay and the global mode state.
- Core edit in `DoMeleeAttackIfReady`: when the mode is on, start the generic strike cast
  (custom `spell_dbc` row, cast time per decision 16, root and lock facing). At cast end wait
  briefly for the verdict, else 120° cone at melee reach. Apply normal melee damage through the
  stock path so log, threat, loot and XP stay stock. Bots: stock attacks, server area check.
- Gate player auto-attack while the mode is on (bots exempt).
- Damage maths for player swings (decisions 6–8, 10) lives in the server module.
- Client: timer bar from the strike's cast start; align the creature's attack clip so its hit
  key lands at cast end (`combat_profiles.rs` `clip_summary`/`enemy_melee_actions`); verdict from
  the engine's i-frame window plus position against the cone.

**I. The first fight** (decision 17), owner-driven.

## 7. Other weak spots from the review (not Souls)

- CI (`.github/workflows/ci.yml`) triggers on pushes to `wotlk` and PRs, but work is pushed to
  `main`: CI never runs on it. Add `main` plus a fast check job.
- App suite recorded at 5514 passed / 1231 failed (fix-round handoff, 2026-11-13), mostly 1.12
  UI tests on the 3.3.5a install. Skip/delete them, keep a known-failures baseline.
- Last client run on record is 2026-11-02 (`docs/agents/active-ownership.md`, itself stale).
- `benilla-app` is 604K of 1.1M lines; `wrath.rs` is 13.5K lines with a 2,044-line function.
  Extract the Wrath session/translation layer first.
- ADR-0001 is still "Proposed". 115 worktrees / 119 local branches. Extracted FrameXML sits
  un-ignored in `.scratch/project-b/`.
- Not reviewed: hot-reload crates, WM native channel, procgen,
  `D:\WOW\PROJECT_B_COMPLETION_PLAN.md`, the early part of `owner-decisions.md`, ADR-0002.

## 8. What is unverified

Everything here is source and docs review plus design decisions. No RustWrath build, test or
client run happened this session. The Elden Ring sandbox has not been built or run yet. The
retarget onto a WoW rig has never been tried. The spell/cast route was checked against source,
not against a running server. UXM/WitchyBND were run by the owner, not by the session.
