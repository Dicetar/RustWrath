# Astra work package — former GLM lane and replacement coverage

Date: 2026-10-26, Europe/Moscow. Owner: Astra. OpenCode remains client/gameplay integration lead. GLM has no active assignment. This is a bounded staged backlog requested by the owner, not authority to execute checks or introduce a second content pipeline.

## Starting point and ownership

The generator is a self-contained data crate with no production consumer. Original `9a1e365e` bundles other workers' catalog/assets/bridge/probe changes; preserve its history and review provenance by path. Astra adopted partial procgen repairs in `3563bc35`, then corrected her own unclosed Result expression in `32e1a8f6`. The latter is a source correction, not compilation evidence. The original report of 17 tests does not cover the repairs.

Own `crates/benilla-procgen/**`, `docs/procgen/**`, Astra checkpoints and `.scratch/project-b/compatibility/**`. OpenCode owns Wrath protocol, application, inventory, lifecycle, graphics integration and WM consumer implementation. Do not edit those files concurrently. Claim exact paths at each slice and release them in a cohesive explicit-path commit. Ownership of this lane does not mean every file is perpetually locked.

No build, tests/new test cases/files, linter/formatter, import, generator invocation, browser, game, receiver/WM process, install, server/DB operation or deployment is authorized by this plan. Do not inherit OC's execution grants. No subagents. Read source and Git, implement authorized code, document pending checks. Leave the old untracked patch script untouched and unexecuted.

## A0 — Repair the immediate blocker

Delivered: `32e1a8f6` closes `Ok(ProcgenOutput { ... })`. Keep attribution clear: Astra introduced the missing delimiter, not GLM. Recheck any further source-reported compilation failures, make the smallest fix in this lane and commit it promptly. Pending: actual compiler acceptance for the complete workspace under a separately applicable grant.

## A1 — Finish the procgen contract audit

Trace each public entry point from request validation to allocation/indexing and error propagation. Review documented limits against intermediate arithmetic and worst-case work, including RNG helpers used by generated paths. Keep dimensions, loops, item counts, actors and noise parameters bounded; reject invalid/non-finite requests explicitly rather than silently guessing a layout. Avoid changing algorithms merely for aesthetics.

Review the complete manifest field mapping against all public data structs and built-in catalog identity. Check sequence lengths, stable enum tags, float bits, static part definitions, request coverage and version semantics. The fingerprint is neither a cryptographic hash nor persistence by itself. If an output/encoding change is required, define the appropriate new version and preserve old artifacts; never reinterpret v1/v2 data silently.

Review graph versus tile connectivity, door markers, single-room entrance safety, level ceilings, zero packs/density, coordinate domains, duplicate positions and encounter grouping. Correct demonstrated defects; document intentional limitations without claiming a generic playable dungeon. Do not expand into a full generation engine while baseline integration is blocked.

Deliverable: one or more cohesive fixes and a source audit mapping each earlier finding to its implementation and pending behavior check. Do not call the algorithms tested from inspection. If review finds no required change, record the evidence and finish the slice.

## A2 — Define a durable generated-artifact handoff

Inspect existing project persistence/content-manifest conventions first. Reuse an existing suitable serializer/bundle convention; do not add a dependency or invent a parallel publisher merely to make export convenient.

Make the replay contract concrete: retained full request, generator/encoding version, owned artifact data or pinned catalog identifiers, stable ordered encoding, recomputable fingerprint, explicit supported versions and invalid-data refusal. Static Rust references and a u64 hash are not a durable schema. Preserve reference/catalog identity alongside part IDs. Reader validation must enforce the same resource/shape bounds before allocating or accepting data.

First deliver a reviewable request/artifact schema and migration policy grounded in existing owners. Implement a bounded library-level export/import seam only where those decisions are settled and within the procgen allowlist. Keep it inert; no CLI execution, file writes outside an explicit caller request or server publication. A format can be authored source-only; successful roundtrip/replay stays pending until authorized checks.

Acceptance to prepare: same pinned request/build reproduces expected data; changes to each meaningful field affect canonical encoding; truncation/unknown versions/oversized counts fail; domains and catalog identities survive roundtrip; invalid inputs leave no partial accepted artifact. Portable floating-point replay needs cross-build/platform evidence and is not promised from byte encoding alone.

