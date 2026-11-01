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

## Shared Live-Session Rules

- **Human Live Session Reservation**: Firna (human warrior, GUID `0x1522`, dedicated existing account).
  Credentials stay in the ignored identity mechanism; no account/server/DB/WM/install modifications.
  Never launch a competing graphical client or automated login while the owner's live session is open.
- **Minimal Diagnostics**: Run only targeted `--lib` unit tests or lightweight offline examples for the
  crates touched (at most 20 essential tests per task; no workspace-wide test churn or unrequested
  headless harness runs).
- **Backups & Hygiene**: Keep at most three previous versions/backups, preserve evidence, and never
  duplicate the full `target/` directory or worktree.
