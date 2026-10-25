# Durable generated artifact handoff proposal

Date: 2026-10-26, Europe/Moscow. Author: Astra. Status: concrete design proposal for review, NOT an accepted schema, implemented codec, generated artifact or production publication. Scope: the current whole-slice `ProcgenSpec`/`ProcgenOutput` generator, not arbitrary direct item requests. ADR-0001 remains Proposed and is not approval of this separate persistence seam.

## Existing owners and selected approach

Inspected read-only in `D:/WOW/wm-project`:

| Existing owner | Actual source behavior | Consequence for this proposal |
|---|---|---|
| `src/wm/content/workbench.py::DraftEnvelope` | JSON `{kind, template, draft}` plus optional reserved_slot/notes; existing draft creation/publication paths | Generated designs must be adapted into accepted content drafts by that owner; do not impersonate a published item/spell |
| `src/wm/content/artifact.py::ManagedArtifact` / ArtifactRegistry | `wm.artifact.v1`, numeric WM-owned artifact ID, declared content kinds and lifecycle/metadata | A generator's local item ID is not this registry ID. No new `procgen` artifact kind or registry write is authorized here |
| `src/wm/world/bundle.py::run_bundle` | `wm.world.bundle.v1`, ordered typed steps, existing native/content owners, live confirmation, earlier results referenced by from_step/field | Reuse this execution boundary after validation/authorization; do not implement a new runner or arbitrary SQL/GM path |
| Same bundle runner | Explicitly returns complete/partial/failed; earlier applied steps remain after later failure | Handoff must preserve per-step receipts and compensation/recovery identity; no atomicity claim |
| `src/wm/world/tools.py::execute_action` | Existing action-kind lookup, payload validation, finite exact placement, ControlScene/SceneStep and coordinator; native receipt/world_result returned | Domain conversion and permissions must be validated before dispatch. Even existing `world_spawn_create` is source-marked Partial, not live-proven |

Use a bounded JSON candidate artifact compatible with WM's existing data conventions, then a separately authorized WM adapter into these owners. Do not create a binary-only parallel pipeline, publish directly from Rust, or pretend the generator output already matches a live bundle. No new serialization dependency was added to implement a design that has not yet been accepted.

## Proposed envelope

Proposed schema identifier: `project-b.procgen.candidate.v1`. Generator identity remains `procgen-v2`; codec/schema version and generator version are separate. Names below are proposed interface names, not existing APIs.

| Field | Type and rule |
|---|---|
| schema_version | Exact candidate schema string above; unknown versions refused |
| generator_version | Exact supported generator version, initially procgen-v2; never relabel old output |
| source_identity | Object containing repository identity, full source commit/tree identity, generator source digest and declared clean/dirty state. Dirty/unavailable source can be retained as draft metadata but is not replay-ready |
| catalog_identity | Built-in catalog version plus content digest covering full pools, weights, names and affix tables; consumer must resolve an available pinned catalog/build. No fabricated digest value |
| request | Complete supported request, with seed as 16 lowercase hex digits, item count and every dungeon/terrain/population parameter |
| effective_item_request | Count equal to request.items and level 10 for this generator version. Makes the default explicit; a different direct item level needs a separately supported request form |
| output | Complete lossless artifact objects described below, without Rust references |
| manifest | `{algorithm: "fnv1a64-procgen-v2", value_hex: <16 lowercase hex digits>}`; recompute using the existing v2 field order, not JSON formatting |
| placement | Optional declared mappings/transforms for each used domain, separate from the generator fingerprint; absent/invalid mapping means NOT PUBLISHABLE, not guessed coordinates |

`output` has the exact keys `items`, `part_definitions`, `dungeon`, `terrain` and `spawns`. Each serialized item uses `part_ids` for its five ordered references into `output.part_definitions`. The current in-memory output's duplicate generator_version, seed and manifest_hash are represented by the envelope's generator_version, request.seed_hex and manifest respectively; reconstruction must use those values consistently, not accept a second conflicting identity. A future codec must spell out the remaining nested field names in its schema alongside these constraints before that schema is accepted.

No live token, credentials, player private data, raw SQL/GM/shell instructions or arbitrary execution hook belongs in this file. It is a candidate, not an authorization token. Transport/file SHA-256 may be retained by the existing candidate/evidence owner over the exact saved bytes; it is an out-of-band integrity receipt, not a circular field embedded in the bytes being hashed. Trust and permission still come from existing owners.

## Lossless JSON representation

