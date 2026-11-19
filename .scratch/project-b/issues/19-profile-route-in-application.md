# 19 — Render route terrain from a selected manifest profile

**Status:** ready-for-agent

**Plan reference:** C04. Owner approved ticket sizes and blockers on 2026-10-25. See [the approved completion queue](../completion-index.md).

**Implementation state:** Acceptance open — existing code may cover part of this slice; inspect the current checkpoint before starting.

**What to build:** The application's asset path renders the fixed route terrain from the selected profile and explains exactly which archive supplied each required asset.

**Blocked by:** None — can start immediately.

**Related existing issues:** [05](05-b004-route-assets-provenance-loader.md). These are traceability links, not additional blocking edges; existing issues and evidence are preserved.

**Suggested owner:** OpenCode in a safely synchronized client checkout.

## Acceptance

- [ ] Pin the route start and boundary, required tiles/neighbors, WMO/doodad dependencies and relevant DBC/texture data from actual samples.
- [ ] Connect the existing manifest/profile implementation to the application's real asset-loading path; a separate file-listing CLI is insufficient.
- [ ] Preserve owner disabled toggles and read-only source install behavior. Profile selection is explicit; no hidden discovery/reordering.
- [ ] Render terrain from the base profile with correct scale/orientation; identify owner-profile differences through provenance.
- [ ] Missing or invalid dependencies yield actionable diagnostics and do not replace a working active profile with partial state.
- [ ] Record unresolved model/liquid dependencies as the blockers of C05/C06, without claiming the complete route gate.

## Boundaries and handoff

Do not rewrite MPQ reading when current sample evidence supports it. Do not broaden to a new asset pipeline unless the real integration exposes a specific gap.

Keep the slice within one fresh implementation context. If exploration reveals additional independent behavior, identify the smallest complete child slices before expanding scope. Read the current project context, relevant skill/rules and source checkpoint. Preserve unrelated edits and existing ownership.

Execution checks described above are acceptance requirements, not authorization. Follow explicit permissions applicable to the executing agent. Report source implementation, compilation and runtime evidence separately. An unexecuted required check remains UNKNOWN. Do not commit, deploy, start services or mutate live state merely because this published ticket exists.

Handoff: exact source identity and changed paths; externally visible behavior; evidence actually collected; unresolved acceptance; next dependent slice. Close only after acceptance is supported. See [the completion plan](D:/WOW/PROJECT_B_COMPLETION_PLAN.md) for the mandatory full stock-client replacement release gate.

## Source gap list (2026-11-21, OpenCode swarm)

Method: read the issue, AGENTS.md, docs/agents/sync.md, completion-index.md, issues 05/20/21,
docs/profiles/base.mpqlist + wm-hd.mpqlist, checkpoint .scratch/project-b/checkpoints/2026-10-26-oc-route-assets.md,
handoffs 2026-10-28-oc-session-handoff.md and 2026-11-02-completion-matrix.md, and the owning
source: benilla-assets/src/{profile_source.rs,lib.rs,adt.rs,coords.rs,wdt.rs,m2.rs},
benilla-profile/src/{lib.rs,main.rs,classify.rs}, benilla-app/src/{lib.rs,no_install.rs,entities.rs,
ui_script/lifecycle.rs}, benilla-world/src/{assets/mod.rs,worldview.rs,terrain_stream.rs,
terrain_stream/window.rs,liquid/mod.rs,liquid/surface.rs}, benilla-formats/src/{terrain.rs,install.rs},
benilla-wdt/src/lib.rs, benilla-assets/examples/glue_profile.rs. No Cargo, test, client or server run.

