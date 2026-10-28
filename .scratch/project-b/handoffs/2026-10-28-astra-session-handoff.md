# Project B — Astra session handoff, 2026-10-28

## Current supersession — bounded grant consumed

The historical UNUSED wording and bootstrap prompt below are superseded. On
2026-10-28 at 17:17 Europe/Moscow, Astra used the grant once at
`3e97bc4d17a76309f47a3e94a3511e6611243741`, with clean tracked source/index before
and after. Check passed (exit 0); existing UI lib suite returned exit 101 with
3337 passed, 2 known timezone failures, 1 ignored. The low-edge clamp test passed.
Total 21.8 seconds within the shared 30-minute limit. No source edit or rerun.
Exact commands, times, failures and logs: [bounded-pass summary](../verification/2026-10-28-astra-bounded/summary.md).
Do not execute the historical grant again. OC's source and other worktrees remain
preserved; installed-addon, live/rendered and full replacement acceptance remain open.

This is the restart document for Astra. Read current source before accepting any worker's completion claim. Preparation performed only source/Git reads and document edits; no verification grant was consumed preparing it.

## Mission and rules

- Priority: faithful replacement of the current AzerothCore 3.3.5a client, stock UI and installed mods. Full stock-client replacement is the release gate. Rendered/live Northshire acceptance remains open.
- Togglable Elden Ring combat and suitable UI are platform goals, requiring actual server authority. Do not invent combat state or mode acceptance.
- Procgen is explicitly deferred. The persistent goal's original procgen wording is stale relative to the user's later priority correction; do not restart procgen from that wording.
- Preserve vanilla defaults while implementing the explicit Wrath target. Read AGENTS.md, docs/METHOD.md, docs/MAP.md and docs/CONTRIBUTING.md. User instructions take precedence over donor-only project assumptions.
- Source edits and explicit-path commits are authorized. Read active claims first and claim exact files in the existing checkpoint before editing. Never sweep the shared index into a commit. No reset/stash/clean/history rewrite or broad staging.
- No subagents or external messages. No tests/new cases/files, builds, linters, extractors, game/graphical/socket/WM/server/DB operations, installs or deployment without an applicable explicit grant.
- Explain concrete failure, likelihood and complexity cost and obtain buy-in before speculative race/security complexity. Do not introduce such work silently.
- Keep source, compilation, tests, receiver acceptance, production consumption, rendered acceptance and demonstrated gameplay separate.

## Exact checkout and preserved work

Inspected baseline: D:/Projects/project-b-client, branch wotlk, HEAD d0ac28ab8829779ae55a536bdaf1085fac923233. This document's commit will advance HEAD. At inspection the tracked worktree and shared index were clean.

