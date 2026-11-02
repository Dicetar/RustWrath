# Owner decisions — adopted defaults (vetoable)

Read dated amendments before applying an earlier proposal. The 2026-11-01 sequencing and
single-owner workflow below supersede older permanent agent/crate partitions. Current
assignments belong in [active ownership](../agents/active-ownership.md); operational rules
are in [the workflow](../agents/workflow.md). This record does not establish that planned
modules or alpha acceptance are implemented.

Recorded 2026-10-24 after owner granted execution authority ("you can build, code and do anything you need").
These defaults were recommended by GLM and adopted under a standing execution authorization; **absence of objection is not owner consent** — especially row 1 (maintenance-restart boundary), which stays a proposal until the owner confirms it.
The owner may override any row; later documents must not treat them as owner-confirmed until then.

| # | Question | Adopted default | Basis |
|---|---|---|---|
| 1 | Maintenance boundary | Routine WM definitions/behaviors update live (proven lane). Rare *new native capabilities* (new C++ primitives, new wire framing) may use a declared, planned maintenance restart; goal is rarity, each preceded by a recorded gap. | GLM recommendation, response 02; Astra flagged as open owner question |
| 2 | Addon compatibility targets | First: installed WM addons (WMBridge, WM_Multiclass, WMQuestieCompat, Spellwright flow). Then: owner to pick exact Bagnon/Bartender4/Questie-335 builds. | Plan rev 3 P5 |
| 3 | First route | Northshire → Elwynn tile boundary (Hogger stretch optional). | GLM R4 default |
| 4 | Souls combat sequencing | Optional parallel work, never a gate prerequisite. | Astra correction, accepted |
| 5 | Stock-client coexistence | Not a product goal. Fallback deployment retained; baseline-protocol invariant maintained as regression check. | Response 01 R8 / plan rev 3 |

## Long-term goals added by owner

| # | Goal | Added | Staged interpretation (proposal, vetoable) |
|---|---|---|---|
| 6 | **Client-side hot-patching** — live client changes without client restart/rebuild, mirroring the server-side live-change rule | 2026-10-24, owner message | **C1:** hot-reload of client data — assets via a file-watch/re-import path on the manifest loader (ticket 05 must leave a seam), tuning/config files, and Lua/XML (the addon host already has a reload lifecycle; stock `/reload` semantics). **C2:** WM-pushed client-side definitions — the native client channel (ticket 11) carries versioned client-content publications (UI tweaks, shell visuals, tuning) to connected clients. **C3, optional/last:** hot-swappable native presentation modules (dylib with stable ABI, swap at state boundaries) — explicitly not promised by the base plan's extension model; only if C1+C2 prove insufficient. |

2026-10-31 update: the owner promoted native presentation hotpatching to immediate work and
approved the one-time host rebuild/restart. Routine supported changes should then activate in
place, using `/reload` for UI. ADR-0002 records the supported interfaces and maintenance limits.
The owner subsequently transferred graphics, rig math and integration scheduling to Astra;
OpenCode retains playable combat. The post-edit pose seam and restoration adapter are now
implemented, with a focused reload/non-accumulation check passing. Actual client activation
still requires rendered acceptance. File-watch support alone does not establish complete
asset/model hotpatching.

2026-11-01 owner-confirmed decisions:
1. **Track Sequencing**: Finish the playable Northshire alpha route first (roaming creature
   position sync, NPC Unknown/Dead state, cast-state gate, remaining FrameXML/UI parity, and
   camera-dependent magenta lighting / HD model fixes), then expand the hot-pluggable module
   system and implement Souls / Elden Ring combat.
2. **Modular Hot-Pluggable Gameplay Seams**: Movement, combat (including the `Funny-Bones/ELDEN-RING-Combat-Rewrite`
   Souls combat rewrite), procedural terrain generation (with dynamic spawns of mobs, decorations,
   and items), and story quests must be implemented as hot-pluggable/swappable modules
   (`PoseModuleV1`, `MovementModuleV1`, `CombatModuleV1`, `SpaceModuleV1`, `StoryModuleV1`) so
   subsystems can be swapped or unloaded at runtime without breaking the base project.
3. **Single-Owner Workflow**: Retire the split Astra-vs-OC crate lockouts in favor of a unified
   single-owner workflow where any agent can touch any crate per ticket.
