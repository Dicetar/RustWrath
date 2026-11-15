# Wrath stock interface, controls and movement gaps (matrix F04, F14, F15)

Status: in progress
Owner: Claude Code (Opus 5.5)
Branch: fix/wrath-stock-ui-errors
Issue: none
Next: F15 settings natives the stock options panels call (BNConnected, sound driver lists, Restore*Defaults), then the F04 movement gaps below

Date 2026-11-15. Worktree `D:/Projects/rw-worktrees/fix-wrath-stock-ui-errors`, cut from `main`
`9dc36e48`. Nothing on this branch has been compiled or run: the owner builds and launches by
hand after a batch lands. Every claim below is source-level.

## What exists now

- `0b4979a3` diag: a Wrath frame whose geometry getter (`GetCenter`, `GetLeft`, …) answers 0
  because its rect is unresolved is named once in the log (`layout_methods.rs`
  `note_unresolved_rect`), so the stock errors that follow (`MoneyFrame.lua:274` and others that
  do arithmetic on the answer) point at a frame instead of a bare number.
- `a47eaef3` `GetRestState` on Wrath answers the server's rest byte (rested/normal and the
  stock exhaustion names), nil×3 with no player, instead of a constant.
- `125807d5` modified clicks, replacing the constant table in `wrath_natives.rs` (which answered
  SHIFT for `STICKYCAMERA`, stock CTRL, and had no `SetModifiedClick`,
  `GetNumModifiedClickActions` or `GetModifiedClickAction`, so `InterfaceOptionsPanels.lua`
  146/374/465 errored):
  - `bindings_xml::parse_file` keeps the `<ModifiedClick action default>` rows; core and addon
    `Bindings.xml` loads register them (`manifest.rs`, `ui_script/addons.rs`, `script/addon.rs`).
  - `script/keybind/clicks.rs`: chord parse/render per `MODIFIEDCLICK::SetBinding`/`GetBinding`
    (12340 `0x0055E180`/`0x0055E230`), `IsActive` `0x0055D6B0`, `CGUIBindings::IsModifiedClick`
    `0x0055F940`, and the five globals (`UIBindings_Scripts.cpp`). The clicks ride in
    `LoadBindings`/`SaveBindings` sets beside the keys.
  - `Model.click_button` is the reference's `CSimpleTop::m_mouseButtonName`, set around
    `CSimpleButton::OnClick` (`button.rs`) and `OnMouseDown`/`OnMouseUp` (`pointer.rs`), so
    `CHATLINK` (SHIFT-BUTTON1) is not active during a right click.
  - The app stores changed chords as `modifiedclick <CHORD> <ACTION>` lines in the same
    `benilla-config/bindings/*.txt` files (`bindings/store.rs`, `bindings.rs`).
  - Named deviation: shift/ctrl/alt are held without their side, so `LSHIFT` reads as either.

## Focused cases written (not run)

1. `tests/anchors.rs` `an_unresolved_wrath_frame_answers_zero_and_is_named_once`
2. `unit/tests.rs` `wrath_rest_state_reads_the_server_byte_and_fails_to_nils`
3. `keybind/clicks.rs` `chords_parse_and_render_in_the_reference_spelling`
4. `keybind.rs` `modified_clicks_follow_their_rows_and_the_binding_sets`
5. `wrath_natives.rs` `is_modified_click_reads_the_held_modifier` (rewritten: rows registered,
   STICKYCAMERA is CTRL)
6. `benilla-app` `bindings/store.rs` `modified_click_lines_round_trip_beside_the_keys`

Run with `cargo test -p benilla-ui <name>` / `cargo test -p benilla-app <name>`; none has been
compiled.

## Not done

- F15: `BNConnected`/`BNGetMatureLanguageFilter` (`InterfaceOptionsPanels.lua:857/869`),
  `Sound_GameSystem_RestartSoundSystem`, `Sound_ChatSystem_Get{Input,Output}DriverNameByIndex`
  (the existing `Sound_ChatSystem_GetNum*Drivers` answer a constant 0),
  `RestoreVideo{Effects,Resolution,Stereo}Defaults`. Voice* stays out per the owner's VoiceChat
  decision; the `ChannelFrame.lua:716` and `VoiceChatSpeakerTemplate` warnings are expected.
- F04 movement: CAN_FLY/FLYING flags, `CMSG_SET_ACTIVE_MOVER` for possession, a walk-off fall
  sending nothing on its first frame, transport time/seat 0, heartbeat in walk mode,
  `Player::ride` on Wrath. Sibling branches `fix/wrath-movement-wire` (`5a647637`) and
  `fix/wrath-auction-classes` (`ca93372a`) are separate and unchanged.
- The `MoneyFrame.lua:274` caller is not identified yet; the diagnostic above should name it on
  the next owner launch.
