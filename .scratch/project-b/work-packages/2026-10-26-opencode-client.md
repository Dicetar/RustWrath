# OpenCode work package — connected replacement client

Date: 2026-10-26, Europe/Moscow. Assigned by the owner's request for a substantial continuation plan. OpenCode is sole client/gameplay integration lead. Astra takes the former GLM/procgen lane. GLM has no active implementation assignment.

## Outcome and starting evidence

Deliver one coherent client path: enter/select character, enter world, move/target/attack/cast, accept and complete the pinned quest, loot and retain its reward, survive relevant death/transfer and logout/re-entry edges. Then expand this path toward the full baseline ledger. Do not substitute packet builders, a renderer demo or an offline generator for connected behavior.

Authoritative shared checkout: `D:/Projects/project-b-client`, branch `wotlk`. Reviewed HEAD before the planning repair: `63e3d4ed`. Prior source slices: `720c83b6`, `2f61b877`, `ec8d1db0`, `b21d0d92`, `f3316a87`, `8ef5833d`, `63e3d4ed`. Re-read current source and Git on admission; these are anchors, not permission to overwrite newer work.

OpenCode reports workspace check excluding procgen, protocol tests 25+2+14, app 6582/6584 with two screenshot/timezone failures, and five Wrath tests. These are reported results at the earlier checkpoint, not results for future commits. Full-workspace compilation is still unverified. Astra's `32e1a8f6` corrects her missing return parenthesis in procgen; do not attribute that defect to GLM or treat the source correction as a successful build.

Parked character selection now has source wiring and a character enumeration parser. It has not passed graphical/live acceptance. The older session-lifecycle checkpoint's missing-roster limitation is superseded by `2026-10-26-oc-parked-reentry.md`; other limitations must be rechecked individually.

## Ownership and execution contract

- Own Wrath protocol/session/gameplay and client integration. Primary paths: `crates/benilla-app/src/wrath.rs`, `crates/benilla-protocol-wrath/**`, and the existing net/glue/roster/inventory/spell owners needed by each explicitly claimed slice.
- Route work may claim exact `benilla-assets`, `benilla-world`, `benilla-m2`, `benilla-adt`, WMO and catalog files after inspecting current implementations. M2 v264/external-skin and MH2O source support already exists: inspect it before adding duplicate parsers.
- Before a substantive edit, publish the exact file allowlist, purpose and base HEAD in a checkpoint. Release that claim in the delivery commit. Keep each slice cohesive and use explicit staging/commit paths; never absorb another worker's dirty files into an unrelated commit.
- Astra owns `crates/benilla-procgen/**`, `docs/procgen/**`, her checkpoints and the new full-replacement inventory under `.scratch/project-b/compatibility/`. Read these outputs; do not edit their claimed files. Shared helper/dependency changes require an explicit ownership handoff, not concurrent edits.
- No reset/stash/clean/rebase or broad commit to make the checkout look tidy. Preserve `tools/patch_procgen_dungeon.py`; do not run it. No new agents or external messages.
- This work package authorizes no new execution. Preserve the owner's restrictions on builds, tests/new tests, linters/formatters, imports, browser/graphical/game/socket/WM/receiver operations, installations and deployment. An earlier Phase 0 grant applies only within its actual explicit scope in OC's session; it cannot be inferred from a report or transferred to Astra. If no applicable grant is established, continue source-only and mark checks pending.
- Treat source, compilation, focused tests, receiver acceptance, WM consumption and rendered gameplay as separate evidence levels. Never close a live acceptance checkbox from source or unit evidence.

## Work order

### O1 — Consolidate lifecycle before adding more producers

Review the parked entry path and its call sites as one ownership chain: stock screen ask → Wrath queue → admission → enumeration → selected GUID validation → player login → new scope. Inspect and repair only demonstrated gaps.

Required behavior: one active/retiring worker; join-before-admit ordering; fresh channels; cancellation from parked/socket/retry states; explicit bounded retry policy; stale picks/replies refused; a dead parked link produces visible loss without an invisible redial. Clean logout returns to character selection; connection loss remains a separate policy. Session failure or panic must produce a visible terminal diagnostic and must not permit worker overlap. A publisher exceeding its stop budget must remain tracked or explicitly block replacement; dropping a handle is not proof that it retired.

