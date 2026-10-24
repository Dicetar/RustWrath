# 02 — B-001A: three extension seam maps

**What to build:** three one-page tables (movement, building, generated spaces) mapping each feature to existing owners: input/intent path, locomotion/collision owner, replication, persistence, WM observation, missing hook, restart need today. Movement owners to inspect: MovementHandler, WorldSession movement info, unit spline/granted-movement code. Building: wm_bridge spawn actions, GO templates, vmap/mmap pipeline (which GO types get server collision). Spaces: map/instance system, vmap/mmap generation path. Bounded: maps only, no designs, no implementation.

**Blocked by:** 01 (identity pairing).

**Status:** done 2026-10-24 — docs/baseline/seam-maps.md

- [x] Movement seam table with quantified "what does AC reject" for custom locomotion
- [x] Building seam table quantifying which placements get real server collision/navigation today
- [x] Generated-spaces seam table confirming vmap/mmap blocker and existing-map layout path
- [x] Each table lists at least one concrete missing hook and its restart need
