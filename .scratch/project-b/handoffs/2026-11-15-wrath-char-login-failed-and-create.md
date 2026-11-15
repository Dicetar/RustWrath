# Handoff: Wrath character-login-failed terminal delivery, character-create class re-homing, and Glue `GetText`

- **Branch:** `fix/wrath-char-login-failed-and-create`
- **Owner:** AI Studio Build agent
- **Status:** ready to merge
- **Next:** Merge into `main` / `integration/wrath-alpha`; owner runs `cargo test` and local launch verification.

## Root causes and fixes

1. **`SessionEvent::CharacterLoginFailed` dropped by `SessionReplies` on `SMSG_CHARACTER_LOGIN_FAILED` (`crates/benilla-app/src/wrath.rs`)**:
   - In `run_session_with_cancel`, receiving `SMSG_CHARACTER_LOGIN_FAILED` pushes `SessionEvent::CharacterLoginFailed { result }` and exits cleanly with `Ok(())`, which immediately causes the worker closure to emit `WrathFeedEvent::SessionEnded` and `SessionEvent::Disconnected { reason: "wrath session ended", end: SessionEnd::LoggedOut }`.
   - Because `terminal_index` only matched `LoggedOut`, `Disconnected`, and `LoginFailed`, `CharacterLoginFailed` was classified as `SessionClass::Lifecycle` in `self.fifo` instead of `SessionClass::Terminal`.
   - Both `replies.retire_scope()` (run by `wrath_net_drain` on `SessionEnded` before `wrath_dispatch_events`) and `replies.take()` (when `self.terminal` is non-empty due to `Disconnected`) clear `self.fifo`, dropping `CharacterLoginFailed` before it could ever reach `glue_mgr::on_char_login`.
   - Fixed by mapping `SessionEvent::LoggedOut | SessionEvent::CharacterLoginFailed { .. } => Some(0)` in `terminal_index`, preserving `[CharacterLoginFailed, Disconnected]` in order across `retire_scope()` and `take()`.
2. **`GlueCreate::reset` and `GlueCreate::select_race` leaving invalid `(race, class)` pairs on Wrath 3.3.5a (`crates/benilla-ui/src/script/glue_create.rs`, `glue_create_tests.rs`)**:
   - On Wrath 3.3.5a, `CharacterCreate.lua` does not call `SetSelectedClass(1)` after `SetSelectedRace(id)` (unlike 1.12), and Blood Elf (`race == 10`) has no Warrior (`class == 1`).
   - Updated `GlueCreate::reset(wrath)` and `GlueCreate::select_race(index, wrath)` so that on `UiTarget::Wrath335`, `class_list` is built for the new race first and `self.class` is re-homed via `self.select_class(0)` whenever the new race's `class_list` does not contain `self.class` (leaving 1.12 donor order unchanged).
3. **`GetText` missing in Wrath Glue VM and 2-argument `GetText(token, gender)` support (`crates/benilla-ui/src/script/client.rs`, `glue.rs`)**:
   - Extracted `client::install_get_text` and installed it in `glue::install` when `model.is_wrath()`, so 3.3.5a GlueXML (`CharacterCreate.lua`, `CharacterSelect.lua`) has `GetText` without altering the 1.12 `UiScript::new_glue()` surface.
   - Extended `GetText` to resolve `_FEMALE` and `_P1` for both 3.3.5a's 2-argument `GetText(token, gender)` (`gender == 2` male, `3` female from `UnitSex`/`GetSelectedSex`) and 3-argument `GetText(token, count, gender)`.

## Verification

- Compilation (`cargo check` / `cargo build`): **not run** (per `AGENTS.md`).
- Unit tests (`cargo test`): **not run** (per `AGENTS.md`; 3 focused unit test additions included in `wrath.rs`, `glue_create_tests.rs`, and `client.rs`).
- Client / server launch: **not run** (per `AGENTS.md`).
