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
