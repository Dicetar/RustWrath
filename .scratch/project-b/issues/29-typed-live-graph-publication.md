# 29 — Publish one immutable Spellwright revision through WM

**Status:** ready-for-agent

**Plan reference:** C14. Owner approved ticket sizes and blockers on 2026-10-25. See [the approved completion queue](../completion-index.md).

**Implementation state:** Acceptance open — existing code may cover part of this slice; inspect the current checkpoint before starting.

**What to build:** WM validates and publishes a selected graph through a typed existing-owner path and receives its immutable identity or a meaningful refusal.

**Blocked by:** Existing [03 — Live-rule source contract](03-b001b1-live-rule-source-contract.md), recorded complete. No new queue blocker for source work; deployment requiring new native code remains gated by the restart-boundary decision and applicable authorization.

**Related existing issues:** [03](03-b001b1-live-rule-source-contract.md), [10](10-b001b2-live-change-proof.md). These are traceability links, not additional blocking edges; existing issues and evidence are preserved.

**Suggested owner:** OpenCode after explicit WM/server source assignment.

## Acceptance

- [ ] Inspect current Spellwright implementation and WM owners before choosing the missing publication adapter.
- [ ] Resolve the actual SubmitCommand/dispatcher gap without freeform mutation paths or reusing stock spell identities.
- [ ] Preserve server resource/quote/validation behavior and immutable revision pinning.
- [ ] Use observed deadlines: chunk upload has a 30-second inactivity timeout refreshed per accepted chunk; quote window is separately 120 seconds, subject to re-reading current source.
- [ ] Demonstrate a valid publication and invalid graph rejection without damaging the prior valid revision.
- [ ] Report any required binary deployment as a separate capability/deployment decision; do not disguise it as an already live hot change.

## Boundaries and handoff

This slice may have multiple implementation locations but one narrow end-to-end behavior. If the necessary dispatcher is larger than one context, split its exact vertical behavior before execution.

Keep the slice within one fresh implementation context. If exploration reveals additional independent behavior, identify the smallest complete child slices before expanding scope. Read the current project context, relevant skill/rules and source checkpoint. Preserve unrelated edits and existing ownership.

Execution checks described above are acceptance requirements, not authorization. Follow explicit permissions applicable to the executing agent. Report source implementation, compilation and runtime evidence separately. An unexecuted required check remains UNKNOWN. Do not commit, deploy, start services or mutate live state merely because this published ticket exists.

Handoff: exact source identity and changed paths; externally visible behavior; evidence actually collected; unresolved acceptance; next dependent slice. Close only after acceptance is supported. See [the completion plan](D:/WOW/PROJECT_B_COMPLETION_PLAN.md) for the mandatory full stock-client replacement release gate.


## Source gap list (2026-11-21, OpenCode swarm)

Method: Read issue 29, AGENTS.md, docs/agents/sync.md, handoffs 2026-10-28-oc-session-handoff.md, 2026-10-27-oc-native-state-consumer-contract.md, 2026-11-20-oc-issues-25-27-gaps.md, issues 03 and 30, docs/live-rules/B-001B-1-contract.md, docs/opencode/live-change-integration.md, and owning source under crates/benilla-wm-native/src/action.rs, crates/benilla-wm-native/src/publisher.rs, crates/benilla-wm-driver/src/main.rs, crates/benilla-app/src/wrath.rs, crates/benilla-app/src/wrath_native.rs, crates/benilla-ui/src/script/addon_message.rs, integrations/wm-native/src/wm_native/context_adapter.py. No Cargo, test, client or server run.