## A3 — Prepare one usable encounter handoff on an existing map

After A2, define a small generated encounter layout using existing WM/server owners and a stock-map scenario. Inspect those owners read-only; read WM repo instructions before accessing implementation conventions. No WM/server source edit or publication is included in this lane without a specific ownership transfer.

The artifact must make coordinate conversion explicit: source domain, origin, scale, height rule, map/world scope, template mappings, encounter IDs and cleanup identity. Distinguish proposed placement from collision/navigation-validated placement. Reject unknown creature/NPC/item mappings; do not fabricate server IDs or NPC services. Define how an existing owner would validate, stage, publish and remove it, with a result/refusal identity.

Deliverable: a concrete static request/example mapping and consumer contract OC can integrate, with exact missing data/authorization. No hand-authored JSON demo standing in for the real generator seam. Do not promise arbitrary runtime ADT, matching vmaps/mmaps, live building or server authority from generated tiles.

## A4 — Establish the full-replacement behavior inventory

Advance approved issue 18 in `.scratch/project-b/compatibility/`. Keep the published issue IDs and completion plan intact. Inventory current server opcode/field definitions, existing adapters, installed UI/API/events, asset formats/profiles and available addon metadata through static inspection. Never expose account credentials or addon private content.

Use stable independently assessable behavior rows with: family, owner, source reference/commit, required normal/error/transfer/reconnect/persistence scenarios, dependencies, implementation status, evidence level and acceptance status. Cover at least character management; world/entities/travel; graphics/assets/collision; combat/spells/auras; quests/death; inventory/bank/vendor/trade/mail/auction; talents/glyphs/skills/professions/pets; social/groups/raids/PvP; vehicles/transports/phasing/instances; stock UI/addons/macros/settings; WM/live publication/reload/rollback; packaging/recovery.

Capture concrete known losses such as narrowed bank capacity, dropped enchants, suffix/scaling/socket data and 1.12 UI semantics. A parsed opcode or catalog is evidence of a component, not completion of a behavior. Server-disabled/unavailable features remain blocked requirements unless the owner explicitly changes scope. Unknown installed-addon versions remain open inventory facts.

Separate baseline, required installed extensions and experimental capabilities without removing release requirements. Publish a first source-grounded ledger plus an explicit uncovered-surface list. A first pass is not exhaustive by assertion. Establish a fixed completion denominator only after inventory coverage is reviewed; until then give qualitative status, not invented percentages or ticket-count progress.

Deliverable: ledger and dependency-ordered child-slice proposals for OC, linked to existing issues rather than duplicated assignments. Full replacement remains open until every required behavior and operational gate has adequate evidence.

## A5 — Evidence review and bounded support for OC

At OC handoffs, review changed owner seams against the actual source and the declared scenario. Prioritize authoritative state, numeric/field semantic translation, lifetime/scope, receiver outcomes and graphically missing dependencies. Use concrete counterexamples; do not restart a full audit of unchanged code every time.

Report defects to the owner with exact file/line and behavioral impact. OC repairs OC-owned files; Astra repairs this lane. Catalog/parser support outside these paths requires a new explicit file claim and a demonstrated dependency, not speculative bulk edits. Do not silently message external sessions or create agents.

Record reported check identity separately from source review, compiler results, receiver/WM consumption and rendered gameplay. Track current exclusions and stale reports. Preparing an acceptance procedure is useful work but never counts as running it.

## Delivery order and stop conditions

Order: A0 → A1 → A2 → A3; A4 is an independent source/document workstream that can progress when the next generation seam needs an OC-owned decision. A5 runs only at concrete handoffs. Do not block OC on optional procgen expansion; the syntax defect is the immediate shared build blocker, while the generator's runtime consumer is later platform work.

Each slice ends with explicit-path commit, released claim, concise source/evidence report and exact pending checks. Once its deliverable is ready, stop that slice; do not expand into an endless polish/review cycle. The replacement release gate and experimental platform completion remain distinct milestones under the approved plan.

References: `docs/procgen/design.md`; `2026-10-25-astra-procgen-repair.md`; `2026-10-26-astra-procgen-syntax.md`; approved issue 18 and issues 16–32; `D:/WOW/PROJECT_B_COMPLETION_PLAN.md`; companion OC work package. These documents schedule existing work and expose decisions; they do not authorize execution or claim product acceptance.
