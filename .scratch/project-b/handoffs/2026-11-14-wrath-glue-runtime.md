# Wrath glue and runtime repairs: button label, load timing, 12340 nameplate gate

Status: parked
Owner: Claude Code / Opus 5.5 (stopped 2026-11-14)
Branch: fix/wrath-glue-runtime
Issue: none
Next: take the Cargo slot, build `benilla-ui` and `benilla-app` tests, run the focused cases listed below

Based on `main` `f1a86931`. Pushed to `origin` on 2026-11-15 after a source review; not merged.

## What exists now

| Commit | What | Checked |
|---|---|---|
| `408df3ad` | The XML loader always creates the font string of a `<ButtonText/>`, named or not, before `SetText`, as the native loader does (`CSimpleButton.cpp:939`). Repairs the nil index at `GameTime.lua:141` in OnLoad. | 5 loader cases passed on 2026-11-14 |
| `b0e906f4` | Two timing tests for the 153 s stall between `ChatFrame.xml` and the end of `FloatingChatFrame.xml`: `each_wrath_core_file_loads_in_bounded_time` (`ui_script/manifest.rs`) and `the_chat_face_shapes_a_line_in_bounded_time` (`ui_text/engine/mod.rs`). | Not compiled, not run |
| `919edd7a` | `wrath_plate_gate`: the 12340 `ShouldShowNamePlate` rules on a Wrath VM, and the six `nameplateShow{Enemy,Friendly}{Pets,Guardians,Totems}` rows. The 1.12 path is unchanged. | Not compiled, not run; 8 cases in `vplates/wrath_gate_tests.rs` |
| `0fae83d6`, `739dea3c` | Ownership entry; owner decision that `VoiceChat.xml` stays skipped on Wrath. | Documentation |

## Source review, 2026-11-15 (Claude Code / Fable 5.2)

Reading only; nothing was built.

- The gate was compared branch by branch with `Unit_C.cpp:20408`-`:20520` and `GetCreatorGuid`
  (`:6686`). Category, creep bit, record flag `0x100000`, the two toggles, types 8 and 12, the
  dead flag with `IsRaidMemberOrPet`, the player-owner rules, the unattackable enemy player and
  the 41 yd range with the arena exception all agree, in the reference's order.
- The creep bit is read from byte 3 of the synthesized 1.12 `BYTES_1`; `wrath.rs` moves the
  12340 byte 2 there, so the read is right.
- Every function the new code calls exists on this branch with a matching signature, and the
  owner getters already drop a zero guid. Whether it compiles is still unknown.

To look at when building:

1. The ButtonText change also runs on the 1.12 VM. Only the 5 Wrath-side loader cases ran, so
   run the 1.12 loader cases that cover a button label as well. The older comment above it
   ("`SetText` creates the label even with no text") no longer describes the Wrath path.
2. `!(unit.is_player && !friendly && !unit.attackable)` may draw `clippy::nonminimal_bool`.
3. The timing tests assert on wall-clock time (10 s per core file, 5 s to rasterize one line) and
   one of them loads the whole installed core. They are probes for one stall. Once the stall is
   explained, mark them `#[ignore]` or remove them rather than leave them in the default run.
4. `docs/agents/active-ownership.md` gains an entry here at a place `main` has since edited. It
   will conflict on merge; drop the branch's entry in favour of this note.

## Focused cases to run (within the 20-case limit)

- `benilla-ui`: `unnamed_button_text_is_available_during_onload` and the loader's button-label cases.
- `benilla-app`: the 8 cases in `vplates::wrath_gate_tests`, the existing `vplates::gate_tests`.
- With the install, and `--nocapture` to see the timings: `each_wrath_core_file_loads_in_bounded_time`
  (prints `CORE_FILE_TIME` for any file of 250 ms or more) and
  `the_chat_face_shapes_a_line_in_bounded_time` (prints `FRIENDS_OPEN`, `FRIENDS_SHAPE`,
  `FRIENDS_RASTER`). A slow core file points at the VM or layout; a slow face points at the fonts.

## Not done, not proven

- No build of the last three code commits, no client run. Nothing here has been seen in game:
  the clock button's label, the stall, and plates at 41 yd with the pet, guardian and totem
  checkboxes are all open visible checks.
- Not carried over from the earlier glue session, because both contradict the 12340 reference:
  the `QuestFontNormalHuge` alias and the TOOLTIP2 to TOOLTIP change.
- `MoneyFrame.lua:274` appeared only in the 12 Nov run and is unverified. The `cursorOffset`
  error-handler failure is probably repaired by `680c6eac`; recheck in the next client run.
- The `ChannelFrame.lua:716` errors and the `VoiceChatSpeakerTemplate` warnings are expected
  under the owner's decision to keep the skip. Do not stub them.
