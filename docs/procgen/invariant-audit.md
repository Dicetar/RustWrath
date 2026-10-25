# Procgen bounded invariant audit

Date: 2026-10-26, Europe/Moscow. Source baseline: `c42cbe2d`, with RNG repair `67f43ab5`. Owner: Astra. Review mode: source inspection and arithmetic reasoning only. No generator call, build, test, linter, formatter, import or new test case/file was run or created. This is not a runtime or portable-determinism certificate.

The preceding review/delivery corrected FIFO distances, boundary doors, single-room safety, resource validation, zero population settings, level ceilings and full-field fingerprint coverage. This audit checks the entire bounded generation path and the limitations relevant to a durable artifact importer. No new publisher, authority, persistence format or accepted architectural decision is created here. ADR-0001 remains Proposed and concerns Wrath compatibility, not acceptance of a procgen codec.

## Entry points and work bounds

| Surface | Source check | Source conclusion / remaining condition |
|---|---|---|
| `ProcgenSpec::validate` / `generate` | Item, dungeon, terrain and conservative spawn budgets checked before generating any subsystem | Invalid whole requests return an error before artifact allocation; lower entry points also validate their own request |
| `ItemRequest` | Count ≤10,000 and level 1–80; level perturbation uses i64 then clamps to 1–80 | Batch allocation bounded; no unbounded caller-supplied part pool is accepted by the public generator |
| `DungeonSpec` | Target rooms 1–1024; min edge ≥3; checked `min_room * 4`; sides ≤2048; area ≤1,048,576; loop attempts ≤4096 | `width * height` is safe in u32 after validation; map sides are nonzero and the initial `width - 2`/`height - 2` cannot underflow |
| BSP partition | Splits only at an axis with at least twice the minimum edge; each child edge on that axis ≥minimum | Leaves remain bounded by the initial interior and count never exceeds target; an unsplittable leaf is skipped, not repeatedly split |
| Room carve | Minimum dimension fits each leaf; random widths/heights do not exceed leaf; offsets lie in remaining leaf extent | Generated room cells and centers lie inside the tile map; imported arbitrary Room values do not acquire this guarantee automatically |
| Corridor carve | Endpoints are bounded room centers; inclusive axis ranges between endpoints | Tile writes remain in the map; spanning chain connects all declared rooms; extra loop attempts can be duplicate/no-op |
| Door pass | Scan the exterior ring excluding diagonal-only corner cells; replace only Corridor cells | Door marks share a cardinal room edge; no new obstruction or entity is created; adjacent/touching rooms may need no corridor-door marker |
| Graph distance | FIFO queues and first discovery in both dungeon and population | Shortest declared room-graph distances, including loops; not shortest tile-space travel distances |
| Archetypes | One entrance; distinct farthest boss when room count >1; single-room entrance retained; treasure selected from other rooms | A one-room dungeon has no boss; `boss_room == entrance` is metadata, not a request to spawn one |
| `TerrainSpec` | Nonzero sides ≤2048; samples ≤1,048,576; finite frequency 0–1024; 1–16 octaves; finite sea level 0–1 | Sample-vector lengths and iteration counts bounded; no zero-octave division or NaN/Inf input from validated requests |
| FBM arithmetic | Frequency grows at most 2^15 during sampled octaves; moisture offset/frequency bounded; lattice coordinates stay within i32 | No saturating float→integer edge needed for valid generated samples; normalization has an epsilon span floor |
| Population budget | Base level 1–80; ≤16 normal packs; ≤100 attempts/100 samples; checked conservative room + terrain actor estimate ≤100,000 | Actual actor count is at most that estimate; each room's extra budget covers entrance/boss/treasure roles conservatively |
| Population supplied shapes | Nonempty bounded dungeon, correct tile/terrain vector sizes, room IDs/index bounds, room rectangles, graph targets and reachability checked before iteration | This is shape protection, not full semantic artifact validation or proof that the data was generated here |
| Population levels/indices | Depth ≤number of rooms−1; dungeon area/sides bounded; terrain area nonzero and ≤1,048,576 | Base+depth fits u32; emitted enemy levels clamp at 80; generated tile/sample coordinates are in their domain |
| Zero controls | Zero normal packs gives zero normal-room packs; terrain attempts use `samples * density / 100` with no forced minimum | Zero density means no overworld attempts; boss/adds, treasure guards and entrance NPC rules remain separate |
| Master RNG forks | Forks use immutable origin and domain, never current rolling state | Increasing item count does not advance other subsystem streams; population remains dependent on dungeon/terrain and shares one stream across its two domains |

Potential work is bounded, not benchmarked. At maximum budgets, tile carving/validation and allocation can still be expensive. Repeated `room_tiles` allocates a per-room coordinate vector; this is bounded by the map but is not a performance acceptance result. Agree actual memory/time budgets and exercise representative worst cases only when execution is authorized.