Preserve forced terminal events while ensuring an old terminal cannot tear down a newly admitted session. Cover the ordering of terminal queue drain, scope retirement, channel replacement and character selection. Re-enumeration must not silently change the chosen character. Empty rosters and rejected/removed GUIDs must leave usable screen state. Do not auto-create or delete characters.

Deliverable: lifecycle changes, exact source-to-owner map, commit and pending acceptance cases. If this is already correct, record bounded source evidence and continue; do not rewrite it for appearance.

### O2 — Complete the selected combat state path

Finish the protocol-to-existing-owner seams needed by Fireball 133 and baseline attack. Prioritize full SpellStart/SpellGo target blocks (unit/object/source/destination, packed caster identities, flags, optional payloads), spell delay/interruption/channel state, initial and incremental cooldowns, aura add/update/remove, action-button state and the combat outcomes consumed by existing UI/animation owners.

Decode by 12340 source semantics. Preserve widths, enum meanings and conditional payload order; do not truncate a Wrath flag or reuse a vanilla field merely because its name resembles it. Keep remote caster activity separate from player failure state. Unknown variants must be diagnosable and must not fabricate success or damage.

Use existing spell/cooldown/aura owners. Server outcomes remain authoritative; local animation/prediction does not grant quest credit or invent health/resource changes. Broader combat-log coverage may proceed in cohesive children after the selected path has a complete state flow.

Deliverable: normal/rejected/interrupted cast and attack source paths, semantic mapping table, remaining variants, commit per cohesive feature. The first real gameplay acceptance gate depends on O4/O5 as well as this source work.

### O3 — Close inventory fidelity gaps exposed by the quest

Inspect the item/container slice against current Wrath fields and consumers. Ensure item 182 entering a bag changes actual held count for quest 6, and item 6076 becomes the actual server-referenced reward. Follow create/update/delete, bag movement, stack changes, item query, slot ownership and reconnect reconstruction. Check that session teardown clears all caches without inventing inventory loss on an ordinary same-session transfer.

The current narrowing from 28 bank slots to 24 and seven bank bags to six, dropped enchant slots, missing random suffix representation and Wrath-only item attributes are full-replacement gaps. Expand the owning domain where needed; do not declare foreign fields supported by dropping them. Sequence work: scenario-critical inventory first, then explicit children for bank/suffix/enchant/socket/scaling fidelity. Preserve vanilla mode through explicit mode/layout boundaries.

Deliverable: inventory ownership map, scenario source path and enumerated compatibility gaps linked to Astra's ledger. No item/reward spawning or database mutation to manufacture a successful scenario.

### O4 — Death, transfer and recovery as authoritative transitions

Implement/review death, corpse/release/resurrection/refusal and required world-transfer acknowledgement paths through existing state owners. Distinguish same-world movement, map/world generation changes, teleport, loading, dead/ghost and session end. Old-scope entities, item references, actions, target, camera and WM observation must not bleed across an incompatible scope.

Expose server refusals and unusable actions in the stock UI. Do not solve a missing message by marking a player alive, teleporting locally or clearing persistent quest/inventory state. Start with the pinned Northshire scenario's failure/recovery path; broader transport/instance/vehicle behavior becomes ledger children.

Deliverable: transition/acknowledgement table, source changes, explicit unsupported cases and commits. Live character death/recovery remains execution-gated.

### O5 — Make the pinned route graphically ready

Use `2026-10-25-oc-gameplay-acceptance-scenario.md`: human mage with Fireball 133, quest 6, Deputy Willem 823/base spawn 79942, Garrick 103/base spawn 80247, item 182, reward choice 0/item 6076. Quest 5261 remains the explicit-accept control because quest 6 has AUTO_ACCEPT. These are base-data pins, not claims about the running DB or chosen character.

Inspect the current application asset path end to end: manifest/source registration → map/tile selection → ADT/WMO/doodads/liquids → actor/model/skin/textures/animation → camera and collision. Pin the actual neighboring tiles and required structures/assets from source/static files. Identify archive/profile provenance for every blocking dependency. Avoid placeholder bodies, free-fly cameras and collision bypasses as acceptance substitutes.

