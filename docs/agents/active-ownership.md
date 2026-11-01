# Active Project B ownership

Owner instruction, 2026-10-31: Astra leads graphics fixes and the hotpatch/module system,
including integration seams. OC continues his existing playable-combat work. This supersedes
the earlier reservation of rig scheduling files to OC. One writer per file; preserve existing
dirty work and coordinate exceptions explicitly.

## Astra

- Graphics, M2/animation/rig math, palettes, attachments, shaders, asset/profile behavior.
- Native module ABI/host/publisher, shader watch and dependency-aware asset reload design.
- `benilla-world/src/rig_anim/{pose,compose}.rs`, `rig_anim.rs`, app `hotpatch.rs`, app/launcher
  Cargo features/registration, new hotpatch crates and associated docs/tools.
- Adopt and preserve OC's pending finite-local repair, including his writeback into locals.
- Current owner request: pull Argon's 27d3b09d, preserve local OpenCode edits, rebuild and
  launch Firna for the human. Astra reserves this integration/build/run slot; no competing
  build or account login. A pre-integration source/history backup is preserved outside the
  checkout; automatic backups run every eight hours (docs/agents/backups.md).
- External-animation repair claim: `benilla-formats/src/models/anim.rs`, its root re-export,
  `benilla-formats/examples/animation_sources.rs`, and `benilla-assets/src/m2.rs`.
- New owner-reported graphics blocker: roaming creature visuals remain at old positions while
  their authoritative positions move; attack range follows the latter. Astra owns diagnosis
  of transform/rig/visual synchronization, coordinating any shared entity-state seam with
  OpenCode. No source repair or live resolution is claimed yet. Magenta lighting remains open.
- Current movement claim: new `benilla-protocol-wrath/src/gameplay/movement.rs`, the minimal
  event/opcode dispatch additions in `gameplay.rs`/`opcode.rs`, and `wrath.rs` movement-event
  translation. Astra also handles Wrath NPC flag projection in `wrath.rs`; raw Wrath vendor
  bit 0x80 currently becomes the donor innkeeper bit. OpenCode should leave these edits to
  Astra while continuing cast/action/inventory/loot UI work.
- Lighting claim: `benilla-formats/src/models/records.rs` and `examples/light_sources.rs`.
  The actual zero-light HD model decoded 37,415 fake lights before the version/layout repair,
  now zero. Camera-dependent magenta still needs same-scene rendered acceptance.

## OC

- Owner's 2026-11-01 live report: spellbook actions sometimes work, sometimes fail with
  "another action is in progress"; action bars, inventory opening and looting still fail.
  The owner has sent this directly to OpenCode. These remain combat/UI-owned blockers;
  the quest/combat/loot/turn-in route cannot yet be accepted.
- OpenCode owns the cast-state gate in `spellbook.rs`/`ui_cast` for this combat task, regardless
  of earlier authorship. Do not redirect it to Astra solely because she previously edited it.
- Binary correction: both human runs used SHA-256 5F4BD0DABE7B1252D9E67A62CD00BCC9013D3936112164C2661A67F6CD46B50A,
  built successfully with 476cbb96/6edea9f5 and the external-animation repair. The 00:29 run
  metadata pins 9c9fb301. The build log `astra-playable-graphics-build-fixed.log` records success.
  3dec692e makes loader ownership more explicit; its absence cannot explain those UI failures.
- Continue playable combat: input/targeting/attack/cast packet-to-state-to-UI behavior and the
  quest/loot/reward/persistence route, with actual combat driven by the human owner.
- Fix Unknown/Dead NPC name/health projection where it blocks interaction/targeting/combat.
- `names.rs`, `ui_unit*`, combat/target/input owners, appropriate protocol gameplay handlers;
  `wrath.rs` only for necessary state/command translation, after recording the exact claim.
- Avoid graphics, rig, shader, profile, module, Cargo/lockfile and app-registration edits while
  Astra owns them. If a combat blocker is in a graphics-owned file, report the concrete seam.

## Shared rules

Firna: human warrior, GUID 0x1522, dedicated existing account. Credentials stay in the ignored
identity mechanism; no account/server/DB/WM/install modifications. No automated combat.
At most 20 essential tests per task, no broad suite churn. A passing test/build does not prove
rendered or playable acceptance. Keep at most three previous versions/backups, preserve evidence
and do not duplicate the full target directory or worktree. Coordinate runs as well as source.

Current OC prompt: `.scratch/project-b/handoffs/2026-10-31-astra-lead-oc-combat.md`.