The durable request/output representation uses exact integer and float semantics across Rust/Python and potential browser consumers. Encode u64 seed/generated item ID/FNV values as fixed 16-digit hex strings, never lossy JSON floating numbers. Encode each f32 as eight lowercase hex digits of IEEE bits, including request frequency/sea level, terrain arrays and item/part values. Reject NaN/Inf when rebuilding validated values. Preserve negative zero if present; do not silently normalize values covered by the fingerprint. Strings are UTF-8. Arrays retain order; object key order/whitespace do not affect the procgen fingerprint.

| Object | Required contents and constraints |
|---|---|
| Request | seed_hex; items; dungeon width/height/target_rooms/min_room/extra_loops; terrain width/height/frequency_f32_bits/octaves/sea_level_f32_bits; spawns base_level/packs_per_room/terrain_density_per_100 |
| Item | id_hex, name, explicit kind/rarity tags, level, ordered unique stat/value-bit pairs, exactly five ordered part references (body/grip/core/adornment/material) |
| Part catalog slice | Owned records for every referenced part: ID, slot, name, weight, minimum rarity, ordered stat/bonus bits. Duplicate part definitions must agree exactly; no deserialized static references, pointer identity or leaked allocations |
| Dungeon | Width/height; row-major tile tags; rooms in output order with ID, x/y/w/h, archetype and connection IDs in existing order; entrance and boss_room indices |
| Terrain | Width/height; row-major height/moisture bit arrays and explicit biome tags, each exactly width*height entries |
| Spawn | Explicit domain tag (Dungeon/Overworld), integer source coordinates, Enemy with archetype/boss or Npc with role, level and encounter ID |

The item array stores part IDs referring to the owned catalog slice; a decoder reconstructs the exact ordered part definitions before recomputing the existing fingerprint. Used part definitions alone do not identify every unused RNG pool: source_identity/catalog_identity must pin the full generator/catalog for regeneration. An importer may retain owned data for review without executing the generator. Rehydrating into the current `Vec<&'static Part>` output type is permitted only against a supported pinned built-in catalog with an exact definition match; no `Box::leak` shortcut.

Explicit enum strings have a schema-owned mapping to the existing v2 fingerprint tags. Initial sets are exactly the enums in `item`, `dungeon`, `terrain` and `spawns`; unknown tags fail the schema version rather than becoming a default. Native WM/server IDs are absent from generated item identity until an existing content owner allocates a managed slot.

## Validation and resource contract

Proposed transport limit: 64 MiB UTF-8 JSON, nesting depth at most 16, strict rejection of duplicate object keys and unsupported fields for v1. A valid generation request whose serialized candidate exceeds this limit must get an explicit size error; never truncate its arrays or silently lower the request. An accepted future implementation must use bounded decoding/sequence visitors, not deserialize unbounded Vec/String values and validate only afterward.

| Data | Proposed preallocation bound |
|---|---|
| Items | ≤10,000; name ≤512 UTF-8 bytes; stats ≤8 unique StatId values; five parts per item |
| Owned parts | ≤50,000 distinct referenced IDs (five per possible item); ID ≤128 bytes, name ≤512 bytes; bonuses ≤8 per part; body/slot/rarity/reference validity checked |
| Dungeon | Sides ≤2048, total tiles ≤1,048,576; rooms ≤1024; per-room connections ≤1023; total adjacency entries ≤10,238 (chain plus at most 4096 new undirected edges) |
| Terrain | Nonzero sides ≤2048, samples ≤1,048,576; all three arrays have the exact count |
| Spawns | ≤100,000; archetype ≤128 UTF-8 bytes; valid domain coordinates, level 1–80; encounter values preserve grouping, no arbitrary rewrite |
| Metadata | Repository/build/catalog locator strings individually bounded at 1024 bytes; no unlimited notes/extensions bag |

Validation order is part of the contract:

1. Reject transport/nesting/version violations before constructing large artifacts. Validate request scalar types/ranges and checked products. Booleans are not integers. A decoder must enforce aggregate limits while reading arrays/strings, even when a malicious sender lies about dimensions.
2. Check every array length, reference, enum and finite value. Require room.id to match output index, checked rectangle bounds, no self/duplicate/out-of-range links, graph symmetry and reachability, valid entrance/boss/archetype counts, and exact domain dimensions. Validate before calling current helpers that assume coherent data.
3. Verify generated geometric consistency: room cells/centers must be walkable, actor positions lie in their declared domain and eligible dungeon room, single-room layouts have no boss, entrance has no dungeon enemies, multi-room layouts have the one boss in its boss room. Preserve existing overlapping actor positions as declared candidate data; publishing needs a separate placement/collision policy, not silent re-rolling.
4. Check output identity against request/version/catalog, recompute the complete v2 fingerprint and compare. Accept matching content only as a validated candidate. A matching FNV value does not authenticate the origin or make an arbitrary artifact authoritative.
5. Resolve placement/template/service/permission/cleanup requirements through the existing WM content owners. An unresolved mapping or unverified navigation surface blocks publication; it does not invalidate otherwise retainable generator data.