| Acceptance line | Source that meets it | Gap |
|---|---|---|
| Inspect current Spellwright implementation and WM owners before choosing the missing publication adapter | `docs/live-rules/B-001B-1-contract.md:19-29` (SpellmakingV3 Command/Purchase/Configure), `docs/opencode/live-change-integration.md:300-345` (route/selection gates), `integrations/wm-native/src/wm_native/context_adapter.py:NativeClientObservationLoader` | No WM publication owner exists: the native seam is observation-only. `crates/benilla-wm-native/src/action.rs:22:ActionIntent` has no Spellwright publication variant, and `crates/benilla-wm-driver/src/main.rs` only publishes observations, so no adapter has been chosen or implemented. |
| Resolve the actual SubmitCommand/dispatcher gap without freeform mutation paths or reusing stock spell identities | `docs/live-rules/B-001B-1-contract.md:29` (MISSING ADAPTER: file-static Command), `crates/benilla-wm-native/src/action.rs:22:ActionIntent`, `crates/benilla-app/src/wrath.rs:3227:match &request.intent`, `crates/benilla-ui/src/script/addon_message.rs:125` (WM_FORGE3 chunking) | The only submission path is the addon whisper dispatcher (`WM_FORGE3`); nothing outside that translation unit can submit. The typed `ActionIntent` enum and its dispatch match have no publication arm, and `benilla-protocol-wrath` has no Spellwright packet, so WM cannot publish through an existing typed owner. |
| Preserve server resource/quote/validation behavior and immutable revision pinning | `docs/live-rules/B-001B-1-contract.md:23-24` (Purchase = new immutable identity; Configure), `docs/opencode/live-change-integration.md:230-260` (instances pin design), `crates/benilla-wm-native/src/publisher.rs:180:publish` | Immutable pinning and quote/nonce validation live only in server-side SpellmakingV3. The Rust/WM side has no publication code, so there is no typed path that preserves quote freshness, AtMaker/ingredient/money gates, or the new-spell-id (not stock-identity) allocation. |
| Use observed deadlines: 30-second inactivity upload timeout refreshed per accepted chunk; 120-second quote window | `docs/live-rules/B-001B-1-contract.md:19` (30 s upload, 120 s quote), `docs/opencode/live-change-integration.md:24-43` (30 s correction), `crates/benilla-ui/src/script/addon_message.rs:125-134` (<=150-byte chunks) | The two docs disagree about the contract's stated upload expiry (live-change-integration still says the contract says 300 s). No Rust code tracks or enforces either deadline; chunking only bounds frame size. |
| Demonstrate a valid publication and invalid graph rejection without damaging the prior valid revision | `docs/opencode/live-change-integration.md:255-320` (proof sequence), `crates/benilla-ui/src/script/addon_message.rs:691:wm_spellmaking_self_whisper_round_trip` | Only a self-whisper round-trip unit test exists. No code or test exercises a real publication, a malformed-graph rejection, or preservation of the prior revision; this needs the live server and is not source-provable. |
| Report any required binary deployment as a separate capability/deployment decision | `docs/opencode/live-change-integration.md:341-352` (gates table), `docs/live-rules/B-001B-1-contract.md:29` (adapter requires new native code) | No Rust/WM code reports deployment identity or capability. Whether the adapter needs a server binary rebuild/restart is an external deployment decision; source cannot demonstrate live hot change. |

### Tickets opened by this survey

1. `Blocked: needs owner decision` — the Spellwright SubmitCommand adapter and its server-side dispatcher must exist before a client ticket can publish a graph. A client-only `ActionIntent` variant that returns `Unsupported` is not a ticket: it would add dead API surface and present a stub as progress. The owner decides whether the adapter is a new native seam or a server rebuild.
2. `oc/spellwright-deadline-doc-reconciliation` — Reconcile the upload-expiry wording between `docs/live-rules/B-001B-1-contract.md` and `docs/opencode/live-change-integration.md` so both cite 30 s inactivity refreshed per accepted chunk and 120 s quote, with the `SpellmakingV3.cpp:346-350` source lines. Acceptance: both docs state the same deadlines and cite the same source lines; no 300 s upload claim remains. Files: `docs/live-rules/B-001B-1-contract.md`, `docs/opencode/live-change-integration.md`.
3. `Blocked: needs owner run` — Acceptance line 5 (valid publication + invalid rejection without damaging prior revision) and line 3 (immutable pinning observed) require a live AzerothCore server and deployed SpellmakingV3 binary; only runtime quote/purchase/rejection and instance pinning can judge them.
4. `Blocked: needs owner run` — Acceptance line 6 (binary deployment) requires an owner-approved deployment decision and process/binary identity capture; source cannot establish whether the adapter is hot or restart-gated.


