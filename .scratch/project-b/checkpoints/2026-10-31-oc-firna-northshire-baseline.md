# OC - Firna Northshire baseline and NaN doodad planes

Claim: graphics track only. Baseline HEAD `36fa08f7` plus Astra's frozen uncommitted
spellbook/quest-log/wrath integration. The owner's dirty `AGENTS.md`, `CONTEXT.md`,
`docs/CONTRIBUTING.md`, `docs/METHOD.md`, `docs/agents/12340-reference.md` and handoffs are
preserved. No server, database, WM, install, combat or Dicetar state was touched.

Character/session: `run-from-identity.ps1 -CharacterName Firna`; boot log
`wrath: boot selection name=Firna guid=0x1522 race=1 class=1 map=0` (Human, Warrior,
Northshire). The old first-roster fallback selected Jeckle before the named-admission build;
this run proves the named path. Binary for the first capture:
`CD97B79EA5833A7D8D4C9A3626A85B074E7637159BCDA7DB6FD786F3C474DBD7` (Astra's build with
named admission and the marker status fix).

Baseline capture: `oc-firna-20261031-133557-344.png`, `connected-run-20261031-133557-729.*`.
Reproducible defects:
- Screen-filling dark planes with a glowing edge obscure the left/centre view; the player is a
  golden, stretched-arm human female; the target frame says `Unknown`/`Dead` and the opened quest
  window is blank. The unit census reports Firna display 50, 4 meshes, held main/off; 84 nearby
  entities, 73 with no geometry; many `display=447` units have 0 meshes. The `Unknown` target and
  blank quest frame are Astra's UI/name/quest area, not this graphics change.

Measurement (`WOW_CAM_DUMP=1`, temporary `WOW_NEAR_OBJECTS=34` probe, then removed):
`connected-run-20261031-141412-154.*` with binary
`C29D5B050E6B59453F94162EF713E61A226ECAA19CBF460C37DFADB48D46034C`. The camera dump shows
`open=14.607017 coll=14.607017 frac=1.000000` at `pos [128.572418,91.456383,8960.761719]`:
the boom cast misses (open equals the boom). The near-object probe found 10,930 tagged objects
and its nearest entries are `world/azeroth/elwynn/passivedoodads/lamppost/lamppost.m2` at
`NaN yd`; their `GlobalTransform`s are non-finite. The NSabbey WMO itself has camera collision
(2781 camera tris in group 000 and thousands across groups), so this is not a missing WMO
camera collider. The corrupt placements are custom `wm-hd` ADT doodads carrying NaN transforms;
NaN reaches Bevy transform propagation and the collider weld, drawing the giant planes and
placing their colliders nowhere useful.

Repair in progress (not yet committed): skip a placement whose translation/rotation/scale is not
finite in `benilla-world/src/terrain_stream/spawn/mod.rs`, and treat a zero/non-finite stored
MODD orientation as identity in `benilla-assets/src/coords.rs`. Focused test
`coords::tests::a_zero_stored_orientation_is_identity_not_nan` passes; `cargo check -p
benilla-assets -p benilla-world` passes.

Blocker: the shared tree currently fails to build `benilla-app` with E0597 in Astra's
uncommitted `crates/benilla-app/src/ui_quest_log.rs:1075` (`assets` borrow lifetime). That file
is Astra's; it was not edited. A rendered before/after check of the NaN guard needs the next
successful build of `benilla`.

Next: after Astra's `ui_quest_log.rs` compiles, build `benilla`, rerun the bounded Firna capture
(same location, no probe), and compare the giant planes. Then trace the player body/equipment:
Firna's stretched golden arm and the missing starter armour/held-item appearance, keeping the
`Unknown`/blank-quest UI with Astra.

Astra commit-snapshot update: the E0597 blocker above is resolved. The loader binds its
result before matching, and the subsequent Windows `cargo build -p benilla` passed
(`astra-quest-build-fixed.log`). Binary SHA-256:
`12E8E17F90B3C245EEC621F0349A88D2E6EAF40D2B6DDA10A8BB8593C2A3A5BB`.
The owner's subsequent Firna screenshots still show severe stretched geometry and an
`Unknown`/`Dead` questgiver label. The placement guard is not a visually accepted repair;
the shared commit preserves work in progress and does not establish playable alpha.

## 2026-10-31 Player skinning repair

The torn character is the **player**, not the guard. Firna is the human-female display; the
guard is the intact human male. `wm-hd` replaces `Character\Human\Female\HumanFemale.m2` with a
7.9 MB custom HD rig (stock 1.5 MB) and its skin with 238 KB (stock 90 KB). During the guard
interaction the palette writer saw a non-finite joint: `benilla_world::rig_palette` warned
`rig 299 bone 39: non-finite joint; bind pose used`. The finite-but-wrong HD rig, not the
guard or the placement guard, produced the stretched golden body/arm planes.

Repair:
- `benilla-world/src/rig_palette.rs`: `write_rig`, `write_rig_worlds` and `write_rider` now
  replace a non-finite joint/rider matrix with the bind-pose row (identity) and warn once. Test
  `rig_palette::tests::a_non_finite_joint_uses_the_bind_pose_row` passes.
- `benilla-formats/src/chain.rs` + `benilla-assets/src/lib.rs`: `character/` paths read the
  lowest-priority stock archive (`read_base`), bypassing the custom WM character rigs and their
  skins/textures until that rig is supported. Test
  `chain::tests::the_base_character_model_is_the_stock_not_the_wm_patch` passes.

Rendered checks (binary
`72FFD47B73F73CE4B5C4DA1E1FBA31FA368FC76DE9390A5EE232D89349967B7D`, HEAD `52b9af2c` plus this
patch):
- `oc-firna-verify-20261031-161752-867.png`: Firna is the stock human female with orange hair,
  white shirt, tan pants/boots; coherent body; the guard and `?` marker are intact.
- `oc-firna-verify-20261031-162145-096.png`: stock Firna in the abbey courtyard; coherent body;
  the guard and `?` marker visible; `nonfinite=1` was sanitized by the palette guard.
- The D-key probe used to change the camera angle moved Firna about 20 yd into the courtyard;
  that test movement is disclosed and not repeated. The latest capture has no open quest window,
  so the talk state itself still needs an owner-driven confirmation.

Remaining: the `Unknown`/`Dead` questgiver label and the blank/absent quest frame are Astra's
UI/name area; the HD character rigs are bypassed rather than supported; the stock talk/emote
still produced one non-finite joint, now held at the bind pose. No playable-alpha claim follows.