Untracked and preserved: .scratch/project-b/verification/** (Astra and OC logs) and tools/patch_procgen_dungeon.py. Do not stage or delete them as cleanup.

Other worktrees from git worktree list:
- D:/Projects/project-b-opencode, abe31781, opencode/project-b-profile-reload.
- D:/Projects/project-b-opencode-assets, 1f8d33b8, detached.
- D:/WOW/wm-project is a separate owner repository.

OC's handoff records their dirty paths. This turn did not independently inspect those other trees; preserve them and consult that inventory before any access beyond reads.

The goal tool reports status usageLimited, timeUsedSeconds 18190 (about 5 h 3 min), not six hours completed. Do not mark the goal achieved or pretend the time target or full project outcome was reached. User's current instruction is handoff preparation.

## Read order and authoritative documents

All following paths are relative to D:/Projects/project-b-client unless absolute:

1. AGENTS.md and its required project documents.
2. This handoff.
3. .scratch/project-b/handoffs/2026-10-28-oc-session-handoff.md — current integration lead handoff, commits fbf25981 and d0ac28ab.
4. .scratch/project-b/checkpoints/2026-10-26-astra-wrath-ui-host.md — chronological file claims and source decisions; read the tail before edits.
5. .scratch/project-b/handoffs/2026-10-27-astra-wrath-item-query-consumer.md — detailed item/API evidence, limitations and OC dependencies. Historical paragraphs are superseded by later follow-ups; do not read an old limitation as current without checking code.
6. .scratch/project-b/compatibility/first-loop-behaviors.md and .scratch/project-b/handoffs/2026-10-27-astra-class-name-consumer.md.
7. .scratch/project-b/checkpoints/2026-10-28-oc-wrath-animation-parity.md and OC's animation timeline/cooldown checkpoints.
8. .scratch/project-b/work-packages/2026-10-26-astra-six-hour-campaign.md and 2026-10-26-system-goals.md for campaign context; apply the user's later procgen priority correction.

Architecture review artifacts are under .scratch/project-b/checkpoints/2026-10-26-astra-architecture-review.md and referenced documents. The Wrath boundary direction was extending existing owners with lossless contracts; the ADR was Proposed, not Accepted. Do not silently promote it.

## Ownership and coordination

Astra's lane is Lua/UI compatibility, installed-addon behavior and item query/link consumers. OC leads session/gameplay/protocol/native/bridge integration and its current animation campaign. WM repository belongs to its owner and requires its AGENTS.md workflow before edits.

No Astra tracked edit is unfinished at this checkpoint. Historical claims are navigation/coordination records, not permission to overwrite later OC changes. Revalidate claims with current Git state. Do not start OC's WP1 animation repair, group-loot protocol, bank projection or mail migration concurrently without coordination.

OC's next source slice: animation.rs CreateAnimation() omitted/nil default and Animation:GetParent for the installed Questie timer. A new regression case needs its own applicable grant. Astra's grant below cannot be transferred to OC.

## Astra implementation map

Recent source changes, not a blanket acceptance claim:

| Commit | Behavior / source entry points |
|---|---|
| e8298b54 | UI layout low-edge clamp repair in crates/benilla-ui/src/layout.rs after an actual suite failure; no Astra rerun yet |
| faf89e39 | Questie GetInventoryItemID, GetItemSpell and IsEquippableItem through existing inventory/template/spell owners |
| 49de390e | formats/item_random_properties.rs retains valid positive rows if independent Wrath suffix-table loading fails; app/entities.rs logs partial failure |
| ee45bf7a | ui_items::item_link_full_in_target; loot-window and receipt links use the active UI layout |
| 2ca39ead, 8fd90bea | group-loot snapshot target layout and retained LootStartRoll suffix factor in ActiveRoll |
| 2b9bcff7 | trade snapshot/refresh and uncached own-item target links |
| 8d4a746b | inspect feed and fresh-hover visible equipment target links |
| e769a4fa | auction refresh/page/list target links |
| 40796c7f, 4cad087e | quest-panel and reward-chat target links |
| 7fd26774 | vendor-list target links |
| 4f972988, 1b6cc7a3 | Wrath GetBuybackItemLink from streamed item/catalog owners and matching rolled row name |
| 89d6e6dc | GetItemFamily reads cached server bag-family mask through CallHost -> ItemSteps |
| 823d7c6a | GetContainerItemDurability reads the existing ContainerSlot durability pair |
| 52d45983 | bare Wrath item-string tooltip resolves rolled name during build/rebuild using existing item-name resolver |
| ed4e3702 | shared Wrath item argument accepts uniquely cached exact names; GetItemCount reuses same lookup |

Key files: crates/benilla-ui/src/script/item_stats/wrath.rs, host.rs, container.rs, merchant.rs, tooltip_item/build.rs; crates/benilla-app/src/script_calls.rs, ui_items/{mod.rs,calls.rs}, ui_merchant/mod.rs and the named UI consumer modules.

Earlier catalog work: 82999308 selects the 24-field Wrath ItemRandomProperties layout; fd5d7479 the 38-field SpellItemEnchantment layout; ae6b8953 retains the separate 29-field ItemRandomSuffix rows and allocation inputs; 719d1b6d connects raw negative-suffix names. See detailed handoff for exact references. Actual target DBC loading was not run by Astra for these changes.

Do not describe negative suffix tooltips as complete. Amount-text substitution is unimplemented; unavailable suffix data can still fall back to a base tooltip name. Several native item enchant slots are lost in the donor projection. Generic target links cannot invent gems absent from the source row.

## Verification ledger and the UNUSED grant

Last Astra item/UI verification was at pinned be00172879e8010deb47c228627976f75dff9c69:
- cargo check --locked --offline -p benilla-formats -p benilla-ui -p benilla-app --lib passed.
- cargo test --locked --offline -p benilla-ui --lib: 3329 passed, 3 failed, 1 ignored.
- Failures: layout::tests::assemble_clamp_low_edge and two existing stdlib timezone cases (date_and_time_read_the_local_zone, time_converts_a_date_table).
- Evidence: .scratch/project-b/verification/2026-10-27-astra-ui4/2026-10-28-be001728-ui-test.log. The older run.log in that directory names b52ad4d3: do not attribute that older transcript to be001728.
- Clamp repair e8298b54 and subsequent Astra item/API changes were outside that pin. No Astra build/test ran on the later changes in this continuation.

IMPORTANT: The user subsequently explicitly answered “Run the bounded pass” to this exact request:

> May I run one bounded verification pass on the committed item-link and addon-query changes: cargo check for benilla-ui and benilla-app --lib, followed by one existing benilla-ui library test run, within 30 minutes? This excludes new tests, reruns, game or graphical execution, sockets, WM, server and database operations.

That grant is UNCONSUMED as of this handoff. It belongs to Astra, and persists across this session transition. Do not ask again for the same pass. Pin the selected committed source before starting and record exact identity, commands, deadline, exits and logs. Suggested exact invocations are cargo check --locked --offline -p benilla-ui -p benilla-app --lib, then cargo test --locked --offline -p benilla-ui --lib, within the single total 30-minute budget. No new cases, reruns, broader build, app tests or runtime execution are authorized by it. If it fails, source repair is allowed but a rerun needs another grant. Do not consume it merely while preparing this handoff.

## OC changes that supersede older Astra observations

Current OC source is newer than several findings in the chronological item handoff:
- Animation is no longer static-only. OC added timeline, constructors, alpha/translation/rotation, loop and bounce work through 6e84aaa1 and accompanying commits. Inspect current animation.rs before accepting either “missing” or “complete” claims.
- OC animation parity checkpoint reports seven focused animation passes, app-lib check and a real-data stock UI smoke. These are OC-recorded evidence, not independently rerun by Astra. Read actual logs and exact pins before relying on their scope.
- That checkpoint still lists rotation hit-test, region rotation and repeating-alpha limitations. Its “complete” heading does not establish installed Questie acceptance.
- WP1 omitted CreateAnimation default and GetParent remains in OC's final handoff.
- 8666d6b1 repairs trade test helpers after Astra's target parameter change. Do not reapply a test-signature repair or report that known compile issue as still current.
- Stock smoke is a limited fixture through PlayerFrame, not a full installed-addon/gameplay/rendered acceptance run.

## Open integration dependencies

1. Group loot: source trace found no Wrath inbound group-roll dispatch or outbound vote tap. Existing stock owners alone do not establish a Wrath route. AC start packets add map ID/count/vote mask, roll includes auto-pass; inspect exact bodies. OC WP2 owns this.
2. Loot response suffix factor is dropped before shared UI rows. AC item-link validator rejects negative suffix IDs with zero factor. Preserve the real field end to end, OC WP3; never guess a factor.
3. Bank: AC supports 28 vault seats and seven bank bags. Donor projection, UI capacity and purchase gates currently constrain them. OC WP4 must coordinate mapping and gates; do not expose purchases of invisible slots.
4. Mail: Postal/aux expect multiple attachments and link APIs. Current donor UI is single-attachment; source trace found no Wrath mail-list mapping. OC WP5 needs an actual attachment/command contract, not synthetic bindings.
5. Difficulty/threat/world queries: earlier Astra review found fabricated unknown instance type and remote-unit battleground assumptions. Read current source and OC WP6 before reporting these as unresolved.
6. Installed Questie, Bagnon, aux, Postal and other addons have not been accepted by Astra through real addon loads. Questie local timers rely on animation lifecycle. Selected 30300 TOC uses Compat AceTimer (OnUpdate), distinct from newer bundled copy and from QuestieCompat's own animation timer.
7. Bagnon feature-detects GetContainerItemQuestInfo and has a fallback; do not add a partial starter-only binding that selects its richer branch without quest-active facts.
8. Item charge semantics: AC Spell.cpp treats nonzero template charges as limited and negative values as expendable. Donor has_finite_charges treats -1 specially. Exact 12340 GetItemCount(includeUses) semantics are unproven; do not rewrite donor behavior from the server observation alone.
9. Native suffix scaled text, descriptor losses, secure UI/coroutine edges, rendered assets and WM production acceptance remain distinct open work. Full replacement remains unproven.

## Next Astra campaign

First execute the unused bounded pass at a pinned snapshot, preserving current OC work. Do not mix source edits into the pin while commands run. Inspect failures before making another claim. The full UI suite can expose unrelated existing failures; retain exact names and distinguish them from new regressions.

Then repair demonstrated item/UI integration failures within Astra's ownership, commit explicit paths, and coordinate contract changes with OC's fresh session. Avoid repeatedly auditing already documented gaps or adding speculative bindings solely to grow coverage. Prefer complete, caller-backed behavior in existing owners.

After the pass, update the existing item consumer handoff and compatibility record with exact evidence. No automatic second pass. Graphical/live Northshire, addon-load harness, new tests and WM/server/DB operations need separate explicit grants.

## Fresh Astra bootstrap prompt

```text
Continue as Astra on Project B. Read D:/Projects/project-b-client/.scratch/project-b/handoffs/2026-10-28-astra-session-handoff.md and its read-order references. Revalidate current Git HEAD, dirty/staged/untracked state and OC file claims before editing. The recorded baseline is d0ac28ab on wotlk; later commits may exist.

Priority is faithful AzerothCore 3.3.5 client + stock UI + installed mods; procgen is deferred. Full replacement and rendered/live acceptance remain open. Preserve OC protocol/native/animation and WM ownership.

The handoff records an explicit UNUSED Astra grant for one pinned cargo check of benilla-ui/app-lib followed by one existing benilla-ui lib test invocation, maximum 30 minutes total. Use that grant without asking again; no new cases/reruns/runtime are covered. Record exact source pin and results. It is not OC's grant.

Resume with that verification, then fix demonstrated UI/item integration problems and continue a cohesive caller-backed source package. Claim exact files, preserve others' work, and commit promptly with explicit paths. Inspect current source before accepting old completion or missing-feature claims. Do not claim the six-hour goal or project complete from source or narrow checks.
```
