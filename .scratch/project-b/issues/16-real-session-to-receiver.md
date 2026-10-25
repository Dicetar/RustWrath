# 16 — Real session to native receiver

**Status:** ready-for-agent

**Plan reference:** C01. Owner approved ticket sizes and blockers on 2026-10-25. See [the approved completion queue](../completion-index.md).

**Implementation state:** PARTIAL — bridge wiring is committed at 827ce851; receiver acceptance and lifecycle evidence remain open.

**What to build:** A real lab world session delivers its own typed observations to the native receiver while baseline login remains usable with the bridge disabled.

**Blocked by:** None outstanding in the new queue. The existing reducer/producer and bridge checkpoint 827ce851 are prerequisites already delivered; pin the current receiver checkpoint before execution.

**Related existing issues:** [11](11-b008-wm-observation-adapter.md). These are traceability links, not additional blocking edges; existing issues and evidence are preserved.

**Suggested owner:** GLM integrates the application; OpenCode owns receiver fixes.

**Checkpoint update:** GLM's driver/bridge wiring is committed at 827ce851; Astra verified that HEAD and a clean working tree. GLM reports compilation, 28 passing tests and real handoffs with no receiver listening. Preserve this completed work. This ticket remains open for actual receiver handshake/store acceptance and lifecycle evidence; pin OpenCode's Python checkpoint before an authorized run. No receiver/WM process startup is authorized by this update.

## Acceptance

- [ ] Use the actual driver, producer, conversion and publisher; no synthetic observations presented as live evidence.
- [ ] Demonstrate authenticated handshake and receiver/store acceptance separately from HandedOff receipts. Record matching epoch, player, sequence, revision, source time, target and visibility metadata.
- [ ] Show unknown target and an explicitly incomplete visibility set survive conversion without becoming no-target or complete.
- [ ] Exercise receiver absence/recovery, idle publication retry, stale rejection and disconnect invalidation under the existing authorized lab scope. Scope each claim to the cases actually run.
- [ ] Keep the producer as the retry owner and acknowledge only the matching accepted handoff. No socket wait in the packet-processing path.
- [ ] Verify baseline-disabled behavior and preserve the normal clean logout path.
- [ ] Hand off the integrated source identity, exact bridge file set, receiver invocation/configuration requirements and evidence references without credentials.

## Boundaries and handoff

Do not merge unrelated profile-reload/combat/config edits. Do not count a background queued publication as delivery. If execution is unavailable, retain PARTIAL acceptance rather than claiming closure.

Keep the slice within one fresh implementation context. If exploration reveals additional independent behavior, identify the smallest complete child slices before expanding scope. Read the current project context, relevant skill/rules and source checkpoint. Preserve unrelated edits and existing ownership.

Execution checks described above are acceptance requirements, not authorization. Follow explicit permissions applicable to the executing agent. Report source implementation, compilation and runtime evidence separately. An unexecuted required check remains UNKNOWN. Do not commit, deploy, start services or mutate live state merely because this published ticket exists.

Handoff: exact source identity and changed paths; externally visible behavior; evidence actually collected; unresolved acceptance; next dependent slice. Close only after acceptance is supported. See [the completion plan](D:/WOW/PROJECT_B_COMPLETION_PLAN.md) for the mandatory full stock-client replacement release gate.



## Work package 01 update (2026-10-25)

Phase A implemented: R1 knowledge-transition change detection, atomic two-word target knowledge,
R2 real transfer lifecycle (`SMSG_TRANSFER_PENDING`/`SMSG_NEW_WORLD`/worldport-ack with a
loading-phase discard window and `world_generation` scope), session `Phase` state machine,
near-teleport standalone movement. Phase B published:
[producer-proposal-r3-r4.md](../../docs/convergence/producer-proposal-r3-r4.md) (relayed to
`D:/WOW/PROJECT_B_GLM_PHASE_B_PROPOSAL.md`). Phase C feed adapter:
`benilla-app/src/wrath.rs` with the R5 scheduling contract (see issue 22).

Still open on this ticket: receiver handshake/store acceptance with a real receiver run (requires
starting WM-side processes — not authorized in this session), stale-rejection and
disconnect-invalidation cases on the wire, and the R4 ack-channel proposal's implementation by
OpenCode (see the Phase B proposal §4).
