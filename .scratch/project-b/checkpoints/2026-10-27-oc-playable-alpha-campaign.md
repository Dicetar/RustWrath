# Playable-alpha campaign — baseline, collision repair and the pending run

Date: 2026-10-27. Owner: OpenCode (campaign lead). Branch `wotlk`, `D:/Projects/project-b-client`.
This is the working record; the requested execution is one run card
(`.scratch/project-b/run-cards/2026-10-27-northshire-loop.md`), not a bundle of asks.

## Baseline actually established

- HEAD at campaign start: `7897e642` (Astra's coroutine heap commit) on top of my Stage-3
  `ae1af2c5`; the worktree was clean except two untracked owner files
  (`.scratch/project-b/verification/`, `tools/patch_procgen_dungeon.py`), both preserved.
- Astra committed UI/coroutine work in this same worktree during the campaign start:
  `6a9965c1`, `1368a420`, `1fbd4010` (UnitCastingInfo/UnitChannelInfo over the mirror),
  `9cf1594b` (native cast/channel events into the Wrath VM), `f4552cf0`, `93d6fbcb`. They are
  preserved; `1fbd4010`/`9cf1594b` were reviewed against my Stage-2 authority and are consistent
  (same attempt serial in getter and event payload; terminal `select(4, …) == castID` matches;
  channel getter uses the START/update anchors; interruptibility stays nil/unknown).
- Rendered route identity: `WM_ASSET_PROFILE=wm-hd`, manifest SHA-256
  `439bdc21…c5083d59`; data `D:\WOW\world of warcraft 3.3.5a hd\data`. Intended character is the
  pinned eligible human mage (never established live; the select screen names it).
- Lab server processes were already running (auth pid 25176, world pid 27100, mysqld 39852/41944,
  since 2026-10-24). No server/DB command was run by this campaign.
- Execution: standing build + minimal-test grant only. Commands run this campaign:
  `cargo check -p benilla-world`, `cargo check -p benilla-app --lib`, `cargo check -p benilla`,
  `cargo build -p benilla`, `cargo test -p benilla-app --lib wrath`,
  `cargo test -p benilla-app --lib wrath_ui_slice`; two throwaway parry reproducers outside the
  repo; `profile-probe read` for the 3.3.5 CastingBarFrame.lua contract. No graphical, live,
  socket, WM receiver or DB run.

## Workstream 1 — parry binned-BVH panic: repaired at the mechanism

`a8a51af9` (+ `1f61f21a`, + build repair `d8250c5c`). The failure is in parry3d 0.26.1's binned
builder, not the mesh: a two-leaf cluster whose AABB centres differ by a subnormal in y makes
`k1 = 8/delta` overflow, `(k1*delta) as usize` saturates to `usize::MAX`, and `bins[usize::MAX]`
panics. `third_party/parry3d` vendors 0.26.1 with one `bin_index` closure used by both the binning
pass and the in-place sort (finite values clamped to `NUM_BINS-1`, `+inf` to the last bin, `NaN`
to the first); the builder's existing single-bin half-split then handles the cluster. No geometry
clamp, no skipped collider, no registry edit, no upgrade. `third_party/parry3d/BENILLA.md` records
provenance, mechanism, drop condition and check.

Verification actually obtained:

- Standalone reproducer (two valid nearly coincident flat triangles + 64 ordinary triangles):
  registry parry3d 0.26.1 panics with the exact recorded error
  (`bvh_binned_build.rs:58:28`, len 8, index 18446744073709551615); the vendored crate builds the
  same 66-triangle mesh.
- `cargo check -p benilla-world` and `cargo build -p benilla` link avian3d against the fork.
- The graphical half of the reproducer (streamed-tile attach in the rendered client) still needs
  the run; a repeat panic disproves the repair and must be re-diagnosed.

## Native aura availability repair (Astra's source counter-evidence)

`adc8a631`. Her review proved the native aura owner could not distinguish unobserved from
observed-empty: the reducer's slot vector defaults empty, create clears it, and both adapters
exported Known/empty for any existing entry; an empty full snapshot produced no revision. The
repair adds per-generation `ObjectEntry.auras_observed` (set by any aura packet, including a
GUID-only `SMSG_AURA_UPDATE_ALL`), makes a full snapshot always a source-observation edge and a
delta an edge on first receipt or a real slot move, carries the flag into the digest, exports
`unknown` until coverage exists, and refuses `UnitAura` answers before the snapshot. Full-snapshot
replacement semantics and per-aura receipt clocks/expiration remain explicitly open (her contract
says not to guess the replacement policy). Verified by `cargo check` for the protocol/app/driver
and the 12 wrath + Wrath UI slice tests.
## Workstreams 2–4 — route source state

The route source chain is in place and compiles: parked select screen → Wrath session → digest →
`wrath_apply_world` (player/creatures/items, SelfPlayer pose, world-enter cascade) → stock
controller/camera/input → `wrath_stream_movement` (12340 opcodes, flags map, no transport pose) →
terrain/WMO colliders (now building) → cast UI (`UnitCastingInfo`/`UnitChannelInfo`, native
`UNIT_SPELLCAST_*` events) and the translated quest/gossip/loot owners. Astra's cast UI was
integrated by inspection and targeted tests; the stock 1.12 `SPELLCAST_*` events do not collide
with the 3.3.5 `UNIT_SPELLCAST_*` family. No source claim of rendered acceptance is made.

## Remaining blockers, ordered

1. **Authorized rendered/live run (G+L).** Without it, no claim about terrain attach, rendered
   player/creatures, camera/input, grounding, the Willem→Garrick route or the quest/loot/reward
   loop can be made. Run card above; server already up; credentials are owner-held.
2. **Runtime evidence for collision.** W1's builder path is proven in isolation and by link; the
   streamed attach path needs the run. Expected failure signature is recorded in the run card.
3. **Live character eligibility.** The pinned mage must exist, be level 2–5, know Fireball 133 and
   not have quest 6 completed. That is a live fact; the run card stops if it does not hold.
4. **AUTO_ACCEPT behaviour for quest 6.** Base SQL has `AUTO_ACCEPT`; the run records whether an
   explicit accept is needed, with quest 5261 as the control. No source change is justified
   before that evidence.
5. **WM/W receiver acceptance (W)** — Stage-3 source is committed; not exercised this campaign.
6. Broader compatibility rows (aura durations/polarity presentation, cooldown initial payload,
   secure UI/coroutine edges) remain open; none is known to block the first route.

## Next critical-path task

The single authorized Northshire run in the run card, at the source identity of this commit.
Until that decision, no further speculative route code is added; the next source slice after the
run is chosen from its evidence.