## RNG overflow repair and stream compatibility

The old inclusive helper computed `max - min + 1` in i64 and added an i64-cast offset. A request such as `i64::MIN..=i64::MAX` overflows even though it is a valid inclusive domain. Repair `67f43ab5` computes the span as u128 through i128 endpoints and adds the offset in i128. The widest span is exactly 2^64 and the result remains within the original endpoints.

For every bounded generator call whose old span/addition were defined, both implementations consume one `next_u64`, take the same modulus and return the same integer. Reversed/equal bounds retain the old behavior and consume no random value. No `ProcgenSpec` algorithm, catalog, default, tag or fingerprint field changed; the generator remains `procgen-v2`.

Modulo reduction is not exactly uniform for non-power-of-two spans. The helper documentation now says so rather than promising exact uniformity. Rejection sampling would change streams and needs an explicit generator version change; it was not introduced. `choose` requires a nonempty slice; `weighted` requires a positive total and nonempty input. Those public helper preconditions still exist. The built-in pools/candidates used by valid generation satisfy them; this audit does not claim arbitrary callers receive fallible validation from every low-level RNG helper.

## Manifest coverage

`lib.rs::manifest_hash` covers the supported request and all fields of the current output except the manifest hash itself. Strings/sequences have length prefixes, enum tags are explicit, numbers are little endian, f32 values use bit patterns and ordering is significant.

| Data | Fields inspected against structs |
|---|---|
| Identity/request | Format marker, generator version, seed, item count, all DungeonSpec/TerrainSpec/SpawnSpec fields |
| GeneratedItem | ID, name, kind, rarity, level, ordered stat/value pairs, ordered parts |
| Part | ID, slot, name, weight, minimum rarity, ordered stat/bonus pairs; no pointer address is hashed |
| Dungeon | Width/height, ordered tiles, ordered rooms with ID/bounds/archetype/connections, entrance/boss indices |
| TerrainField | Dimensions, height and moisture arrays, biomes |
| Spawn | Domain, coordinates, enemy archetype/boss flag or NPC role, level, encounter ID |

The output's duplicated seed/version match generation inputs by construction, but a future importer must check them explicitly. The hash function is private and there is no validated import/export API today. FNV-1a is not authentication or a collision-free identity. Full request retention and catalog/build identity are still needed for replay. Exact cross-platform floating arithmetic is unproven; canonical bit encoding alone cannot prove it.

## Deliberate limitations and importer obligations

- Generated items still use the current generic part pools across equipment kinds; a generated bow/staff is not a complete WoW item template or balance design. Part provenance does not allocate a WM/server entry or supply client-visible art.
- Room graph paths and tile paths can differ because corridors may cross other rooms/corridors. The entrance is edge-nearest by center, not an exterior map entrance. Doors are walkable markers, not actual doors/collision.
- Population samples with replacement. NPCs, adds and packs may overlap. Boss and adds currently have different encounter IDs; consumers must preserve them or apply an explicitly declared grouping policy, not silently assume one shared boss encounter ID.
- Terrain uses local normalized sample coordinates, per-field normalization and no adjacent-field origin/height contract. It is not a seam-validated ADT terrain pipeline, world-coordinate field, vmap/mmap result or live geometry feature.
- Public data structs are mutable/constructible. `Room::contains`, `Dungeon::room_tiles`, `Dungeon::all_reachable` and terrain indexing helpers assume coherent data. A durable importer must validate dimensions, checked rectangle arithmetic, arrays, graph symmetry/indices, archetype counts, finite values and actor placement before calling them.
- Population's supplied-artifact checks do not reject every semantic error: wrong archetypes, asymmetric graph links, a solid boss center, overlapping rooms/actors or a mislabeled domain require stronger handoff validation. An accepted shape does not prove client/server traversability or authority.
- No complete request retention, stable durable codec, import validation, owned deserialized parts or production consumer exists yet. These are specified in the companion handoff proposal and remain implementation/approval work; this audit does not substitute a hand-authored sample for generated output.

## Pending execution evidence

When separately authorized: verify bounded/full-i64 RNG behavior and unchanged supported streams, invalid request refusal before allocation, graph distances with loops, door adjacency, single-room safety, zero controls, actor bounds/levels, manifest sensitivity for every field and repeated outputs. Durable roundtrip/corruption/oversize/catalog-mismatch cases belong to the future codec slice. No new tests have been written, and none of these checks is claimed to have passed.

The bounded source audit is complete for these entry points and current structs. The production generated-space requirement remains open. See [the handoff proposal](artifact-handoff.md), [current generator contract](design.md) and the Astra delivery checkpoint for scope and ownership.
