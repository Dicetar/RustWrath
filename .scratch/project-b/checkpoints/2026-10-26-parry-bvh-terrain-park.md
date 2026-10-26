# Parked: parry3d BVH build panic on a streamed terrain collider

Date: 2026-10-26
Owner: OpenCode (found during the shaped-capture smoke; left parked, un-fixed)
Gate touched: rendered/alpha capture — not the native WM client lane.

## Failure

`WOW_CAPTURE=water-noon` (water-mark shaped capture), run with the profile source:

- `WOW_DATA=D:\WOW\world of warcraft 3.3.5a hd\data`
- `WM_ASSET_MANIFEST=D:\Projects\project-b-client\docs\profiles\wm-hd.mpqlist`
- `WM_ASSET_PROFILE=wm-hd`

The app boots, opens a Vulkan window, and panics while
`benilla_world::terrain_stream::spawn_streamed_tiles` is attaching the tile's static collider:

```
panicked at parry3d-0.26.1\src\partitioning\bvh\bvh_binned_build.rs:58:28:
index out of bounds: the len is 8 but the index is 18446744073709551615
Encountered a panic in system `benilla_world::terrain_stream::spawn_streamed_tiles`!
```

Parry instrumentation (applied to the registry copy only as a diagnostic, then reverted; no
workspace or third-party patch survives) narrowed it to the binned builder's binning constant:

```
BVH DEBUG leaves=2 axis=1 range=[3.0108332e-36, 3.0112447e-36]
           k1=inf center=Vec3(160.41666, 3.0112447e-36, 9164.584)
```

`NUM_BINS=8`; `k1 = 8·(1−ε)/(range[1]−range[0])` overflows to `inf` because the two leaves'
centroid spread is a subnormal (~4e-39), so one `bin_id` cast saturates to `usize::MAX`.

## Failing collider evidence

`crates/benilla-world/src/terrain_stream/collider.rs` instrumentation at
`build_collider_task` (also reverted) logged the collider that the panic followed:

```
DEBUG collider verts=37120 tris=65520 bad_verts=0 degenerate=0 max_abs=9600
tiny_y=[] tiny_tris=[]
```

- `37120` = 256 chunks × 145 verts; `65520` triangles; streamed-tile terrain geometry in draw
  space (`wow_to_bevy`), no non-finite vertices, **no zero-area triangles**, no vertices or
  triangle centroids anywhere near subnormal magnitude.
- The panicking node is a depth-2 cluster deep inside this valid 65k-triangle mesh: two leaves
  whose AABB centers coincide in x/z and differ only by a subnormal y, while every other axis has
  a zero-or-smaller spread at that node, so the subnormal axis wins `max_position()`.

## Disposition (per direction: park, do not patch)

- No third-party source patch was kept (`parry3d-0.26.1` registry copy restored byte-identical;
  verified no `.bak` left).
- No geometry clamping, no collision omission, no local `[patch]` fork.
- Candidate directions, for the owner to choose: upstream parry fix/upgrade, a `BvhBuildStrategy`
  that tolerates degenerate clusters (Ploc is not reachable through `TriMesh::with_flags`), or
  splitting the terrain colliders single-trimesh-per-tile construction so no BVH node is built
  from a degenerate cluster. Each changes collision surface sizing; not decided here.
- Baseline (`docs/profiles/base.mpqlist`) and the stock discovery path did not reach this code
  path in the same run (base exits before world attach), so the reproducer as run is: wm-hd
  profile + `water-noon` capture.

Action status: REPAIRED at the mechanism (`a8a51af9`). `third_party/parry3d` vendors 0.26.1 with
`rebuild_range_binned`'s bin index clamped (`third_party/parry3d/BENILLA.md`). Verified with a
standalone reproducer outside the repo: the registry crate panics on two valid nearly coincident
flat triangles at exactly this panic site; the patched crate builds the same 66-triangle mesh.
The graphical capture itself (terrain spawn, route) still needs its own authorized run.