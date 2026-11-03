# Historical ownership record

Preserved during documentation maintenance on 2026-11-02, including pre-existing dirty
instructions. The following is a historical snapshot, not an active build/client lock or
current task assignment. Current reservations belong in docs/agents/active-ownership.md.

---

# Active Project B ownership

Owner instruction, 2026-11-01: Project B uses a **unified single-owner per-ticket workflow**.
Any active agent may touch any crate in the workspace (`benilla-app`, `benilla-world`, `benilla-ui`,
`benilla-formats`, `benilla-m2`, `benilla-assets`, `benilla-protocol-wrath`, `benilla-hotpatch*`)
required to complete the active ticket end-to-end. The previous Astra-vs-OC crate partition is
retired so cross-cutting fixes (wire -> state -> ECS -> M2/shader -> FrameXML) never stall at
artificial file boundaries.

## Sequencing & Architecture Rules

1. **Track Sequencing**:
   - **Phase 1 (Immediate Priority)**: Finish the playable Northshire alpha route end-to-end:
     - Roaming creature visual vs. authoritative position synchronization (`SMSG_MONSTER_MOVE`
       smooth/Catmull-Rom flags, create-block `SPLINE_ENABLED` paths, stop position snaps) and
       NPC `Unknown` / `Dead` state resolution (`NameCache` / `GameObjectTemplates` hooks on create).
     - Spellbook & hotbar cast-state gate cleanup (`PendingCast` clearing on instant casts,
       `SMSG_CAST_FAILED`, and self `SMSG_SPELL_FAILURE` so `"Another action is in progress"` never sticks).
     - FrameXML / UI parity (Character PaperDoll, Talents, Vendor/Merchant, Quest Log, Bags/Inventory).
     - Camera-dependent magenta lighting & v264 M2 external `.anim` track / header offset fixes.
   - **Phase 2**: Expand the hot-pluggable module architecture (`benilla-hotpatch-abi` &
     `benilla-hotpatch`) across five swappable seams:
     - `PoseModuleV1` (`pose-<sha1>.dll`): post-animation skeletal pose & attachment adjustments.
     - `MovementModuleV1` (`movement-<sha1>.dll`): pluggable movement physics/controller (standard WoW vs. Souls inertia/dodge).
     - `CombatModuleV1` (`combat-<sha1>.dll`): pluggable combat state machine (standard tab-target vs. Elden Ring / Souls stamina, poise, dodge i-frames, and weapon hitboxes).
     - `SpaceModuleV1` (`space-<sha1>.dll`): pluggable procedural terrain chunks, decorations, dynamic mob spawns, and world item spawns.
     - `StoryModuleV1` (`story-<sha1>.dll`): pluggable custom quest chains, dialogue trees, and objective trackers.
   - **Phase 3**: Implement the Souls / Elden Ring Combat module (`Funny-Bones/ELDEN-RING-Combat-Rewrite`
     reference) and Procedural Terrain / Spawner / Story modules on top of the Phase 2 ABI without
     breaking the baseline client.

## Current ticket

2026-11-01 bug split requested by owner: OpenCode managed continuation
`call_5380cc6ac3844b74a45cdf5465d64056` is assigned four CVar repairs under
`.scratch/project-b/handoffs/2026-11-01-opencode-bug-fixes.md`. It owns app
`cvars/table.rs`, UI `script/cvars.rs`, and their CVar tests. This explicitly
supersedes that session's earlier read-only reservation. Gemini's four fixes are
prepared in `2026-11-01-gemini-bug-fixes.md` (not launched): `glue_select.rs`,
app `ui_script/glue_mgr.rs`, UI `tooltip_item/`, `wrath_natives.rs`, and their
focused tests. Astra retains shared build/client coordination; neither assignment
may run Cargo or a client independently. Preserve existing dirty source. Manual OC
session was inspected idle after its cursor commit/web merge; no Cargo or client
was running at this assignment. Current HEAD is `8807107f`.

2026-11-01 23:16 Moscow continuation: Astra coordinates login -> realm -> roster ->
creation/cancel -> world entry. Manual OC session `ses_e67532ec7fferWEL6rThWdiGOY`
acknowledged that Astra owns `cvars/table.rs`, `script/cvars.rs`, `glue_select.rs` and
login/world-entry seams. OC retains only `wrath_natives.rs` cursor work, now complete;
its targeted six tests passed and Cargo slot was released. No further OC build/client.
AutoSwarm `call_14bd8c3d02904f618719b6c3ff2f23eb` owns a read-only API contract review.
Astra launched existing binary `D9EB9F3F...C26B24`, PID 53268, for revalidation before
rebuilding. Login visibly has no modal, but backdrop is black. Owner credential handoff
is pending. This reservation supersedes the stale build reservations below; no competing
client or Cargo job. A later rebuild must include OC's source-only GetCursorInfo repair.

2026-11-01 follow-up screenshots: Astra owns the login CVar initialization, Wrath
creation return contracts/catalogs, ModelFFX methods/preview consumption, and world-entry
loading-screen release/art repairs. Build reservation: `astra-glue-followup-build.log`.
The merchant owner must leave these paths and this Cargo slot alone until release.
The merchant run made mechanical ModelEffects export / integer-width compilation fixes;
those edits are preserved. Do not launch a client from the previous binary for acceptance.

2026-11-01: OC is stopped by the owner. Astra resumes the reported login/realm-screen
errors and artwork repair: glue_core.rs, glue_select.rs, glue_realm.rs, Cfg_Categories
loader, selected-profile UI texture consumers as needed, and their existing app seams.
The shared Cargo slot is reserved for this bounded repair. No competing client or build.

Astra: review-contract repairs in UI attributes/container/action and app action dispatch;
interactive Wrath login/realm/roster/create/delete integration and launcher tools.
This is a per-ticket reservation under the unified workflow, not a permanent crate partition.
No client was running at task admission; the earlier Firna reservation has expired.
The integration rebuild passed; the shared Cargo slot is released. No client was launched.
Origin `511369d7` has been pulled, preserving its movement/combat/graphics/module work
and the local login changes. Source changes remain uncommitted for review.

Collaborating Engineer: Vendor / Merchant FrameXML 3.3.5a Parity.
File ownership: `crates/benilla-ui/src/script/merchant.rs` and associated tests.
Scope: 12340 reference parity for `GetMerchantItemInfo` (7-value return), `GetMerchantItemCostInfo`,
`GetMerchantItemCostItem`, `ExtendedCostView`, and invalid-index return tuples matching
`WoW/Source/Ui/MerchantFrame_Scripts.cpp`. Zero overlap with Astra's uncommitted files or
OC's reserved combat/action-bar areas.
Status: COMPLETED. Targeted merchant unit tests passed (26/26). Handoff documented in
`.scratch/project-b/handoffs/2026-11-01-ce-merchant-framexml-parity.md`. Build slot released.

## Shared Live-Session Rules

- **Human Live Session Reservation**: Firna (human warrior, GUID `0x1522`, dedicated existing account).
  Credentials stay in the ignored identity mechanism; no account/server/DB/WM/install modifications.
  Never launch a competing graphical client or automated login while the owner's live session is open.
- **Minimal Diagnostics**: Run only targeted `--lib` unit tests or lightweight offline examples for the
  crates touched (at most 20 essential tests per task; no workspace-wide test churn or unrequested
  headless harness runs).
- **Backups & Hygiene**: Keep at most three previous versions/backups, preserve evidence, and never
  duplicate the full `target/` directory or worktree.
