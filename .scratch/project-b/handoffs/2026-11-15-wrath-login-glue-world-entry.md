# Wrath Login, Realm List, Character Select, and World Entry — ZAEBAL Audit & Repair

- Branch: `fix/wrath-login-glue-world-entry`
- Base: `origin/main` (`9dc36e48`)
- Status: ready to merge
- Scope:
  - `crates/benilla-protocol-wrath/src/opcode.rs`
  - `crates/benilla-protocol-wrath/src/auth.rs`
  - `crates/benilla-protocol-wrath/src/logon.rs`
  - `crates/benilla-app/src/wrath.rs`

## Findings & Root Causes (ZAEBAL)

1. **Auth & World Refusal Codes Stripped Before Reaching `GlueMgr` (`auth.rs`, `logon.rs`, `wrath.rs`)**:
   - `read_challenge_reply` and `read_proof_reply` in `crates/benilla-protocol-wrath/src/auth.rs`, `verify_server_proof` in `crates/benilla-protocol-wrath/src/logon.rs`, and `SMSG_AUTH_RESPONSE` failure handling in `crates/benilla-app/src/wrath.rs` formatted non-zero server reject bytes into plain strings via `bail!`, and `WrathControl::start`'s worker wrapper emitted `SessionEvent::LoginFailed { refusal: None, .. }`.
   - Consumer impact: `GlueMgr::login_refused_status` (`crates/benilla-app/src/ui_script/glue_mgr.rs:1464-1521`) requires `Some(LoginRefusal::Logon(code) | LoginRefusal::LogonProof(code) | LoginRefusal::World(code))` to show `LOGIN_BAD_PASS`, `LOGIN_UNKNOWN_ACCOUNT`, `LOGIN_BANNED`, `LOGIN_LOCKED`, etc.; `None` collapsed every refusal to generic `"DISCONNECTED"`.
   - **Fix**: Added `LogonAuthError::{Challenge(u8), Proof(u8), World(u8)}` in `benilla-protocol-wrath::auth`, returned it from `read_challenge_reply`, `read_proof_reply`, `logon`'s `verify_server_proof` mismatch (`Proof(0x04)`), and `SMSG_AUTH_RESPONSE` refusal (`World(code)`), and extracted `LoginRefusal` via `extract_login_refusal` when emitting `SessionEvent::LoginFailed`.

2. **`CancelWorld` vs `DisconnectFromServer`, Realm Switching, and Realm Refresh Lifecycle (`wrath.rs`)**:
   - `GlueLogin::cancel_world` (`glue_session/login.rs:348`) calls `abandon.advance()` while keeping the logon session open (`Step::RaiseRealmList`), expecting `RequestRealmList` and subsequent `ChangeRealm` calls to operate on the existing authenticated logon session.
   - However, `wrath_login_admission` reacted to any `LoginAbandon` generation change by wiping `config.account` / `config.password` and setting `control.cancel = true`, killing the worker and breaking realm re-selection after canceling a world handshake or realm queue.
   - Conversely, `close_logon` (`DisconnectFromServer`) emitted `RealmRequest::Close` / `CharRequest::Abandon`, which set `control.cancel = true` without clearing `config.account` / `config.password` / `config.preferred_realm` or `control.restart_allowed`, allowing `wrath_session_ownership`'s `roster_restart` to immediately respawn a parked session after the user disconnected from `CharSelect`.
   - Furthermore, `wrath_session_ownership` ran before `wrath_login_admission` in `PreUpdate` without checking `login.pending.is_none()`, so when switching realms from `CharSelect` (`RealmRequest::Enter`), `roster_restart` could claim the worker slot with `roster_asked: true` before `wrath_login_admission` consumed `login.pending`.
   - **Fix**:
     - Added `WrathParkedCommand::{CancelWorld, LeaveRealm, Close}` and `WrathFeed::login_generation` (`Arc<AtomicU64>`).
     - Wrapped the realm-selection -> world-connect -> world-auth -> `CMSG_CHAR_ENUM` -> parked `CharSelect` flow in `run_session_with_cancel` inside a `'realm: loop` with a `refresh_or_relogon` fallback on `RefreshRealms`.
     - On `LoginAbandon` advance (`CancelWorld`), `wrath_login_admission` updates `config.login_generation` and `feed.login_generation`, clears `preferred_realm`, and sends `WrathParkedCommand::CancelWorld` to the parked worker so it returns to the logon realm loop in-place.
     - On `RealmRequest::Close` / `CharRequest::Abandon` (`DisconnectFromServer`), `wrath_pick_admission` clears credentials, `preferred_realm`, `login.pending`, and `restart_allowed`, sends `WrathParkedCommand::Close`, and cancels the worker.
     - On `RealmRequest::Leave`, `wrath_pick_admission` clears `preferred_realm` and `restart_allowed` and sends `WrathParkedCommand::LeaveRealm`.
     - Gated `roster_restart` in `wrath_session_ownership` on `!has_pending_login && !c.account.is_empty()`.

