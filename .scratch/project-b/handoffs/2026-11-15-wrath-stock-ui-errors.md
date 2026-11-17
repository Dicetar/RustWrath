# Wrath stock interface, controls and movement gaps (matrix F04, F14, F15)

Status: ready to merge
Owner: Claude Code (Opus 5.5)
Branch: fix/wrath-stock-ui-errors
Issue: none
Next: source review and the owner's batch build; F04 movement gaps continue on `fix/wrath-movement-gaps`

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
- Options panel Defaults buttons and the sound restart (F15):
  - `RestoreVideoResolutionDefaults`/`RestoreVideoEffectsDefaults`/`RestoreVideoStereoDefaults`
    (12340 `0x00510DC0`/`0x00510DD0`/`0x004DD420`, all `ConsoleDeviceDefaults(page)`
    `0x0076A130`) restore the page's rows (`cvars.rs` `VIDEO_*_PAGE`, `restore_video_page`):
    `fixedFunction` 0, the `SetGxCVars` rows plus `Client.cpp SetDefaults` (`0x00402530`:
    desktopGamma 0, gamma 1.0, windowResizeLock) and a `RestartGx` ask on page 0; the
    `CWorldParam::SetDefaults` (`0x0078E1A0`) rows, groundEffectDist 70.0, and
    textureFilteringMode on page 1; the stereo rows on page 2. Named deviation: the registered
    default stands in for `ConsoleDetectSetDefaults` hardware detection, and a row this client
    never registered is skipped.
  - `Sound_GameSystem_RestartSoundSystem` (`0x00985D30`, `AudioOptionsFrame.lua:29`) counts an
    ask; `sound::drain_restart_sound` turns it into a close and immediate reopen of the output
    stream on the current default device (`OutputBackend::request_restart`). Named deviation: the
    renderer and playing sounds survive, and `SoundBufferSize`, `Sound_NumChannels` and
    `Sound_OutputQuality` are not re-read.
  - Not needed: `BNConnected`/`BNGetMatureLanguageFilter` are only reached behind
    `BNFeaturesEnabled()`, which answers false; `Sound_ChatSystem_Get*DriverNameByIndex` are only
    reached for a count above 0, and the chat-system counts answer 0 under the owner's VoiceChat
    decision.

## Focused cases written (not run)

1. `tests/anchors.rs` `an_unresolved_wrath_frame_answers_zero_and_is_named_once`
2. `unit/tests.rs` `wrath_rest_state_reads_the_server_byte_and_fails_to_nils`
3. `keybind/clicks.rs` `chords_parse_and_render_in_the_reference_spelling`
4. `keybind.rs` `modified_clicks_follow_their_rows_and_the_binding_sets`
5. `wrath_natives.rs` `is_modified_click_reads_the_held_modifier` (rewritten: rows registered,
   STICKYCAMERA is CTRL)
6. `benilla-app` `bindings/store.rs` `modified_click_lines_round_trip_beside_the_keys`
7. `wrath_natives.rs` `options_panel_defaults_and_sound_restart_follow_the_reference`

Run with `cargo test -p benilla-ui <name>` / `cargo test -p benilla-app <name>`; none has been
compiled.

## Not done

- F15: Voice* stays out per the owner's VoiceChat decision; the `ChannelFrame.lua:716` and
  `VoiceChatSpeakerTemplate` warnings are expected.
- F04 movement: moved to `fix/wrath-movement-gaps` (cut from `fix/wrath-movement-wire`
  `5a647637`). Its note covers the mover orders, possession, the transport seat and the
  remaining named items (flight, transport time, `Player::ride`). `fix/wrath-auction-classes`
  (`ca93372a`) is separate and unchanged.
- The `MoneyFrame.lua:274` caller is not identified yet; the diagnostic above should name it on
  the next owner launch.
