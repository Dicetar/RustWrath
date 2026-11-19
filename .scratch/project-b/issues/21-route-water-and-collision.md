# 21 — Make route water and collision usable

**Status:** ready-for-agent

**Plan reference:** C06. Owner approved ticket sizes and blockers on 2026-10-25. See [the approved completion queue](../completion-index.md).

**Implementation state:** Acceptance open — existing code may cover part of this slice; inspect the current checkpoint before starting.

**What to build:** The selected route's terrain, water and required structures have coherent rendering and collision behavior in the application.

**Blocked by:** [19 — Render route terrain from a selected manifest profile](19-profile-route-in-application.md)

**Related existing issues:** [05](05-b004-route-assets-provenance-loader.md). These are traceability links, not additional blocking edges; existing issues and evidence are preserved.

**Suggested owner:** OpenCode.

## Acceptance

- [ ] Implement the route's observed MH2O data through existing liquid/rendering structures; do not treat missing legacy MCLQ as dry terrain.
- [ ] Pin required structure/doodad dependencies and use existing collision owners.
- [ ] Demonstrate correct water extent/height and transitions for the selected route, plus no falls through required ground/structures.
- [ ] Check both the required base profile and material owner-profile overrides where they affect collision/visibility.
- [ ] Record remaining unsupported liquid/material or structure variants in C03's ledger.

## Boundaries and handoff

This slice closes the chosen route only. Dynamic building navigation is a separate capability and must not be implied by route collision.

Keep the slice within one fresh implementation context. If exploration reveals additional independent behavior, identify the smallest complete child slices before expanding scope. Read the current project context, relevant skill/rules and source checkpoint. Preserve unrelated edits and existing ownership.

Execution checks described above are acceptance requirements, not authorization. Follow explicit permissions applicable to the executing agent. Report source implementation, compilation and runtime evidence separately. An unexecuted required check remains UNKNOWN. Do not commit, deploy, start services or mutate live state merely because this published ticket exists.

Handoff: exact source identity and changed paths; externally visible behavior; evidence actually collected; unresolved acceptance; next dependent slice. Close only after acceptance is supported. See [the completion plan](D:/WOW/PROJECT_B_COMPLETION_PLAN.md) for the mandatory full stock-client replacement release gate.

## Source gap list (2026-11-21, OpenCode swarm)

Method: read the issue and the route-asset pins (`.scratch/project-b/checkpoints/2026-10-26-oc-route-assets.md`, `docs/baseline/route-assets.md`, the 2026-10-28 first-alpha MH2O repair, the 2026-10-31 Firna baseline and the 2026-11-15 Wrath handoffs), then the owning source: `benilla-adt/src/lib.rs` (MCNK/MH2O), `benilla-formats/src/{terrain,liquid,sound_water,lib}.rs`, `benilla-world/src/liquid/{mod,query,surface,spatial,real_data}.rs`, `benilla-world/src/terrain_stream{,.rs,/collider.rs,/spawn/mod.rs}`, `benilla-world/src/{collision,surface,world_point}.rs`, `benilla-app/src/player/{controller,swim,mover}.rs`, `benilla-app/src/sound/{footsteps,liquid_loop}.rs`, `benilla-app/src/ui_mirror.rs` and `benilla-assets/src/{world_assets/images.rs,profile_source.rs}`. Read-only reference check at `D:/Projects/wow/reference/wowdecomp-reference` (pin e67b6ffe) for the LiquidSystem/MSDF facts already quoted in the source. No Cargo, test, client or server run.

