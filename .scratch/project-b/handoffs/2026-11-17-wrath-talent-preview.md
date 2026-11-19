# Wrath talent preview and LearnPreviewTalents

Status: ready to merge
Owner: OpenCode / DeepSeek V4.1 Flash
Branch: fix/wrath-talent-preview
Issue: none
Next: owner batch build; then preview/unspent checks below. Source review is complete; the branch merged `origin/main` `ce34ef95` and is 0 behind at push time, and no Cargo, test or client run was made.

The stock 3.3.5a `previewTalents` CVar was wired to constants in `script/talent.rs`:
`AddPreviewTalentPoints`, `GetGroupPreviewTalentPointsSpent`, the reset verbs and
`LearnPreviewTalents` did nothing, so shift-clicking a talent only greyed a button. This branch
implements the client-side preview map and the learn batch. The CVar stays off by default
(`crates/benilla-app/src/ui_script/lifecycle.rs:77`).

## Reference

- 12340 `Ui/TalentInfo_Scripts.cpp`: `AddPreviewTalentPoints` (`0x5c9590`, clamps inside the
  engine and signals `PREVIEW[_PET]_TALENT_POINTS_CHANGED` with `talent+1, tab+1, group+1,
  after-before`), `GetGroupPreviewTalentPointsSpent` (sums the group's tab preview counts),
  `ResetPreviewTalentPoints` / `ResetGroupPreviewTalentPoints` (same event shape, zero for the
  cleared index), `LearnPreviewTalents` (`0x5c6a10`: active group's items with
  previewRank > rank, tab/tier order), and `GetTalentInfo`'s 10 values
  (`0x5c7800`: 9th = preview rank, 10th repeats the required-spell availability) and
  `GetTalentPrereqs`' four values per prerequisite (`0x5c7ed0`, 4th = preview verdict).
  `GetTalentLink`'s 6th argument selects the preview rank (`0x5c7ce0`).
- AC `SkillHandler.cpp:34` (`HandleLearnPreviewTalents`) and `PetHandler.cpp:1119`
  (`HandleLearnPreviewTalentsPet`): player body `u32 count` + `(u32 talentId, u32 rank)` pairs;
  pet body prefixes the raw `u64` guid, then the same counted pairs. Both cap the count
  (150/30) and resend `SMSG_TALENTS_INFO`.
- Stock `Blizzard_TalentUI.lua` / `TalentFrameBase.lua` read the 5th `GetTalentTabInfo` value,
  the 9th/10th `GetTalentInfo` values, and pass the 4-value prereq tuples to
  `TalentFrame_SetPrereqs`.

## Source change

- `WrathTalentUiState::preview_ranks` keys deltas by `(pet, group 0-based, tab 1-based,
  index 1-based)`. `set_wrath_talents` clears it, and `WrathTalentState::snapshot_seq` is bumped
  by every `SMSG_TALENTS_INFO` and involuntary reset. The feed compares that sequence as well
  as the built views, so an identical packet after a refused learn still reaches the VM and
  drops the preview; `LearnPreviewTalents` itself leaves the map for the server ack.
- `AddPreviewTalentPoints` clamps the preview rank to `known..max`, stores only deltas above
  the known rank, and signals the player/pet preview event with the reference's four arguments.
  `GetGroupPreviewTalentPointsSpent` and `GetTalentTabInfo`'s 5th value sum the deltas in
  scope; `ResetPreviewTalentPoints` and `ResetGroupPreviewTalentPoints` clear the tab/group and
  signal only when the total changed.
- `GetTalentInfo` returns `rank + delta` as the 9th value (10th stays the required-spell
  verdict); `GetTalentPrereqs` adds the preview verdict as the 4th value per prerequisite;
  `GetTalentLink` uses the preview rank when its 6th argument is true.
- `LearnPreviewTalents(pet)` collects the previewed pairs in tab/index order and submits one
  `TalentCall::LearnPreview`. The app resolves the pet guid from the player's
  `UNIT_FIELD_SUMMON`, the summoned pet (`Player::LearnPetTalent` compares against `GetPet()`,
  `Player.cpp:14120`; a charmed NPC is not the pet talent unit), and sends
  `CMSG_LEARN_PREVIEW_TALENTS` (`0x4C1`) or `_PET` (`0x4C2`). The 1.12 donor writer drops the
  command unchanged.
- `TalentCall` is no longer `Copy` because the batch owns a `Vec`; the call queue clones.

## Focused cases (written, not run)

- `wrath_preview_points_feed_the_tuples_and_the_learn_batch`
  (`crates/benilla-ui/src/script/tests/talent.rs`): add/reset totals, the 5th/9th values, the
  4th prereq value with a required rank of 3 (one preview point is not enough, the second is),
  and the one `(talentId, previewRank)` call.
- `an_identical_talent_snapshot_still_clears_previews` (`crates/benilla-app/src/ui_talent.rs`):
  the view-equality gate alone would skip `set_wrath_talents`; a new `snapshot_seq` forces the
  push and therefore the preview clear.
- `learn_preview_talents_sends_the_counted_pairs_inside_its_call`
  (`crates/benilla-app/src/ui_talent_call_tests.rs`): the player batch and the pet batch with
  `UNIT_FIELD_SUMMON`'s guid reach `ClientCommand::LearnPreviewTalents` inside the call; the
  fixture also sets a different `UNIT_FIELD_CHARM` guid, which must not win.
- `learn_talent_and_pet_orders_match_12340_wire_layouts`
  (`crates/benilla-protocol-wrath/src/gameplay.rs`): player and pet `LearnPreview` bodies.

Four focused cases (one new, three extended); nothing else was added or run.

## Named limits

- `GetPreviewTalentPointsSpent` stays 0 exactly as 12340's function does; the group verb is the
  real reader.
- The feed clears the whole preview map on a pet snapshot too; the stock client resets only
  the pet group (`UpdatePetTalentData`) or the player groups (`UpdateTalentData`). The stock
  frame shows one scope at a time, so this is a narrow over-clear rather than a mixed-scope
  preview.
- `SetActiveTalentGroup` is still a stub; this branch only reads the active group.
- The preview event's group argument is 0 for pet, where the reference pushes the player's
  active group + 1; the stock handler comments those arguments out.
- No compile, test, CI or client run was made on this branch.

## Owner checks

1. Batch build (sccache) and the four focused cases.
2. At a trainer, enable `previewTalents`, shift-click/add a talent, confirm rank and tab preview
   points rise, and right-click/reset drops them.
3. `LearnPreviewTalents` spends exactly the previewed pairs; confirm the server's
   `SMSG_TALENTS_INFO` (new ranks, unspent) replaces the preview. Repeat with no unspent point
   so the learn is refused: the identical snapshot must still clear the preview, not leave the
   phantom points.
4. Hunter pet: preview a pet talent and learn it; confirm the pet packet carries the live pet
   guid and the pet tab updates. With a charmed NPC present, the packet must still carry the
   summoned pet, not the charm.
