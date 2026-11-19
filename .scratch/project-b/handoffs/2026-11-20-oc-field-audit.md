# Land or close the stale field audit

Status: landed
Owner: OpenCode / DeepSeek V4.1 Flash (landing the argon branch)
Branch: argon/field-mapping-audit
Issue: none
Next: none; landed on main as 6d488fae (merge 8b95a553).

## What changed

The branch's single commit (04c16dbc) projected GameObject, DynamicObject, Corpse and extra
Unit/Player fields into the 1.12 descriptor. Main has since gained the live field table
(`wrath_field_map.toml`) and the generic `raw_fields` digest window, so this merge keeps
main's data-driven projection and drops the branch's duplicate `push_unit_fields`,
`push_gameobject_fields`, `push_dynamicobject_fields` and duplicate `push_corpse_fields`.

Offsets checked against the 12340 reference
(`D:/Projects/wow/reference/wowdecomp-reference`, pin `e67b6ffe`):
`GAMEOBJECT_DESCRIPTORS` gives parent rotation 10..13, dynamic 14, faction 15, level 16 and
bytes_1 17; `CORPSE_DESCRIPTORS` gives display 10, items 11..30, bytes 30..36. Main already
covers the corpse shift and every unit word. Kept from the branch because main lacked them:
gameobject rotation and the art-kit/anim-progress bytes of `GAMEOBJECT_BYTES_1`, the honor
counters at `PLAYER_FIELD_KILLS` 1225..1230, and the `FieldChanged` edge for
`FIELD_GAMEOBJECT_STATE` / `FIELD_CORPSE_DYNAMIC_FLAGS` on the snapshot adapter.

Named tests: `wrath_player_honor_fields_sit_in_the_1_12_honor_block`;
`shipped_gameobject_rules_carry_rotation_and_bytes_1`.

## CI and limits

CI 44477619887 (on 4dc98a95): the Windows check passed and the gates `test` step passed,
including `wrath_player_honor_fields_sit_in_the_1_12_honor_block` and
`shipped_gameobject_rules_carry_rotation_and_bytes_1`. The gates job's only red step was
`player-clippy`, on pre-existing findings in untouched lines (wrath.rs:9019/9020/9850/11985 and
other files); the package says clippy findings in lines the ticket did not write are ignored.
The branch then merged origin/main 881f9385 (souls-only); CI 44481364607 on 8b95a553 had the Windows check and tests green, with the same pre-existing player-clippy red only. Landed as 6d488fae.
Not proven: no client run; the rotation/art-kit values are source checks against the table and
the 12340 descriptor only. The 1.12 position words stay unwritten; movement applies the entity
transform, as on main.