| Acceptance line | Source that meets it | Gap |
|---|---|---|
| Implement the route's observed MH2O data through existing liquid/rendering structures; do not treat missing legacy MCLQ as dry terrain | `crates/benilla-adt/src/lib.rs:203` `read_mh2o` (all four `lvf` layouts); `crates/benilla-formats/src/terrain.rs:460` appends MH2O meshes to each chunk's `liquids`; `crates/benilla-formats/src/liquid.rs:260` `build_mh2o_meshes`/`:278` `build_mh2o_instance`; spawned by `crates/benilla-world/src/terrain_stream.rs:1020` `spawn_liquids` into `WaterChunkInfo`/`LiquidSurface`; "Elwynn liquid = MH2O only, 0 MCLQ" is pinned in `docs/baseline/route-assets.md:24` and the array-layout repair is evidenced in `checkpoints/2026-10-28-oc-first-alpha.md` | none in source for the base profile (id 5 "Basic Procedural Water" resolves to `Still`). A live run is the only proof of the rendered extent, so the rendered claim is owner-run. |
| Pin required structure/doodad dependencies and use existing collision owners | WMO set pinned in `checkpoints/2026-10-26-oc-route-assets.md` (NSABBEY + ABBEYGATE/WIDEBRIDGE); placement parse `crates/benilla-adt/src/lib.rs:476` `read_modf` (doodad_set/name_set) and `crates/benilla-formats/src/terrain.rs:504`; hulls via `crates/benilla-world/src/terrain_stream/spawn/mod.rs:250` and `:502` `placement_collider_data`; props `spawn/mod.rs:782` `resolve_wmo_doodads`; layers/epoch `crates/benilla-world/src/collision.rs:29` `CollisionLayer`, `:63` `ColliderEpoch`; fences `terrain_stream/collider.rs:287` | WMO-only doodad sets resolve from MODF `doodad_set` (`spawn/mod.rs:784`), but an ADT MDDF doodad placement carries no set field at all (`benilla-adt` `DoodadPlacement`; `terrain_stream.rs:1177` hardcodes `doodad_set: 0`), so set-variant route doodads cannot be selected. See ticket 1. |
| Demonstrate correct water extent/height and transitions for the selected route, plus no falls through required ground/structures | height/extent queries `crates/benilla-world/src/liquid/query.rs:333` `surface_z_at` + `:158` `WaterChunkInfo` (bilinear); submersion `crates/benilla-world/src/liquid/query.rs:609` `submersion_claim_at`/`crates/benilla-world/src/liquid/query.rs:684` `detect_submersion`; swim/wade `crates/benilla-app/src/player/swim.rs:75` `surface_over_feet`, `:87` `update_swimming`; one-way ground `crates/benilla-world/src/terrain_stream/collider.rs:246` `terrain_collider_data`, `collider.rs:287` `impassable_wall_data`; terrain trimesh `terrain_stream/spawn/mod.rs:502`; edge gate `crates/benilla-app/src/player/edge.rs` `may_resolve` + `player/mover.rs:120`; focused `terrain_stream/collider.rs:389` and `player/mover.rs:1074` | `Blocked: needs owner run`. The extent/height/transitions and "no fall through required ground or structures" are visible-client facts; source tests are UNKNOWN for them. |
| Check both the required base profile and material owner-profile overrides where they affect collision/visibility | profile selection `crates/benilla-world/src/assets/mod.rs:72` `open_profile_chain` (and `benilla-app/src/lib.rs:372`), `crates/benilla-assets/src/profile_source.rs:670`; ADT bytes identical in both profiles per `docs/baseline/route-assets.md`; NaN-doodad guard `terrain_stream/spawn/mod.rs:155`; no-liquid lever `liquid/mod.rs:103`; real-data coverage `liquid/real_data.rs:147` | `Blocked: needs owner run`. Nothing in source selects or A/B-compares the two profiles for collision/visibility; the route pins are base-profile facts. |
| Record remaining unsupported liquid/material or structure variants in C03's ledger | C03 is issue 18 (`issues/18-stock-replacement-inventory.md` + `architecture-traceability.md:20` C06 row) | Three variants have no ledger row: `LiquidKind::Rapids` is defined and keyed in `benilla-formats/src/liquid.rs:61` but never authored by any shipped MCLQ/MH2O path (`:40`, `:258`); the 3.3.5 `LiquidType.dbc` is not read at all, so `from_mh2o_liquid_type` is a hand-kept id table and unknown ids silently stay dry (`:58`); ADT MDDF doodad-set selection is missing (row above). See ticket 1. |

### Tickets opened by this survey

1. `oc/mh2o-liquid-type-dbc` — Read the route's `LiquidType.dbc` and resolve an MH2O instance's kind and loop kit from its row instead of the hand-kept id table in `from_mh2o_liquid_type`. Acceptance: `LiquidType.dbc` is loaded through the existing profile `Chain`; an MH2O instance whose id has a row takes its kind from that row; an id with no row, or a row that maps to no render path, is named in a `warn!` and leaves the cell dry (never a guessed surface). Files: `crates/benilla-formats/src/liquid.rs` (the mapping) and a new `crates/benilla-formats/src/liquid_type.rs` (schema + catalog), re-exported from `lib.rs`. Tests: `liquid_type_catalog_resolves_the_route_water_row`, `an_unknown_liquid_type_id_is_reported_not_guessed`.
2. `oc/mh2o-unsupported-id-diagnostic` — Name every unsupported MH2O instance instead of dropping it silently: `build_mh2o_instance` returns `None` for an unknown `liquid_type`, a `width`/`height` of 0, an empty `exists` bitmap and a missing vertex array alike. Acceptance: each of those four returns is logged once per ADT with its grid index and the field that failed; a supported control instance builds and logs nothing. Files: `crates/benilla-formats/src/liquid.rs` only. Tests: `an_unknown_liquid_type_id_is_reported_once_per_adt`, `a_supported_instance_logs_nothing`.
3. `oc/exterior-group-terrain-type` — Let the footstep/footprint surface query read an exterior WMO group's `MOPY.material_id → MOMT+0x20` `TerrainType`, which `crates/benilla-world/src/surface.rs:13` names as a known deviation: "the claim covers interior groups only, so a surface on an exterior group (bridges and docks, `Wood`) still takes the terrain leg, unlike the reference". Acceptance: standing on a bridge/dock exterior group yields that group's `TerrainType` id, not the MCNK layer's; an interior group's answer is unchanged; off any placement the terrain leg still answers. Files: `crates/benilla-world/src/surface.rs` and the claim in `crates/benilla-world/src/wmo_portal/interior.rs`. Tests: `an_exterior_group_floor_reports_its_own_terrain_type`, `an_interior_floor_answer_is_unchanged`.

Not ticketed: the rendered water extent/height/transitions and the no-fall-through proof are the live checks above, and the "never authored" `LiquidKind::Rapids` row (`benilla-formats/src/liquid.rs:34`, `:258`) is a ledger entry, not a code gap — no shipped MCLQ or MH2O cell carries nibble 8, so no client path can reach it.
