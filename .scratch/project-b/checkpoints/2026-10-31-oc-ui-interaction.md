# OC - stock UI clicks, action APIs and the rig local write-back

Canonical repository is now `https://github.com/Dicetar/RustWrath`, branch `wotlk`
(`48fa3dea` at the start of this package). The local `origin` already points there; `upstream`
remains benilla. Owner's reference-workflow documents and Astra's uncommitted hotpatch/animation
work are preserved and were not staged.

## Stock UI interaction

The read-only Firna action-bar probe (`oc-actionbar-20261031-230213-375`) showed the live bar is
populated: page 1, `ACTIONBUTTON1` bound to `1`, slot 1 Battle Shout, slot 2 Attack
(`Ability_Marksmanship` icon), slots 3-12 empty. The user's play session then exposed the exact
blockers in `connected-run-20261031-232449-545.stderr.log`:

- `bindings(ACTIONBUTTON1/2): SecureTemplates.lua:537: attempt to call global
  'SpellCanTargetItem'` — the secure action button died before `UseAction`, so Battle Shout and
  Attack did nothing by click or key.
- `LootButton1/2/3:OnClick: bad argument #1 to IsModifiedClick (string expected, got no value)` —
  the loot buttons died before `LootSlot`, which is why the loot frame opened but item clicks did
  nothing. The same no-argument call broke the spellbook `SpellButton` clicks.
- `MainMenuBarBagButtons.lua:132` and `LootFrame.lua:168` could not find the
  `displayFreeBagSlots`/`lootUnderMouse` CVars, aborting those panels' setup.

Changes:
- `benilla-ui/src/script/action.rs`: add `GetActionInfo(slot)` (`spell`/`item`/`macro` and the id;
  empty slot returns nothing) and `GetMultiCastBarOffset()` (0, no multicast bar). Six focused
  action tests pass, including the two new cases.
- `benilla-ui/src/script/wrath_natives.rs`: `IsModifiedClick` accepts the stock no-argument call
  (any held modifier; no modifier answers nil) and add `SpellCanTargetItem` (false: no
  spell-targeting cursor item) so the secure handlers reach `UseAction`. The focused
  `is_modified_click_reads_the_held_modifier` test passes.
- `benilla-app/src/cvars/table.rs`: register `displayFreeBagSlots` (1) and `lootUnderMouse` (0)
  so the bag bar and loot frame OnLoad complete.

No client rebuild was made: the running client (`BDD50993...`) predates these source changes and
the current tree is blocked in `crates/benilla-assets/src/m2.rs` by a concurrent animation/source
edit (`E0502`, `ctx` borrowed across `read_asset_bytes`). That file is not this package's.

## Rig local write-back

Astra's handoff finding was correct: `RigPose::compose` sanitized a local copy for `self.model`
but did not write it back, and `rig_anim/compose.rs::rig_worlds` later composes `rig.locals[i]`
again, so a non-finite talk local could re-enter the world palette. `pose.rs` now writes the
sanitized bind local into `self.locals[i]` before both passes. The focused test
`rig_anim::pose::tests::a_non_finite_animation_local_falls_back_to_the_bind_pose` was added but
could not run in this tree because the `benilla-assets` compile error above is upstream of
`benilla-world`; it is not a rig-test failure.

## Remaining

- WASD/`too far away`: the play log shows `wire_in: took control of player ...` every ~2 s; the
  server re-seats the client, so its attack-range check sees the player far from the wolf. That is
  the movement/`wrath.rs` path Astra owns, not the hotbar.
- Character-selection screen and the named boot/roster are the same glue/`wrath.rs` area.
- The model/rig still needs a built client and a rendered check after Astra's animation-source
  edit compiles; this package only writes the local back.
- A client rebuild/reload is still required to exercise the action, loot and bag fixes in game.
