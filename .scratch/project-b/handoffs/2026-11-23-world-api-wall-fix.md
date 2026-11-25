# Close world API leaks from the benilla-dev split

Status: ready to merge
Owner: ClaudeJr / coordinator
Branch: jr/world-api-wall-fix
Issue: none
Next: merge origin/main and push this repair to main once; read the resulting CI run

## Request

Repair main's failing `benilla-app/tests/world_api_wall.rs::the_world_api_doorway_stays_shut` (131 leaks, ceiling 111) caused by the benilla-dev split. Do not raise the ceiling. Fix a new `created_guid` lint only if confirmed in this CI.

## State and evidence

- Based on `origin/main` at `5d874e82f`; failing baseline CI was 45047780531.
- The split added `benilla-dev` to `GAME_CRATES`, so its instrument-only engine references were counted as game API leaks. `INSTRUMENT_CONSUMERS` already defines that instrument boundary for app-side paths.
- Fixed `is_instrument_consumer` to classify `benilla-dev/` files as instrument consumers while retaining `benilla-dev` in `GAME_CRATES`; real references from `benilla-app`, `benilla-client` and `benilla-souls` remain measured.
- Local focused result after fix: `world API surface: 208 items — 108 leaks (ceiling 111, slack 10; 92 sorted, 16 unsorted), 100 published; + 51 named only by instruments`; passed.
- Local full target: `cargo test -p benilla-app --test world_api_wall` passed 4/4.
- `cargo fmt --all -- --check` and `git diff --check` passed.
- CI 45162170164 for the intervening main commit 5d874e82f: gates/windows green; no test-build/test ran because that commit changed only build configuration. The `created_guid` lint was not reproduced or changed.
- Not run in game/client/server. Owner's integration checkout was not edited or built.

## Next

Push the repair to `main` once. Then read its one CI run and record the result here.
