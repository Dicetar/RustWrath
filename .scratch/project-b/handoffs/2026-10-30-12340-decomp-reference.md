# 12340 decompilation reference for Project B

Ongoing agent workflow is now canonical at `docs/agents/12340-reference.md`, linked from
AGENTS, METHOD, CONTRIBUTING and CONTEXT. The `project-b-12340-reference` skill is installed at
`C:/Users/Qraag/.codex/skills/project-b-12340-reference/SKILL.md`. This handoff retains acquisition
details; use the canonical guide for future assignments.

Owner requested investigation, download and use on 2026-10-30. Local checkout:
`D:/Projects/wowdecomp-reference`.

Remote: https://github.com/Terrorblade/wowdecomp
Pinned commit: `e67b6ffef23655dc7da061cb2d36b7963f6ef71a`.
Initially a single-branch shallow clone; subsequently fetched full available history and all
advertised branch/tag refs from the surviving remote. Git reports non-shallow, 544 commits;
`git fsck --full --no-reflogs` reported no errors. Pinned worktree remains at the commit above.
No build, dependencies,
executable execution or game data download. Checkout clean at acquisition.

## Why this copy

No independent full-client acceptance establishes a most-complete fork. Compared five public
trees through GitHub metadata and READMEs. This is a preserved 12340 reference candidate,
chosen to limit experimental platform/realm changes; it is not certified pristine upstream.
Terrorblade contains 2,726 C++ source/header files under Engine/WoW. Counts are inventory,
not completeness measures. Its old upstream progress numbers are self-reported.

Other inspected candidates:
- RovxBot `82b1681172b44cecff75cf579c79b63c90961e14`: 2,768 source/header files; extensive
  Linux/macOS port work, Windows used as reference, documented remaining runtime failures.
- RosemyneH `cb54f58119b88c8a3cfda6ad8d64e747dc65df7d`: 2,782; explicitly experimental.
- STRHercules/Esteria `6723f9fe1c88e17ae493650e87386e603392e6a8`: 2,907; realm-specific
  extensions, Vulkan/D3D11 and additional rendering features.
- gswxy `9ec0359dca2759144d234a0f7dd84c3c54804a87`: 2,774; not independently accepted.

Only the selected reference was downloaded. No backup copies or second engine project.

## Use in current repairs

Read these local paths first when resolving a 12340 behavior question:

| Problem | Reference paths relative to the downloaded tree |
|---|---|
| Water geometry/vertex format and offsets | `WoW/Source/LiquidSystem/LiquidData.cpp`, `Liquid.h`, `WoW/Source/WorldClient/MapChunkLiquid.cpp`, `LiquidGeomFactories.cpp` |
| Water material and rendering state | `WoW/Source/LiquidSystem/LiquidRenderer.cpp`, `MaterialWater.cpp`, `MaterialProcWater.cpp` |
| M2 materials, draw state, texture/track layout | `Engine/Source/Model2/CM2SceneRender.cpp`, `M2Data.cpp`, `M2Shared.h` |
| Version-264 particles | `Engine/Source/Services/ParticleSystem2.cpp`, `ParticleSystem2.h` and their Model2 callers |
| UI contracts | Search `WoW/Source` and `Engine/Source` for the Lua binding name and original address tag |

Start with exact symbol/format questions; cite the reference commit, file and OFFSET tag where
present. Cross-check doubtful reconstruction with the matching original 12340 executable,
installed data, caller ABI or deterministic test. A function in the tree is not evidence it is
correct or complete. Do not import the reference's build scripts or run its executables as part
of inspecting source.

No root reuse license was found during acquisition. Keep this outside the MIT/Apache Project B
tree. Use it for behavior investigation; do not copy source into Project B without resolving
provenance and licensing. Existing game-install/server/credential boundaries remain.

Current immediate application: compare the MH2O array-layout fix in `benilla-adt` with this
tree's vertex-data accessors, then validate the same scene before declaring the massive-plane
symptom closed. Next inspect actual material/texture binding for white quest markers and
version-264 particle semantics. This document does not claim those graphics bugs fixed.

Reference comparison completed: `LiquidSystem/LiquidData.cpp:82` (OFFSET 0x008A3130)
reads the height float array; line 92 (OFFSET 0x008A3160) reads depth after vertexCount*4
height bytes. This independently supports the current MH2O decoder repair. Particle record
layout is explicitly described by `Engine/Source/Model2/M2Data.h`, class M2Particle; use its
fields/offsets and relevant runtime callers to replace the existing vanilla-only decoder,
retaining malformed-input bounds and the memory regression acceptance.
