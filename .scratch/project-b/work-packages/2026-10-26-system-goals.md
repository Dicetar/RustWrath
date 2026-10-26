# System implementation goals — Astra and OpenCode

Owner instruction: substantial coding sessions with multiple cohesive implementation milestones. This supersedes the proposal-only limits of Astra A2/A3 and extends OC O6/O7. A milestone is a commit boundary, not a request to stop and ask for another task.

## Astra: durable procedural content and encounter planning

ACTIVE source claim at base 5a6eaaa4: `crates/benilla-procgen/**`, `docs/procgen/**`, this file, Astra checkpoints. Narrow shared claim: the `benilla-procgen` dependency stanza in `Cargo.lock`; no other lock changes. OC must leave these paths/index entries alone. GLM has no assignment.

Implement an owned versioned artifact model, lossless float/integer encoding, bounded strict JSON, pinned build/catalog identities, supported-catalog restoration, complete manifest verification, defensive imported-data validation, deterministic encounter placement with independent coordinate domains and explicit server template mappings, inert drafts conforming to the existing WM bundle/action owners, cleanup/partial-application receipts and an offline CLI. No server IDs, NPC services, collision proof or execution authority may be invented. Read actual WM payload contracts before emitting drafts.

Commit each coherent subsystem promptly using exact paths. Retain the generator-v2 output stream unless a demonstrated algorithm fix requires a version bump. The persistence schema is a separate version. Keep the architecture ADR Proposed. Completion requires implemented source paths and a source review, not just this plan or example JSON. Runtime acceptance remains independently pending.

## OpenCode: native WM observation and player-action system

Deliver the full source path from the authoritative Wrath session through observation transport into WM context, and from WM player intent through existing control owners to authoritative client/server results. Reuse the current receiver, action/control bus, application command owners and WM lifecycle. Do not create a competing publisher, execution bus or context store.

1. Read `D:/WOW/wm-project/AGENTS.md` and its mandated workflow documents; inspect current dirty state, bridge copies, schemas, receiver, runtime, context builder and action coordinator. Establish exact file claims before editing. Audit the earlier “integrated” claims against current call sites. Preserve Astra's paths and shared-index entries.
2. Finish observation end to end: session/world generation, self identity and position timestamps, authoritative target/combat/resource/death state, delivery ACK versus producer/session freshness, receiver acceptance and WM context inclusion. Define and implement stale/unknown/disconnected behavior; old observations must never revive retired scope. Retain bounded fallback with explicit provenance, not fabricated fresh data.
3. Implement the permitted Wrath player-action adapter using existing command owners: move/stop, target, attack/stop, cast/cancel, interact/gossip, quest and loot actions where the owners exist. Carry typed arguments, session/world scope, request identity, deadline and cancellation. Reject unsupported actions visibly. Server replies determine outcomes; enqueue success is not gameplay success.
4. Connect WM's existing action admission and result lifecycle to that adapter. Enforce one authoritative owner, bounded queues, deduplication, stale-scope/deadline rejection, human override and stop/cancel. Preserve explicit partial/unknown outcomes when the link fails after send. No unsafe replay of non-idempotent actions on reconnect.
5. Complete recovery: startup/shutdown, receiver loss, producer retirement, context expiry, transfer/death, character selection/re-entry, reconnect ownership, pending-result invalidation and cancellation. Reuse O1 lifecycle rules; never allow a second session/publisher because a timeout dropped a handle.
6. Expose useful diagnostics through existing health/status owners: observation age/scope, transport ACK, WM consumption age, admission/refusal, queue pressure, pending action and final authoritative result. Keep secrets out of diagnostics. Cross-host settings reload must have a concrete invalidation/acknowledgement contract and source path, or an exact externally blocked dependency.
7. Reconcile the compatibility ledger and write a single system checkpoint mapping every implemented path, dependencies, remaining protocol variants and prepared acceptance scenarios. Then continue the implementation gaps within this goal; do not stop after the first adapter or table.

Use cohesive immediate explicit-path commits. Never stage/commit the entire shared checkout. A bounded slice may finish without asking for another assignment: move to the next dependency above. If an actual design decision or external access blocks one item, state the precise blocker and continue independent source work.

## Execution and release gates

These goals authorize source work and commits only. No new builds, tests or test cases/files, linters, formatters, Python imports, generator/CLI execution, browsers, graphical/game/socket/receiver/WM/server processes, installs, DB/deploy operations or external messages. Grants from another session do not transfer. No agents. Preserve the untracked patch helper without running it.

Source, compilation, fixtures, transport receipt, WM consumption, authoritative action results and rendered gameplay are separate evidence levels. All newly changed paths remain unverified at those later levels until separately authorized checks run. Full stock-client replacement remains the release gate; neither the pinned first loop nor procedural content nor this WM system closes it.
