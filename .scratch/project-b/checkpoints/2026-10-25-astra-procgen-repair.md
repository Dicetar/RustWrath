# RELEASED FILE CLAIM: Astra — finish interrupted procgen repairs

User explicitly reassigned GLM's unfinished procgen work to Astra. Base HEAD at claim: `b21d0d92`. OpenCode retains `crates/benilla-app/src/wrath.rs` and all client/lifecycle work.

Claimed paths: `crates/benilla-procgen/src/{lib,dungeon,spawns,terrain,item}.rs`, new `docs/procgen/design.md`, and this checkpoint. GLM's dirty dungeon/lib/spawns edits are adopted and preserved where correct. Its untracked `tools/patch_procgen_dungeon.py` is left untouched and will not be executed or committed.

Scope: finish complete manifest coverage/versioning, FIFO distances, actual room-boundary doors, parameter rejection/bounded generation, single-room safety, zero-density behavior, necessary existing caller adaptations, and the missing integration document. No new consumers, deployments or live geometry support.

Source review only. No builds, tests, new test cases/files, linters, formatters, imports, probes, services or visual execution. Existing test call sites may need mechanical adaptation to the interrupted Result API; their assertions are not execution evidence. Commit cohesive repairs with explicit paths, release ownership when complete. Full stock-client replacement remains the release gate.

## Delivered in the commit containing this checkpoint

Status: SOURCE IMPLEMENTED; UNCOMPILED / UNTESTED. Claimed files are released after this delivery. Adopted GLM's FIFO distance traversals, fallible dungeon generation and safe single-room archetype handling. Finished its partial work without running its patch script.

- `procgen-v2` manifest covers every request/output field with explicit enum tags, numeric encoding and sequence/string lengths. Includes item/part provenance, dungeon rooms/graph, terrain moisture and full spawn metadata. FNV remains a change fingerprint, not authentication or collision-free identity.
- Door markers are exterior corridor cells sharing a cardinal edge with room floor; corner-only ring contacts are excluded. No door objects or route rendering are implemented.
- Whole-request validation precedes generation. Subsystem APIs return Result. Dimensions, arithmetic, loop/octave/item/actor budgets, finite terrain parameters and level ranges are bounded. Population also checks supplied artifact shapes/room indices and graph reachability before indexing.
- Zero overworld density yields zero scatter attempts; zero normal-room packs yields none. Enemy levels clamp to 80. One-room layouts retain entrance NPCs and have no boss. Original output algorithms and data remain otherwise intact.
- Existing test call sites were adapted to fallible returns, preserving assertions; no test cases/files added or executed. Source search found no production consumers to migrate.
- Added `docs/procgen/design.md` with exact current API, resource bounds, manifest/replay contract, proposed consumer seams and explicit missing integration. Runtime arbitrary geometry remains unresolved.

Source review checked Result propagation, all changed callers, arithmetic bounds, FIFO traversal, door indexing and manifest coverage against each public data struct. No compilation, test pass, real output, cross-platform determinism or live behavior is claimed. Pending checks are listed in the design document. GLM's original passing-test report predates these repairs.

Preserved OC's dirty `wrath.rs`, event-scope/session-lifecycle checkpoints and the untracked GLM patch script. No shared client, bridge, WM, assets, dependency or server file was changed or staged by Astra. The unrelated files bundled into original commit `9a1e365e` were not amended or reverted.
