# Approved Project B completion queue

Owner approved the proposed ticket sizes and blockers on 2026-10-25. Published as local issues **16–32**, retaining planning references C01–C17. [The completion specification and roadmap](D:/WOW/PROJECT_B_COMPLETION_PLAN.md) includes the mandatory **full stock-client replacement release gate**, the full compatibility program and later expansion work.

## Working rules

- Issues 01–15 and their evidence are preserved. Related-issue links map new executable slices to the older broad descriptions; they do not create a new parent-issue hierarchy.
- Use this dependency table to schedule the approved continuation, rather than starting the older broad issue and its new slices as duplicate work.
- `ready-for-agent` means the ticket is specified. It does not override its **Blocked by** edges or authorize execution, deployment, tests, new agents, or live processes.
- Work the frontier: choose an unowned ticket with satisfied technical blockers. Respect current file ownership and explicitly assigned repository scope.
- GLM's wiring at **827ce851** is complete; issue 16 continues at receiver acceptance. Do not repeat the wiring or mistake publisher handoff for delivery.
- Full replacement is not complete when these 17 slices finish. Issue 18 establishes the exhaustive compatibility ledger; unresolved required families become further approved slices. The plan also preserves combat, building and generated-space decision gates.

## Published tickets

| Issue | Plan ID | Deliverable | New-queue blockers |
|---|---|---|---|
| [16 — Real session to native receiver](issues/16-real-session-to-receiver.md) | C01 | A real lab world session delivers its own typed observations to the native receiver while baseline login remains usable with the bridge disabled. | None |
| [17 — WM consumes native state with fallback](issues/17-wm-consumes-native-state.md) | C02 | The existing WM context reads a live client observation, and safely uses its existing fallback when native state is unavailable. | 16 |
| [18 — Inventory the complete stock replacement gate](issues/18-stock-replacement-inventory.md) | C03 | A complete, reviewable compatibility ledger makes it impossible to label the fork a stock-client replacement while required behavior is missing. | None |
| [19 — Render route terrain from a selected manifest profile](issues/19-profile-route-in-application.md) | C04 | The application's asset path renders the fixed route terrain from the selected profile and explains exactly which archive supplied each required asset. | None |
| [20 — Render and animate a route WotLK actor](issues/20-wotlk-actor-rendered.md) | C05 | A source-identified WotLK actor appears with the correct mesh, skin, textures and animation in the same application route. | 19 |
| [21 — Make route water and collision usable](issues/21-route-water-and-collision.md) | C06 | The selected route's terrain, water and required structures have coherent rendering and collision behavior in the application. | 19 |
| [22 — Render the live player and nearby replicated entities](issues/22-live-entities-rendered.md) | C07 | The graphical application enters the lab world and displays the actual player and nearby entities from the Wrath session replica. | 20 |
| [23 — Walk, turn, jump and reconcile on one tile](issues/23-local-movement-and-correction.md) | C08 | Player input moves the actual server-connected character with coherent local presentation and server corrections. | 22 |
| [24 — Complete route travel, chat and reconnect](issues/24-route-travel-and-reconnect.md) | C09 | The player completes the Northshire/Elwynn route across tile boundaries, communicates, and reconnects into a coherent world. | 21, 23 |
| [25 — Perform normal attacks and one real spell](issues/25-ordinary-combat-slice.md) | C10 | The selected character targets one enemy, uses baseline attacks and one selected spell, and sees authoritative outcomes. | 24 |
| [26 — Complete a quest, loot and preserve the reward](issues/26-quest-loot-reward.md) | C11 | The player accepts a real quest, completes its objective, loots and turns it in, then reconnects with the correct reward and inventory. | 25 |
| [27 — Recover from death and reject invalid actions](issues/27-death-and-recovery.md) | C12 | The player can die, release/recover/resurrect in the selected scenario, and sees correct refusal of actions that are unavailable. | 25 |
| [28 — WM performs one typed player action with human override](issues/28-wm-typed-player-action.md) | C13 | WM observes a scoped opportunity, requests one permitted player action and receives its actual outcome while the player can take control. | 17, 25 |
| [29 — Publish one immutable Spellwright revision through WM](issues/29-typed-live-graph-publication.md) | C14 | WM validates and publishes a selected graph through a typed existing-owner path and receives its immutable identity or a meaningful refusal. | None |
| [30 — Change WM selection A to B and roll back live](issues/30-live-selection-and-rollback.md) | C15 | WM selects revision B for a logical action while active revision A instances obey their pinning contract, then rolls the selection back without restarting the server. | 29 |
| [31 — Reload a client asset and Lua behavior with rollback](issues/31-client-live-content-reload.md) | C16 | A running client adopts a staged asset and Lua behavior revision and returns to its previous working version when validation fails or rollback is requested. | 20 |
| [32 — WM publishes a client-content revision and rolls it back](issues/32-wm-client-content-publication.md) | C17 | WM publishes a versioned client-content revision to the intended client, receives application status and can restore the previous revision. | 17, 31 |

Additional source/authorization prerequisites are recorded inside issues 16, 22, 29 and 31. All existing completed prerequisites must be rechecked against the executing checkpoint.

## Current frontier

- **16 — Real session to native receiver:** GLM's existing integration continuation; receiver startup/runtime checks still require applicable explicit authorization.
- **18 — Full replacement inventory:** can proceed through source/document work without launching gameplay.
- **19 — Manifest-selected route terrain:** a useful independent client slice after worktree/source ownership is settled.
- **29 — Typed live graph publication:** source investigation/implementation can proceed when assigned; native deployment/restart remains gated. It need not wait for route rendering or native observations.

Issue 17 is ready in specification but awaits issue 16 acceptance. OpenCode may prepare/review the WM glue in its assigned scope; it cannot close end-to-end acceptance before live delivery is established.

## Publication record

Only Markdown planning/tracker files were changed by Astra. No tests, builds, services, live game/WM operations, commits, external messages or code changes were performed as part of publication. Source draft files remain historical references; the published issues are canonical for ongoing ticket edits.