Prepare a complete source path for the bridge/WMO/water crossings and selected player model. Do not guess binary layouts. If a format fact is missing, isolate the exact chunk/field/reference needed and continue independent source work. Static inspection of the install is read-only; never write generated assets into it.

Deliverable: exact dependency manifest, remaining parser/renderer/collision repairs, route and failure checklist. Acceptance awaits the graphical/live gate below.

### O6 — Native observations and one WM action through existing owners

After the gameplay owners stabilize, review protocol v2.1 sender/receiver coupling, ACK/delivery health, observation age, expiry, scope retirement, fallback and reconnect. Read `D:/WOW/wm-project/AGENTS.md` and its mandated workflow before any WM edit; use per-repository ownership claims. Do not create a second WM runtime or migrate orchestration into the client.

Prepare one permitted typed player action using existing client/server intent and result owners, with bounded action IDs, timeout/cancellation, human override, duplicate rejection and authoritative outcome. Native observation delivery, native context consumption and action outcome are three different gates. Keep secrets out of checkpoints/logs. Do not start processes or publish actions under source-work authorization.

Deliverable: source-integrated observation/action path and exact pending receiver/WM/action acceptance. O6 must not displace O1–O5 while their ordinary gameplay blockers remain.

### O7 — Expand toward the complete replacement gate

Consume Astra's behavior inventory and implement dependency-ordered compatibility children: character management, communication/social, vendors/bank/trade/mail/auction, talents/glyphs/professions/pets, group/raid/instance/PvP, vehicles/transports/phasing, settings/macros/addons, packaging and recovery. Select complete behaviors, not arbitrary opcode batches. Keep unsupported-but-required rows blocking until the owner explicitly changes scope.

Live content/Spellwright publication, client reload/rollback and experimental combat/building remain approved platform goals with their own owners and decision gates. Generated layouts on stock maps do not establish arbitrary runtime terrain or native hot swapping.

## Acceptance gates — prepare now, execute only with applicable explicit authorization

| Gate | Evidence needed | Current posture |
|---|---|---|
| S — Source | Exact commits, mappings, ownership, negative cases, unresolved dependencies | Work can proceed |
| C — Build/focused checks | Full workspace including procgen at one source identity; relevant checks; raw logs and precise exclusions | OC's earlier subset results are reported; new work is unverified |
| G — Graphical launch | Correct profile, real player/creature/terrain, input/camera, asset identities, teardown | NOT RUN / UNKNOWN |
| L — Lab gameplay | Character select → enter → route/combat → quest/loot/reward → logout/re-entry with server state retained | NOT RUN / UNKNOWN |
| W — Receiver/WM | Receiver ACK/store, production context read, stale/disconnect fallback, permitted action/result and override | NOT demonstrated by client-only checks |

Before G/L/W, prepare a concrete launch/run card: source and binary identity, profile and manifest hash, protected account locator (no secrets), exact character eligibility and quest availability, endpoint/process permissions, maximum duration, stop conditions and evidence folder. Account login can evict another client; no account substitution or DB repair. If a detail requires live access, leave it explicitly pending. Do not silently reduce the scenario.

During a live failure, preserve logs and source identity, stop at the predeclared safety boundary, repair the owning seam and repeat only the failed/affected acceptance when authorized. Screenshot/timezone failures require reproduction and classification; their label as pre-existing is reported evidence, not a license to suppress them.

## Delivery discipline

Complete cohesive source slices continuously; do not wait for permission to fix already authorized source work. At each boundary commit explicit paths, release the file claim, and report: source identity, behavior changed, evidence actually obtained, remaining blocker and next slice. If execution is gated, continue the next independent source slice rather than repeatedly asking the same authorization question. Do not start an endless audit after the assigned deliverable is ready.

References: approved `D:/WOW/PROJECT_B_COMPLETION_PLAN.md`; local issues 16–32; latest OC checkpoints; Astra's `docs/procgen/design.md` and companion work package. The plan refines scheduling and ownership without closing or renumbering acceptance issues. Full stock-client replacement remains the mandatory release gate. No project percentage follows from this work-package length.
