# Build 12340 client reference workflow

The owner requested preservation and routine use of the available decompilation on 2026-10-30.
For Project B's Wrath behavior, inspect this reference before guessing mechanisms or adapting a
vanilla layout. This is a source-investigation workflow, not a second implementation project.

## Identity and preservation

- Local checkout: `D:/Projects/wowdecomp-reference`.
- Remote: https://github.com/Terrorblade/wowdecomp
- Pinned revision: `e67b6ffef23655dc7da061cb2d36b7963f6ef71a`.
- Full history available from that remote was fetched: 544 commits at preservation time, all
  advertised branch/tag refs; Git reported non-shallow and fsck reported no errors.
- Keep the reference outside Project B. Do not modify, update, reset or build it as a side effect
  of reading. A deliberate update must retain the recorded reference pin and identify changes.
- If missing or at a different revision, report that fact and inspect the pinned revision with
  Git where available. Do not silently treat a changed tree as the preserved baseline.

Useful read-only preflight:

```powershell
git -C D:/Projects/wowdecomp-reference rev-parse HEAD
git -C D:/Projects/wowdecomp-reference status --short
git -C D:/Projects/wowdecomp-reference rev-parse --is-shallow-repository
```

## Route the question to its authority

| Question | Start with |
|---|---|
| Wrath client rendering, internal formats, lifecycle or API behavior | Preserved decompilation, matched to the original 12340 binary when uncertain |
| Actual selected asset/layout and Lua callers | Owner's read-only selected manifest/profile and installed DBC/M2/FrameXML/GlueXML |
| AzerothCore packet bodies and server outcomes | Matching local server source and actual received responses |
| Donor 1.12 compatibility | benilla-re and the existing donor implementation/tests |

No one source establishes all four. A decompilation function can contain reconstruction errors,
stand-ins or fork changes. Read relevant callers and data types, inspect OFFSET/VERIFY tags and
surrounding TODOs, and cross-check uncertain behavior. Original runtime/data evidence settles
contradictions; a source-file count or README progress percentage does not.

## Entry points

Paths are relative to the preserved checkout:

| Area | Files/directories |
|---|---|
| MH2O heights, depths and UV arrays | `WoW/Source/LiquidSystem/LiquidData.cpp`, `Liquid.h` |
| Liquid geometry and transforms | `WoW/Source/WorldClient/MapChunkLiquid.cpp`, `LiquidGeomFactories.cpp` |
| Water draw/material state | `WoW/Source/LiquidSystem/LiquidRenderer.cpp`, `MaterialWater.cpp`, `MaterialProcWater.cpp` |
| M2 records, tracks and rendering | `Engine/Source/Model2/M2Data.h`, `M2Data.cpp`, `CM2SceneRender.cpp`, `M2Shared.h` |
| Particles and ribbons | `Engine/Source/Services/ParticleSystem2.cpp`, `ParticleSystem2.h`; associated Model2 callers |
| Quest markers | `WoW/Source/Object/ObjectClient/Object_C.cpp` and model/render consumers |
| Lua/UI, quests and item appearance | Search exact binding/struct name under `WoW/Source` and `Engine/Source`; then follow its registered function and producer |

Example: `LiquidData.cpp`'s `CInstanceVertexData::GetHeight` (OFFSET `0x008A3130`) and
`GetDepth` (`0x008A3160`) distinguish separate MH2O height/depth arrays from interleaved MCLQ
records. Do not generalize one format's stride to another.

## Implementation loop and evidence

1. State the exact failing behavior and find its existing Project B owner.
2. Search the reference for the symbol, layout or original address. Read the narrow call chain.
3. Record the pin, reference file/symbol/address, mechanism and confidence in the owning issue or
   checkpoint. Identify any stand-in or unsupported branch explicitly.
4. Implement the mechanism idiomatically through existing Rust owners, preserving version
   boundaries. Do not transplant global architecture, use fake success values or bypass a
   producer merely because a Lua caller then stops throwing.
5. Use the applicable authorized focused verification and original symptom rerun. A reference
   match, test pass or successful build alone does not establish rendered/gameplay acceptance.

No root reuse license was found during acquisition. Inspection is for behavior research.
Do not copy decompiled source into Project B's MIT/Apache code or redistribute it as part of
the client without resolving provenance/licensing. No reference build scripts, dependencies or
binaries need execution for ordinary investigation. Existing permissions for server, install,
credentials, combat and verification remain unchanged.

Handoffs must mention this local reference and pin when assigning Wrath work. Record a specific
reference gap when fallback research or original-binary analysis is needed; avoid rediscovering
known layouts through repeated blind client launches. Acquisition/comparison details remain in
`.scratch/project-b/handoffs/2026-10-30-12340-decomp-reference.md`.