The exact implementation must distinguish errors such as unsupported_schema, unsupported_generator, unresolved_catalog, size_limit, invalid_request, invalid_shape, invalid_reference, non_finite_value, fingerprint_mismatch and placement_not_ready. These are proposed categories, not currently implemented return variants. Import failure must not mutate an existing accepted candidate, allocate live IDs, write DB rows or leave partial applied content.

## Placement and publication handoff

Each used domain needs an explicit transform: map/world/instance target, origin, scale, orientation, height authority and bounds, plus generator artifact identity. Dungeon tile coordinates and Overworld sample coordinates cannot be compared or combined until each is transformed. Current terrain height is normalized data, not WoW elevation; current dungeon tiles have no server navigation proof.

Template mapping translates enemy archetypes/NPC roles into approved existing template or WM-managed content identities. Vendor/questgiver roles require actual configured services and visible content; a role enum does not create them. Generated item kinds/stats need a content-owner mapping for class/subclass, display/icon, tooltip and reserved template fields. No stock named quest creature or stock spell is silently repurposed as a permanent WM carrier.

Materialization uses existing DraftEnvelope/content allocation and the validated typed action catalog. Native actions return receipt/world_result identities; persist those actual IDs with the originating candidate, transformation revision and run key. Do not derive spawn IDs from generator-local IDs or assume an allocated slot is already active. Evidence must include server/content revision, placement validation and any permission refusal.

For ordered `wm.world.bundle.v1` publication, record per-step results. A later failure leaves earlier applied steps present. Compensation must use those actual returned identities through existing typed despawn/delete/retire owners, with explicit authorization and lifecycle status. Never claim all-or-nothing rollback from this runner, blindly replay a whole partial bundle, or mark an observed dirty visible ID reusable. Existing WM retirement/replace-on-new-ID rules apply.

Client archive/UI publication is a separate consumer. This proposal neither creates ADT/WMO geometry nor supplies matching server maps/vmaps/mmaps. Arbitrary runtime geometry and native restart exceptions remain open owner decisions.

## Library and consumer implementation sequence

The following is a concrete proposed API boundary, not code already landed:

1. `capture_candidate(request, generated_output, supplied_provenance)` retains the full request and owned artifact definitions, checks shape/identity and computes canonical data/fingerprint without running WM, reading Git, opening sockets or writing files.
2. `encode_candidate` produces bounded JSON bytes or a size error; caller owns any save path. `decode_candidate` performs bounded validation and returns owned candidate data or an error; it never publishes or regenerates implicitly.
3. A supported-catalog adapter may expose current in-memory generator types after exact catalog matching. Until then, owned durable data remains a distinct candidate representation, not fake static Rust references.
4. A WM adapter, owned separately by OC/WM owners, validates placement/templates and builds existing content/bundle drafts. It must obtain execution authority before even a purported dry run that touches DB/services. No new mutation bus is introduced.

Use the existing project's JSON serialization conventions. A codec implementation would need a reviewed bounded parser/serializer choice and dependency declaration if current crate dependencies cannot supply it. This proposal does not approve a new dependency merely for convenience, a hand-written unchecked JSON parser, or a new content runner. The current source repair implements only the settled RNG arithmetic fix; no unsettled persistence API was silently added.

## Versioning and acceptance

Existing v1/v2 generator outputs without a complete request/catalog identity remain historical artifacts; never manufacture missing provenance from a seed. Candidate schema v1 initially supports generator procgen-v2 only. Unknown future versions must be retained externally for an appropriate tool or rejected explicitly, not decoded with guessed defaults. Source-level non-output RNG overflow repair does not change valid v2 request streams; output/canonical-encoding changes still require a generator/encoding revision.

When implementation and execution are separately authorized, evidence must cover exact encode/decode fidelity, incomplete/truncated/oversize/duplicate-key/reference refusal, all manifest field sensitivity, catalog/source mismatch, float-bit and u64 preservation across Rust/Python, no partial mutation on import failure, and existing-owner partial publication/compensation. File existence, source review or generated tiles do not prove roundtrip, live authority or navigability.

This design deliverable is complete enough for a codec/consumer review: fields, identities, limits, validation order, ownership, migration and failure behavior are explicit. Schema/dependency acceptance, codec implementation and all execution acceptance remain open. See [the invariant audit](invariant-audit.md), [current generator contract](design.md) and the behavior inventory for the independent baseline client blockers.