3. **`SMSG_CHARACTER_LOGIN_FAILED` (`0x041`) Unhandled in Wrath World Loop (`opcode.rs`, `wrath.rs`)**:
   - `SMSG_CHARACTER_LOGIN_FAILED` (`0x041` in build 12340) was absent from `benilla-protocol-wrath/src/opcode.rs` and ignored in `run_session_with_cancel`, leaving the client stuck on the loading screen if the server refused `CMSG_PLAYER_LOGIN`.
   - Consumer contract: `GlueMgr::char_login_status` (`glue_mgr.rs:1554`) indexes a 6-entry table `CHAR_LOGIN_STATUS[1..=6]` (`IN_PROGRESS`, `DISABLED`, `NO_WORLD`, `DUPLICATE_CHARACTER`, `NO_INSTANCES`, `LOCKED_FOR_TRANSFER`, else `CHAR_LOGIN_FAILED`), whereas 12340 sends `ResponseCodes` `0x3B..=0x44`.
   - **Fix**: Added `SMSG_CHARACTER_LOGIN_FAILED = 0x041`, added `char_login_failed_result(raw)` mapping `0x3B..=0x44` to `1..=6` (and `0x40` to `0`), emitted `SessionEvent::CharacterLoginFailed { result }` + trace milestone, invalidated scope, retired the publisher, and returned `Ok(())` so the session ends as `SessionEnd::LoggedOut` and `GlueMgr` transitions back to `CharSelect` (`ScreenState::LoginRefused`).

4. **Character Rename (`CMSG_CHAR_RENAME` `0x2C7` / `SMSG_CHAR_RENAME` `0x2C8`) Unhandled in Wrath (`opcode.rs`, `wrath.rs`)**:
   - `CharRequest::Rename { guid, name }` was dropped by `wrath_pick_admission`, and `CMSG_CHAR_RENAME` / `SMSG_CHAR_RENAME` were missing from `opcode.rs` and the parked character-select loop.
   - **Fix**: Added `CMSG_CHAR_RENAME = 0x2C7` and `SMSG_CHAR_RENAME = 0x2C8` to `opcode.rs`, routed `CharRequest::Rename` to `WrathParkedCommand::Rename` in `wrath_pick_admission`, sent `encode_char_rename(guid, &name)` in the parked loop, decoded `SMSG_CHAR_RENAME` via `parse_char_rename` (mapping 12340 `0x32` `CHAR_CREATE_NAME_IN_USE` to `0x31` for `GlueMgr::char_rename_status`), updated the in-memory `records` entry and cleared `CHARACTER_FLAG_RENAME`, and emitted `SessionEvent::CharRenamed { result, renamed }`.

5. **World Entry State Parity (`NetStatus.connected` and Self Player `NameCache` Seeding in `wrath_apply_world`)**:
   - Unlike the stock `net/session.rs::connected` handler, `wrath_apply_world` never set `NetStatus.connected = true` on world entry (nor cleared it on session teardown), causing systems gated on `NetStatus.connected` (such as `send_query_time` in `net.rs:808`) to remain unseated.
   - Furthermore, `wrath_apply_world` ran `unit_name_lookups` on initial self player creation before seeding the local player's name into `NameCache`, leaving `UnitName("player")` unresolved until a network `CMSG_NAME_QUERY` round-trip completed.
   - **Fix**: Recorded the selected character's `(guid, name)` on `WrathFeed::player_name`, seeded `NameCache::insert_player(player_guid, name, None)` (from `Roster`, `WrathFeed::player_name`, or `WrathControl.config.character_name`) before `unit_name_lookups` on world entry, set `NetStatus.connected = true` (`last_reason = None`) on world entry, and cleared `NetStatus.connected = false` when `changes.world_transferred && player_guid == 0`.

## Verification

- Static contract and caller/consumer audit completed across `benilla-protocol-wrath`, `wrath.rs`, `glue_mgr.rs`, `glue_session/{login,roster}.rs`, and `net/{mod,session}.rs`.
- Added unit tests in `crates/benilla-protocol-wrath/src/auth.rs` (`challenge_reply_preserves_typed_refusal_code`, `proof_reply_preserves_typed_refusal_code`) and `crates/benilla-app/src/wrath.rs` (`extract_login_refusal_maps_challenge_proof_and_world_codes`, `char_login_failed_result_maps_12340_response_codes_to_glue_indices`, `char_rename_codec_round_trips_and_maps_status_codes`).
- Cargo build/check/test not run in this cloud sandbox (no `cargo`/`rustc`/`sccache` toolchain installed in container).
