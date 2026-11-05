# Active Project B ownership

Use one accountable owner per ticket, across whichever crates are needed. Permanent
Astra/OC crate partitions are retired. Standing coordination and execution rules are in
[workflow.md](workflow.md); this file records assignments and resource reservations.

## Current assignments to revalidate

Snapshot reconciled on 2026-11-02 from existing reservations and recorded outcomes.
A reservation is not proof that a process is still running. Inspect live state before
editing, building or launching. Preserve dirty source from all owners.

| Owner / task | Files or responsibility | Last recorded state |
|---|---|---|
| Astra / integration coordinator | Login, realm, roster, creation/cancel, world-entry integration; shared build/client coordination | Runtime follow-up and integration remain open; see next-batch handoff |
| OpenCode / CVar repair | app cvars/table.rs, UI script/cvars.rs and CVar tests | AutoSwarm call call_5380cc6ac3844b74a45cdf5465d64056 completed; integration/backend gaps remain reported |
| Gemini / proposed contract fixes | glue_select.rs, app ui_script/glue_mgr.rs, UI tooltip_item/, wrath_natives.rs and focused tests | Prompt prepared; launch/acceptance not established. Inspect actual work before assigning |
| Merchant contributor | UI merchant.rs and associated tests | Reported complete in merchant handoff; build slot released |

Manual OC session ses_e67532ec7fferWEL6rThWdiGOY previously acknowledged Astra's integration
reservation and completed cursor work. Read-only AutoSwarm review
call_14bd8c3d02904f618719b6c3ff2f23eb has recorded findings; inspect its result before repeating it.
Do not infer that manually opened sessions are controlled by AutoSwarm.

## Build and client slots

At the documentation-maintenance admission on 2026-11-02, no benilla/Cargo/rustc process
was observed. This is a snapshot, not a persistent free-slot guarantee. This docs task
does not reserve or start a build/client. The previous dated build/PID reservations are
historical and must not be reused as active locks.

Before reserving a slot, inspect processes and coordinate with the task owner. Record:

| Resource | Owner | Task / command and evidence log | State |
|---|---|---|---|
| Cargo | Astra | `cargo build -p benilla --features hotpatch`; `.scratch/project-b/build-2026-11-02-astra-pose.log` | Passed; slot released |
| Client | Astra / owner | Run `20261102-222346-059`, PID43304, SHA256 `1D853A073F916C1CBD952FA21E36ACB6BA960C29486B919CD2F6A7A13B3B43BC` | Owner entered Northshire as Firna; visible world/combat observed, do not interrupt or compete |

Update the reservation when acquired and release it on completion. Do not overwrite a
newer reservation or kill another owner's process to acquire the slot.

## Current evidence and continuation

- Codex U4 (2026-11-05): native multiclass Request/Poll transport, scoped preview
  receipts, operations 6/7/8, Glue load and server-owned trainer grouping prepared
  on `feat/wrath-multiclass-host`, based on the P0 inventory/session integration at
  `39e3c62a`. Source-only handoff:
  [multiclass host](../architecture/multiclass-host-2026-11-05.md). No build/client
  slot reserved; compilation and installed-addon/runtime acceptance remain open.

- D00 default-on recording approved by owner on 2026-11-03, with D01–D07 and Q00–Q04
  correlation and lifecycle continuation added in `crates/benilla-app/src/quest_trace.rs`,
  `target/click.rs`, `ui_quest/{mod,net,verbs}.rs`, `ui_quest_log.rs`, `wrath.rs`, `net.rs`,
  `net/io.rs`, and `debug_panel/mod.rs`. See
  [resident quest diagnostics](../architecture/rfc-resident-quest-diagnostics.md).
  Source now includes run-local `npc_alias` redaction, `Open`/`Select`/`Accept`/`RequestReward`/`ChooseReward`
  panel and server-refusal correlation (`ServerQuestLogFull`, `ServerQuestInvalid`, `ServerQuestFailed`),
  scope retirement on disconnect/world transfer, reload-aware journal projection evaluation
  (distinguishing `JournalTemplatePending` / `JournalTemplateRefused` from `JournalRowInModel`
  mismatches), `QuestSlotProgressed` objective counter tracking (Q03), asynchronous turn-in
  delta correlation across `QuestCompletePacket`, `QuestSlotRemoved`, `PlayerXpChanged`,
  `PlayerMoneyChanged`, and `RewardItemPushed` (Q04), `SnapshotV1` header version validation,
  and 9 focused acceptance unit tests in `quest_trace::tests`. Visible pixel rendering and
  target-client runtime validation remain open (Q05).

- Codex six-task batch (2026-11-03): G01–G06 source/design deliverables prepared; G03 focused
  unit test `item_info_rejects_oversized_stat_count_without_cursor_desync` added in
  `crates/benilla-protocol-wrath/src/gameplay.rs`. Batch evidence and exact checks:
  `.scratch/project-b/handoffs/2026-11-03-g01-g06-batch.md`.

- Codex V3 continuation (2026-11-03): G00 baseline recorded and H00 manifest retry source
  repair + focused unit test `poll_retries_unchanged_manifest_after_transient_load_failure`
  in `crates/benilla-hotpatch/src/lib.rs`. Live same-PID A→B→A runtime swap remains open (H01).
  See `.scratch/project-b/handoffs/2026-11-03-v3-admission-h00.md`.

- Astra recovery: cached self-position deduplication in `wrath.rs` and ordinary pose correction
  no longer requests mover handover in `player/state.rs`. Two focused `self_pose_` tests pass;
  runtime acceptance pending. Detailed live handoff: `.scratch/project-b/handoffs/2026-11-02-astra-deepseek-live-handoff.md`.
- DeepSeek support `call_24773bde2ded47c0a3423ea012d6e9b3` completed completion-matrix and
  CVar-backend review; recovery receipt `call_b5c695e824dc4b9fae08f5e554546b1f` acknowledged.
  DeepSeek is idle; no production source or build slot assigned to it yet.

- Local `.scratch/project-b/handoffs/2026-11-02-next-batch.md`: prioritized work,
  known source changes, unproven behavior and acceptance order. Revalidate its baseline.
- Local `.scratch/project-b/handoffs/2026-11-01-astra-next-session.md`:
  prior source/build findings and AutoSwarm setup context; historical snapshot.
- [Ownership record](../../.scratch/project-b/handoffs/2026-11-02-ownership-record.md):
  preserved earlier reservations, including the pre-existing uncommitted instructions.
- Runtime evidence is under the owning handoff's referenced verification/run paths.
  Implemented APIs, compilation, model probes and visible gameplay are separate evidence.

Local handoffs may be untracked or absent from a fresh clone. Request the relevant evidence
when unavailable; do not infer its contents or treat a dated baseline as current state.

## Acceptance and scope

Deliver the private Windows login/realm/roster/create-cancel/world-entry workflow, then
Firna's Northshire playable route. Resolve the current runtime blocker before expanding
the module platform or Souls/procedural/story features. Adopted sequencing is recorded in
[owner decisions](../baseline/owner-decisions.md); module interfaces are in
[ADR-0002](../architecture/adr-0002-live-client-modules.md), not implementation guarantees.

The owner enters credentials and drives combat. Respect the Firna live-session reservation;
no competing account login, automated combat or persistent character creation/deletion.
Use authorized essential targeted checks within the 20-case task ceiling, not workspace churn.
Keep at most three completed backup generations and preserve evidence.
