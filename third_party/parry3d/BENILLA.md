# benilla's `parry3d` fork — what differs from upstream, and how to check

Upstream: `dimforge/parry`, Apache-2.0, version `0.26.1` — the version the workspace lock
resolves to (avian3d 0.6.1 is the only consumer). Wired in through `[patch.crates-io]` in the
workspace root. The crate is copied from the cargo registry as published (`src/`, `Cargo.toml`,
`README.md`) plus `LICENSE`; the one patched file is LF, the rest are as published.

## Why a fork exists

`WOW_CAPTURE=water-noon` on the `wm-hd` profile panics while the terrain streamer attaches a
streamed tile's collider:

```
panicked at parry3d-0.26.1/src/partitioning/bvh/bvh_binned_build.rs:58:28:
index out of bounds: the len is 8 but the index is 18446744073709551615
Encountered a panic in system `benilla_world::terrain_stream::spawn_streamed_tiles`!
```

The tile collider was 37,120 vertices / 65,520 valid triangles in draw space: finite, no
zero-area triangles, no subnormal vertices (`.scratch/project-b/checkpoints/
2026-10-26-parry-bvh-terrain-park.md`). The panic is a builder defect, not corrupt input:

- `rebuild_range_binned` picks the axis of maximum spread between leaf AABB centers and computes
  `k1 = NUM_BINS * (1 - BIN_EPSILON) / (range[1] - range[0])`.
- A nearly flat cluster — two leaf AABBs coincident to f32 precision, with the only center
  difference a subnormal in y (`range = [3.0108332e-36, 3.0112447e-36]`) — makes `k1` overflow
  to `inf` (`8 / 4.1e-39 > f32::MAX`).
- `(k1 * (center - k0)) as usize` then saturates to `usize::MAX` and indexes `bins[usize::MAX]`.
- The builder already handles the semantic degenerate case ("all leaves in one bin") by splitting
  the node in half; only the index computation runs before that protection.

No geometry clamp, collider omission, registry edit or physics-stack upgrade fixes the mechanism:
the collision surface is valid, the binning arithmetic is not.

## Patch — `src/partitioning/bvh/bvh_binned_build.rs`

The bin index is computed by one `bin_index` closure used by both the binning pass and the
in-place sort: a finite scaled value is clamped to `NUM_BINS - 1`; `+inf` clamps to the last bin;
`NaN` (an exactly zero center difference multiplied by an infinite `k1`) lands in the first bin.
Every leaf then has a valid bin, and the existing single-bin fallback splits the node in half.

What changes: for normal centroid ranges the clamp is a no-op, so every tree built today is
byte-identical in structure. Only clusters whose range overflows — today, a panic — now build
with the half-split fallback. Triangle data, positions and the collision surface are untouched.

Compatibility: same crate version and public API as `0.26.1`; no consumer code changes. Drop the
fork when the resolved parry3d contains the upstream fix (the check: `cargo tree -i parry3d`
resolves to a version whose `rebuild_range_binned` clamps or guards the bin index).

## How to check

- `cargo check -p benilla-world` builds the patched crate through avian3d.
- `WOW_CAPTURE=water-noon` with the `wm-hd` profile no longer panics in
  `spawn_streamed_tiles`; the capture then proceeds to its visual gate. (The capture gate itself
  is separately authorized; this note records only the panic reproducer.)
