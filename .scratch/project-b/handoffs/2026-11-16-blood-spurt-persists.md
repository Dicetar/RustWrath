# Blood spurt cloud hangs over a melee-killed corpse

Status: landed on main; not compiled, not run, not verified in-game
Owner: Claude Code (Opus 5.5)
Branch: fix/blood-spurt-persists
Issue: none (owner screenshot 2026-11-16, Northshire Young Wolf, build at main `a73b08ba`)
Next: the owner's batch build and a melee kill on the HD install. Expect a spray from the chest
that leaves nothing in the air.

Date 2026-11-16. Worktree `D:/Projects/rw-worktrees/fix-blood-spurt-persists`, cut from `main`
`a73b08ba`. Per the owner's rule nothing here was compiled or run: no Cargo check, test or
build. The owner compiles the client and reports errors.

## Symptom

After a Young Wolf is meleed to death, a dark red splatter cloud stays in the air over the corpse
instead of spraying and fading.

## Root cause (asset evidence plus a source read of the 12340 reference)

- The install is the HD pack (`D:/WOW/world of warcraft 3.3.5a hd`). Its `patch-b.mpq` replaces
  `PARTICLES\BloodSpurts\BloodSpurt.m2`: 10528 bytes and 6 emitters, against stock `common-2.mpq`'s
  6768 bytes and 4. It also adds the whole `PARTICLES\BloodSpurts\Splats\` folder
  (`BloodSplat{Red,Black,Blue,Green,Purple}01.m2`) and the `BloodSplatNew_*` textures. None of
  these is in the stock archives.
- Stock emitters #0–#3 are identical in both copies, with lifespans 0.15–0.5 s and no geometry.
- HD emitter #5 has these properties:
  - bone 0 at the origin, flags `0x21221`, world mode (file `0x10` clear), no ground snap
    (file `0x2000` clear), geometry model `Splats\BloodSplatRed01.m2`;
  - a rate of 12/s for about 0.33 s, gated off at 0.5 s, so about 4 shards;
  - speed 0, speed variation 0, gravity 0, lifespan 45 s, alpha 1 over life.
- Bone 0 has no translation track in either copy. The splat model is one flat quad, 2.78 yd
  across at z=0 (about 1.4 yd at particle scale 0.5), with an alpha-blended `BloodSplatRed03.blp`.
  It is a ground decal.
- The spurt hangs on the victim's chest attachment, 15 front or 16 back (`blood.rs`, reference
  `0x625010`). The decals are born there and never move.
- The instance's span clock ends it and `drain_instance_emitters` (`spell_fx/mod.rs`) flips its
  emitters to `OwnerLoss::Drain`, so the shards lived out their 45 s at chest height. That is the
  red cloud.
- The reference does the same with this data. `0x6203e0` hides and queues the node, the particle
  draw admits by live count, and the node is freed once nothing is live. So this is not a teardown
  divergence: the HD asset authors decals that 12340 has no means to put on the ground.

## Candidates from the brief

- **Model never despawned: ruled out.** The span clock despawns the root, and emitters drain or
  free after it.
- **Emitter keeps emitting: ruled out.** A draining emitter does not emit, and #5 gates itself
  off at 0.5 s anyway.
- **Gravity or velocity not applied: ruled out.** The data has none to apply.
- **Model-space particles on a still corpse: ruled out.** #5 is world mode.

## Fix (a named divergence)

`ParticleEmitter::drain_on_owner_loss` (`benilla-world/src/particles.rs`) is the shared teardown
for every ending kit instance: expiry, same-slot replace and reap. It now frees a geometry-particle
pool whose shards are parked, rather than draining it. Parked means every live particle has zero
velocity and gravity is zero at the emitter's clock.

- A parked pool can only hang where the effect stood, which is never spray.
- Shards in flight or under gravity keep the reference drain, and so does every quad cloud. This
  includes stock spurt #3, a 0.35 s zero-speed plane, and the `Strike_Impact_Chest` drain from
  `1d7252ae`.
- Gravity is sampled on the emitter's own spawn-age clock, not the host's playing sequence. That
  only matters for an emitter whose gravity is keyed, and none in this case is.

Shared-path check: `benilla-extract shardcensus` misreads Wrath (v264) models and returned
`[UNRESOLVED]` byte soup for every pair, so I have no corpus-wide count of other parked shard
emitters. The rule is data-driven rather than tied to an asset name, so any other model with
resting geometry shards gets the same teardown.

Not done: ground-projecting the HD decals (dropping the splats onto terrain for their 45 s) is a
larger, separate divergence. The owner asked for spray and fade, so the shards go with the
instance instead.

## Evidence levels

- Asset: dumped with `benilla-extract m2part` and Python header and bone readers against the HD
  data and a stock-only archive set. Extracted files stayed in `F:/AI/tmp/bloodspurt`, outside the
  repo, and nothing was committed.
- Source: reference teardown read (`Effect_C.cpp` MoveToDestroyList / CleanUpStaticLists,
  `M2Model.cpp` HasLiveParticles, `ParticleSystem2.cpp`). No code copied.
- Compilation: not run. One regression test was added and has not been run:
  `particles::tests::an_ending_instance_frees_its_parked_blood_splats`. It ends a parked
  45 s geometry pool and expects `OwnerLoss::Free`.
  The existing `an_ending_instance_switches_its_emitters_from_free_to_drain` covers a quad
  emitter and still expects `Drain`.
- Runtime and visible gameplay: not run. The owner launches.
