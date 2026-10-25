# ACTIVE FILE CLAIM: Astra — finish interrupted procgen repairs

User explicitly reassigned GLM's unfinished procgen work to Astra. Base HEAD at claim: `b21d0d92`. OpenCode retains `crates/benilla-app/src/wrath.rs` and all client/lifecycle work.

Claimed paths: `crates/benilla-procgen/src/{lib,dungeon,spawns,terrain,item}.rs`, new `docs/procgen/design.md`, and this checkpoint. GLM's dirty dungeon/lib/spawns edits are adopted and preserved where correct. Its untracked `tools/patch_procgen_dungeon.py` is left untouched and will not be executed or committed.

Scope: finish complete manifest coverage/versioning, FIFO distances, actual room-boundary doors, parameter rejection/bounded generation, single-room safety, zero-density behavior, necessary existing caller adaptations, and the missing integration document. No new consumers, deployments or live geometry support.

Source review only. No builds, tests, new test cases/files, linters, formatters, imports, probes, services or visual execution. Existing test call sites may need mechanical adaptation to the interrupted Result API; their assertions are not execution evidence. Commit cohesive repairs with explicit paths, release ownership when complete. Full stock-client replacement remains the release gate.
