# Task: Login, Realm List, Character Select, World Entry (ZAEBAL Audit & Repair)

- Branch: `fix/wrath-login-glue-world-entry`
- Agent: cloud
- Status: in progress
- Scope:
  - `crates/benilla-protocol-wrath/src/opcode.rs`
  - `crates/benilla-protocol-wrath/src/auth.rs`
  - `crates/benilla-protocol-wrath/src/logon.rs`
  - `crates/benilla-app/src/wrath.rs`

## Goal

Audit the end-to-end Wrath 3.3.5a (build 12340) Login -> Realm List -> Character Select -> World Entry -> Logout pipeline under ZAEBAL claim-vs-evidence discipline and close all remaining seam and state-machine bugs:

1. **Typed Auth & World Refusal Codes on `SessionEvent::LoginFailed`**:
   - Propagate real `LoginRefusal::Logon(u8)`, `LoginRefusal::LogonProof(u8)`, and `LoginRefusal::World(u8)` codes instead of `refusal: None` so `GlueMgr::on_failed` displays the real 12340 account/auth status dialog instead of always falling back to `"DISCONNECTED"` / `STATUS_FAILED_TO_CONNECT`.
2. **`CancelWorld` / `DisconnectFromServer` / `ChangeRealm` Lifecycle in `wrath_login_admission`, `wrath_pick_admission`, and `wrath_session_ownership`**:
   - Preserve credentials and re-park at the realm list when `GlueLogin::CancelWorld` advances `LoginAbandon` without `RealmRequest::Close` / `CharRequest::Abandon`, so selecting another realm in the raised `RealmList` dialog succeeds.
   - Clear `preferred_realm`, `restart_allowed`, and credentials on explicit `RealmRequest::Close` / `CharRequest::Abandon`.
   - Prevent `wrath_session_ownership`'s `roster_restart` from pre-empting a queued `login.pending` during realm switching (`ChangeRealm` -> `RealmRequest::Leave` -> `RealmRequest::Refresh` -> `RealmRequest::Enter`), and allow `RealmRequest::Refresh` to reconnect/re-serve the realm list when the worker is cycling or the idle auth socket closed.
3. **`SMSG_CHARACTER_LOGIN_FAILED` (`0x041`) Handling**:
   - Decode `SMSG_CHARACTER_LOGIN_FAILED` (`0x041`) after `CMSG_PLAYER_LOGIN`, map 12340 `ResponseCodes` (`CHAR_LOGIN_*`) to `GlueMgr`'s `char_login_status` index, emit `SessionEvent::CharacterLoginFailed`, and return cleanly to `CharSelect` instead of hanging on the `EnterWorld` loading screen.
4. **Character Rename (`CMSG_CHAR_RENAME` `0x2C7` / `SMSG_CHAR_RENAME` `0x2C8`)**:
   - Wire `CharRequest::Rename { guid, name }` through `WrathParkedCommand::Rename`, send `CMSG_CHAR_RENAME` (`0x2C7`), decode `SMSG_CHAR_RENAME` (`0x2C8`), update the cached roster record (name and clearing `CHARACTER_FLAG_RENAME`), and emit `SessionEvent::CharRenamed`.
5. **World Entry State Parity in `wrath_apply_world`**:
   - Set `NetStatus.connected = true` on world entry (and clear on `SessionEnded` / `WorldTransfer` exit) so downstream systems gated on `NetStatus.connected` (such as `DeathView` and periodic `send_query_time`) operate in Wrath.
   - Seed `NameCache` with the entering player's own name from `Roster` / `WrathControl` before `unit_name_lookups` runs so `UnitName("player")` resolves immediately during `PLAYER_LOGIN` / `PLAYER_ENTERING_WORLD`, and reset session-scoped `GuildState`, `PetitionRecords`, and `QuestGiver` state on world entry.