| Acceptance line | Source that meets it | Gap |
|---|---|---|
| Pin the route start and boundary, required tiles/neighbors, WMO/doodad dependencies and relevant DBC/texture data from actual samples | `crates/benilla-world/src/terrain_stream.rs:405` `SPAWN_XY` (route start); `crates/benilla-world/src/terrain_stream/window.rs:33-110` `StreamWindow::at`/`wanted_tiles`/`inner_tiles` (neighbor ring); `crates/benilla-world/src/terrain_stream.rs:706` WDT `has_tile` gate; `crates/benilla-assets/src/adt.rs:130-195` `AdtLoader::load` carrying MWMO/MODF + MDDF placements via `crates/benilla-formats/src/terrain.rs:433-472`; `crates/benilla-assets/src/lib.rs:212-215` `MapCatalogRes` from `Map.dbc` | Partial: the start constant, dynamic neighbor ring and placement parsers exist, but the sampled pin set (tile (32,48), the checkpoint's exact WMO/M2/DBC/texture list and profile hashes) lives only in `.scratch/project-b/checkpoints/2026-10-26-oc-route-assets.md`; no executable pin fails if the route's data moves or a dependency silently drops, and the "boundary" is farclip-derived, not the sampled extent |
| Connect the existing manifest/profile implementation to the application's real asset-loading path; a separate file-listing CLI is insufficient | `crates/benilla-app/src/lib.rs:366-381` boot registers `register_profile_source` (`crates/benilla-assets/src/profile_source.rs:692-712`) when `WM_ASSET_MANIFEST` is set; `crates/benilla-world/src/assets/mod.rs:66-77` `open_world_assets` opens `open_profile_chain`; `crates/benilla-world/src/worldview.rs:78-90` same for the viewer; the file-listing CLIs (`crates/benilla-profile/src/main.rs`, `crates/benilla-assets/examples/glue_profile.rs`) are separate and not load-bearing | none |
| Preserve owner disabled toggles and read-only source install behavior. Profile selection is explicit; no hidden discovery/reordering | `crates/benilla-profile/src/lib.rs:96-117` `Profile::open_archives`: absent or `.disabled` archive is a hard `bail!` naming the file; manifest line order is precedence; crate doc `lib.rs:1-10` never writes/renames/deletes in `Data`; explicit env selection and "One registration, never both" at `crates/benilla-app/src/lib.rs:366-378` | none |
| Render terrain from the base profile with correct scale/orientation; identify owner-profile differences through provenance | `crates/benilla-assets/src/coords.rs:13-46` `wow_to_bevy`/`placement_rotation` (det +1, 1 unit = 1 yard); `crates/benilla-formats/src/terrain.rs:22-24` `TILE_SIZE`/`CHUNK_SIZE`; `crates/benilla-assets/src/adt.rs:71-127` mesh build in Bevy space; base default at `crates/benilla-app/src/lib.rs:372`; provenance primitives `crates/benilla-profile/src/lib.rs:148-154,208-211` and `crates/benilla-profile/src/classify.rs:34-58`, used by `crates/benilla-profile/src/main.rs:88-103` | Partial: scale/orientation math and base-profile default are in source, but owner-profile differences are only identifiable through the out-of-app `profile-probe classify` CLI; the app emits no per-asset provenance (the only in-app `provenance()` consumer is the UI-reload log, `crates/benilla-app/src/ui_script/lifecycle.rs:1354`), and rendered correctness is visible behavior source cannot prove |
| Missing or invalid dependencies yield actionable diagnostics and do not replace a working active profile with partial state | `crates/benilla-assets/src/profile_source.rs:144-172` `ContentRevisionError` names input/class/archive; `:369-405` stage off to the side; `:417-447` base-serial commit guard; `:560-583` no-regression `validate_input`; boot failures exit via `crates/benilla-app/src/lib.rs:382-385` + `crates/benilla-app/src/no_install.rs:38-48`; `crates/benilla-profile/src/lib.rs:99-117`; UI-reload refusal keeps the live VM/profile at `crates/benilla-app/src/ui_script/lifecycle.rs:1277-1285,1352-1370` | Partial: the reload seam is guarded, but the world path never validates the route's required inputs at boot — `stage_content_revision` is called only from the UI reload with `RELOAD_CONTENT_INPUTS` (UI files, `lifecycle.rs:1414`); a profile that opens but lacks the route ADT/WMO streams an empty world, and `terrain_stream.rs:706-722` warns only for an off-grid focus, not for a WDT-authored tile the profile cannot serve |
| Record unresolved model/liquid dependencies as the blockers of C05/C06, without claiming the complete route gate | none found for a C05/C06 blocker record. Nearest: the stale OPEN-children list in `.scratch/project-b/issues/05-b004-route-assets-provenance-loader.md` (external skin since implemented at `crates/benilla-assets/src/m2.rs:272-283`; MH2O meshed at `crates/benilla-formats/src/terrain.rs:460-472`); data-gated skip pattern `crates/benilla-formats/src/install.rs:165` `skipped` + `crates/benilla-app/src/entities.rs:1335-1343` | No current record names which route model/liquid dependencies are unresolved (no M2 version census of the wm-hd archives, no per-route-tile MH2O/MCLQ coverage list) and nothing ties them to C05/C06; issue 05's OPEN list is stale against HEAD |

### Tickets opened by this survey

1. `oc/route-pins-executable` — encode the sampled Northshire route pins as executable checks: a
   test resolves `StreamWindow::at` at `SPAWN_XY` and asserts tile (32,48) plus its 8 neighbours are
   in `wanted_tiles()`, and a fixture of the checkpoint's MWMO/MDDF/DBC/texture dependency paths gets
   a data-gated test (`benilla_formats::skipped` without `WM_ASSET_MANIFEST`) asserting each resolves
   through `open_profile_chain` for `base`. Files: `crates/benilla-world/src/terrain_stream.rs` (or a new
   `route_pins.rs`) plus one fixtures module. Tests: `route_start_tile_and_neighbours_are_wanted_at_spawn`,
   `route_dependency_fixture_resolves_in_base_profile`.
2. `oc/profile-provenance-report` — emit a boot-time provenance line per required route input: the
   world asset open resolves each required path through the profile and logs the winning archive,
   listing missing inputs as missing. Files: `crates/benilla-world/src/assets/mod.rs`,
   `crates/benilla-assets/src/profile_source.rs` (expose a required-inputs report). Tests:
   `a_required_input_report_names_each_winning_archive`,
   `a_missing_route_input_is_listed_as_missing_not_dropped`.
3. `oc/c05-c06-blocker-record` — refresh issue 05's stale OPEN children against HEAD (external skin
   and MH2O meshed) and record the still-unresolved route model/liquid dependencies — M2 version
   census of the wm-hd archives, per-route-tile MH2O/MCLQ coverage, external-skin presence per route
   model — as the C05/C06 blockers in issue 19's note and the completion index, without claiming the
   route gate. Doc-only; no Cargo run; acceptance is the three named deps each carrying an evidence
   pointer and the C05/C06 rows referencing the same record.

`Blocked: needs owner run` — acceptance line 4's visible half (terrain rendered from the base
profile at correct scale/orientation on screen, and the expected base-vs-owner visual deltas) and
line 2's end-to-end boot on the owner's install: source proves the wiring and the transform math,
but only a live client judges pixels, and only the owner launches it (AGENTS.md).



