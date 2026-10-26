# Procedural generation data and consumer boundaries

`benilla-procgen` is a data generator and offline artifact/planning library in the `crates/*` Cargo workspace. It reuses serde/serde_json, with no Bevy, MPQ, network or running WM/server dependency. It includes owned persistence candidates, encounter planning and inert existing-owner WM drafts; it has no accepted production consumer, renderer or live publisher. Source support here does not satisfy generated-space or full stock-client replacement acceptance.

Current source interfaces: [owned artifact API](artifact-api.md), [encounter planning](encounter-planning.md), [WM drafts/receipts](wm-drafts.md) and [offline CLI](offline-cli.md). The [invariant audit](invariant-audit.md) and [earlier handoff proposal](artifact-handoff.md) retain their original review scope. The implementation supersedes that proposal's implementation-pending statements; production schema acceptance remains separate. RNG range overflow was repaired in `67f43ab5` without changing valid bounded generator streams. No execution evidence was added.

## Request and failure contract

`benilla_procgen::generate(&ProcgenSpec)` returns `Result<ProcgenOutput, ValidationError>`. The request contains the seed, item count, dungeon parameters, terrain parameters and population parameters. The top-level call validates all parameters and worst-case population before generating artifacts. Errors identify the field and reason. The subsystem entry points for items, dungeons, terrain and population also return `Result`; direct callers must propagate or explicitly handle errors.

The whole-slice item request uses the existing default level 10; direct item requests can select a level. This is not a player-level or authoritative reward policy.

| Parameter | Accepted budget / behavior |
|---|---|
| Item batch | 0–10,000 items; direct item level 1–80, individual rolls clamp the existing ±2 band to 1–80 |
| Dungeon | At most 2048 per side and 1,048,576 tiles; minimum room edge 3; both map sides must fit four minimum edges; 1–1024 target rooms; up to 4096 extra corridor attempts |
| Room count | BSP stops at the target or geometric limit; it can produce fewer rooms than requested |
| Terrain | Nonzero dimensions, at most 2048 per side and 1,048,576 samples; finite frequency 0–1024; 1–16 octaves; finite sea level 0–1 |
| Population | Base level 1–80; 0–16 packs per normal room; density 0–100 attempts per 100 terrain samples; at most 100,000 potential actors |

These are generator resource bounds, not server capacity promises. Population budget rejection is conservative and includes possible ocean attempts and the requested maximum room count. Terrain attempts are `floor(samples * density / 100)`; zero density makes no overworld attempts. Zero packs suppresses normal-room packs; boss/adds, treasure guards and entrance NPCs retain their separate rules. Levels saturate at 80. Overworld scatter skips ocean but does not guarantee a requested actor count or unique placement.

Dungeon partitioning carves rooms and connects them with a spanning chain plus optional loop attempts. FIFO breadth-first traversal assigns shortest room-graph distances. The entrance is the room whose center is nearest an outer map edge; this does not create an exterior doorway. A multi-room dungeon assigns one distinct farthest boss room. A single room remains the safe entrance, `boss_room == entrance`, and produces no boss. Consumers must inspect archetypes rather than interpret that index as an unconditional boss request.

Doors mark exterior corridor cells sharing a cardinal edge with a room. They are walkable markers, not implemented door objects or collision barriers. Tile connectivity and the abstract room graph are different representations: a corridor may cross another room or corridor and create additional physical shortcuts. Encounter depth is based on the declared room graph, not measured tile distance.

## Provenance and replay

The repair changes the generator identity to `procgen-v2`. Version 1 outputs and hashes are not interchangeable with version 2. Retain old persisted artifacts with their original version; there is no implicit migration or old-version regeneration support.

The v2 manifest hashes the following in fixed order:

1. Manifest format marker, generator version and every field of `ProcgenSpec`, including seed.
2. Items in output order: identity, name, kind, rarity, level, ordered stats and complete ordered part definitions (ID, slot, name, weight, minimum rarity and bonuses).
3. Dungeon dimensions, tiles, rooms (ID, bounds, archetype and ordered connections), entrance and boss index.
4. Terrain dimensions, height samples, moisture samples and biomes.
5. Spawns: coordinate domain, position, kind with enemy archetype/boss flag or NPC role, level and encounter ID.

The encoding uses explicit enum tags, little-endian u32/u64 numbers, u64 lengths for strings/sequences/indices, UTF-8 string bytes and f32 IEEE bit patterns. Sequence order is significant; this is not an order-insensitive set hash. FNV-1a is a 64-bit change fingerprint, not a cryptographic integrity or authorization mechanism. Hash equality alone cannot establish artifact equality.

Persist the complete request, version, artifacts and fingerprint together. The native `ProcgenOutput` contains seed/version; `artifact::Candidate` adds the retained request, owned definitions and durable encoding. Static part references are restored only against the supported full catalog; raw pointers are never persistence identities. A consumer must pin build/catalog identity and establish cross-build/platform float reproducibility before claiming portable replay. A manual generator version bump remains required whenever output algorithms, built-in pools or its canonical manifest encoding change; the candidate and planning encodings carry independent schema versions.

The RNG forks from immutable master seed and domain without consuming master history. Changing item count therefore does not advance dungeon/terrain/population streams. Population intentionally depends on generated dungeon and terrain data; changing geometry may change actors. Dungeon and overworld population currently share one stream and encounter-ID sequence. No per-actor or per-domain population edit stability is promised.

## Owned artifact boundary

The source now includes owned candidate capture, strict bounded JSON and supported-build/catalog restoration with complete manifest verification. See [artifact-api.md](artifact-api.md) for the implemented API and evidence limits. Serde and serde_json reuse dependencies already present in the workspace lock; the crate remains independent of Bevy, MPQ and any running WM/server. Compilation and roundtrip acceptance are pending.

## Production consumer seams — not accepted at runtime

| Data | Next consumer responsibility | Acceptance needed |
|---|---|---|
| Generated items | Experimental authoritative item-ID/template allocation, owned persistence schema, validation and existing inventory/tooltip projection | Server-approved item survives reconnect and displays its actual stats; no client-only authoritative loot |
| Dungeon rooms/graph | WM encounter layout on an existing supported map, explicit transform, asset/template mapping, collision/navigation validation | Real connected player can reach the intended encounters using existing server authority |
| Terrain field | Offline ADT authoring plus matching server collision/navigation assets and versioned deployment | Matching client/server geometry, traversability and rollback evidence |
| Spawns | Existing WM/server publisher with explicit creature/NPC mapping, placement checks and lifecycle ownership | Correct actor identities, world scope, placement and bounded cleanup |

`SpawnDomain::Dungeon` means generated dungeon tile coordinates; `Overworld` means terrain sample coordinates. Neither is a WoW world coordinate. Consumers must select a domain-specific origin, scale, height rule and map/world scope before publishing. Actor overlap, actual NPC services, item balance, quests and live building are not solved by these data types. Runtime arbitrary geometry remains an unresolved platform decision, not a capability delivered by this crate.

## Evidence limits

The original worker reported 17 passing tests before these repairs. That report does not validate v2. The repair was source reviewed only: no build, test, formatter, linter, import, service, renderer or game execution was performed. Existing test callers were mechanically adapted to `Result`; no test cases were added.

When separately authorized, checks should cover invalid/overflowing requests, actual door openings, shortest graph paths with loops, single-room safety, zero population controls, actor levels, complete manifest field sensitivity, repeated outputs and subsystem stream isolation. These are pending checks, not demonstrated behavior. Full stock-client replacement remains the release gate independently of this generator prototype.
