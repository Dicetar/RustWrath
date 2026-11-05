# Autoswarm waves — 2026-11-05

## Baseline and branch

- Owner baseline: `origin/main` at `a8c3ff547aeddd984d8886c2e0f01821c5183d1d`.
- Integration branch: `integration/autoswarm-waves-2026-11-05`.
- Pre-existing local work was preserved on `wip/preserve-2026-11-05-pre-autoswarm` before the
  fast-forward; it was not discarded or stashed.
- Worker output was reviewed before wiring; workers did not run Cargo, git, or a client. The
  Overseer owned shared-file edits and the Cargo slot.
- Wave 1 commits: `2bbf4641` (loose-fixture compile repair) and `e308ad6a` (12340 reply
  decoders and app seam), pushed to `origin/main` as `e308ad6a`.
- `cargo check --workspace --all-targets` was run on the joined tree. Log:
  `.scratch/project-b/check-wave1-2026-11-05.log` (the checked targets finished; pre-existing
  warning volume is unchanged).

## Wave 1 — inbound pet/talent/stable and economy decoders

### Protocol and app wiring

- Added `crates/benilla-protocol-wrath/src/gameplay/pet_talent.rs`: 12340 readers for
  `SMSG_PET_SPELLS` (creature-family word and 32-bit cooldown spell ids), `SMSG_PET_MODE`,
  pet action feedback/sounds, pet learned/removed ids, `SMSG_PET_GUIDS`,
  `SMSG_TALENTS_INFO` (player dual-spec and pet forms), `MSG_LIST_STABLED_PETS`,
  `SMSG_STABLE_RESULT`, and the 12340 six-byte `SMSG_LEARNED_SPELL` plus
  `SMSG_SUPERCEDED_SPELL`/`SMSG_REMOVED_SPELL`.
- Added `crates/benilla-protocol-wrath/src/gameplay/economy.rs`: bank, trade status/tail,
  7-slot 3.3.5 trade-extended with socket-enchant words, mail list (real/shown counts,
  up to 12 attachments, 7 enchant triplets), mail results/query replies, auction hello,
  command result and the three auction list pages.
- Wired both modules into `gameplay.rs` and the 12340 opcode table in `opcode.rs`.
- Added app translation in `crates/benilla-app/src/wrath.rs` and the shared event seam for
  pet spell/learned/removed, talent info/reset and the existing stable/trade/mail/auction
  consumers. `SMSG_LEARNED_SPELL` no longer falls through the old four-byte reader.
- `SessionEvent::AuctionHello` now carries 3.3.3's `enabled` byte; the auction UI refuses a
  disabled house instead of opening it.

### Focused evidence

- `cargo test -p benilla-protocol-wrath gameplay::pet_talent::tests` — 11 passed.
- `cargo test -p benilla-protocol-wrath gameplay::economy::tests` — 7 passed.
- `cargo test -p benilla-protocol-wrath gameplay::pet_talent::tests::talent_and_stable_bounds_are_rejected`
  — 1 passed after tightening the stable cap to the client's five writable rows.
- `cargo test -p benilla-protocol --test auction auction_hello_reply_wire` — 1 passed.
- `cargo check -p benilla-protocol`, `cargo check -p benilla-protocol-wrath` and the workspace
  all-targets check completed. The full app test binary build exceeded the 10-minute command
  timeout before running its single selected case; that case is **not** claimed as passed.

### Audit

- No remaining production `ObjectFields` bank-slot 25–28 or bank-bag-7 lookups were found in
  `ui_bank`, `ui_trade`, `ui_mail` or `ui_auction`; the typed `WrathInventory` path is used.
- Remaining bank purchased-count/money reads are the explicitly projected donor fields in
  `wrath.rs`, not the truncated bag slots.

### Known limits / next consumers

- The donor mail UI still displays the first attachment only and does not render the inline
  3.3.5 body; the decoder retains all attachments and body.
- The donor trade UI has no socket-enchant field, 3.3.5 `trade_id`, or target-error flag; the
  Wrath decoder retains them, the app translation preserves the fields the donor state owns.
- `SessionEvent::TalentInfo` crosses the seam, but the talent window still builds player ranks
  from known spells and the pet talent tab is not yet exposed by `wrath_natives.rs`.
- No live client, login, server/DB or combat session was started.

## Wave 2a — social, party, raid, channel and guild inbound

- Added `crates/benilla-protocol-wrath/src/gameplay/social.rs`: 12340 group invite/notices,
  full `SMSG_GROUP_LIST` (roles, group guid, counter, LFG tail, loot and all three difficulty
  bytes), masked party-member stats (plain and `_FULL`), who/whois, unified contact list,
  friend status, channel notify/list, guild info/roster/event/command, raid target update and
  ready-check request/answer.
- Added the corresponding opcodes and `ServerEvent`/app translation. The existing party,
  social, channel and guild owners receive `SessionEvent::GroupList`, `PartyMemberStats`,
  `WhoResults`, `FriendList`/`IgnoreList`, `FriendStatus`, `ChannelNotify`/`ChannelList`,
  `GuildInfo`/`GuildRoster`/`GuildEvent`/`GuildCommandResult`, `RaidTargetSet`/`List` and
  ready-check events.
- Fixed the generated social test fixture's status/HP mask (`0x11` advertised power while the
  body carried HP); the decoder itself was not loosened.

Evidence:
- `cargo test -p benilla-protocol-wrath gameplay::social::tests` — 14 passed.
- `cargo check -p benilla-app --lib` — completed after the app translation.
- Source-only caveats: 12340 `SMSG_WHOIS` is free-form text and crosses as
  `SessionEvent::WhoisText`; the donor social UI has no console text consumer yet. Guild bank
  tab rights, contact notes, party LFG roles/difficulties and 64-slot aura tails are retained
  by the decoders but not all are represented in the donor UI state.
- Wave 2a commit: `8e744419`, pushed to `origin/main`.

## Wave 2b — Wrath-only LFG, battleground, calendar, rune and vehicle wire

- Added `crates/benilla-protocol-wrath/src/gameplay/wrath_systems.rs` (LFG join/leave/
  proposal/comment/roles encoders and result/queue/update/proposal decoders; battlemaster and
  battlefield-port encoders; battlefield-status and PvP-log decoders; calendar get/signup/rsvp
  encoders plus full calendar/command decoders; rune convert/resync/add-power decoders;
  vehicle dismiss/exit/seat encoders).
- Added the 12340 opcodes, `DecoderFamily::WrathSystems`, `ServerEvent` variants and parse arms.
- The app currently returns no `SessionEvent` for these decoded records and logs that the LFG,
  battlefield, calendar and rune **views are not yet wired**. This is an explicit remaining
  consumer gap, not a claim that the Lua natives or state stores are complete: `OpenCalendar`,
  `GetRuneCooldown`/`GetRuneType`, `CanExitVehicle`/vehicle seat natives and the calendar
  day/invite stores still need their host state owner and feed. The wire adapters are real and
  tested; the UI feature is not claimed.

Evidence:
- `cargo check -p benilla-protocol-wrath` and `cargo check -p benilla-app --lib` passed.
- Focused `wrath_systems` run exercised 39 of the module's 40 tests; 4 vehicle-fixture
  expectations were wrong about packed-guid framing, were corrected, and the 4 corrected tests
  then passed (the other 35 had already passed). The intended 20-case ceiling was exceeded by a
  test-runner argument mistake; this is recorded rather than hidden.
- The remaining unrun systems test is not claimed.
- No client, login, server/DB or live session was started.
- Wave 2b commit: `d1d23f93`, pushed to `origin/main`.

## Wave 4 (early) — Windows release sets

- `tools/release-set.ps1` now builds deterministic SHA-256 manifests, stages immutable version
  directories outside user config, validates paths/reparse points/disk space/hashes/markers,
  atomically switches `active-release.json`, and recovers the previous known-good set after a
  failed activation or startup health check. SHA-256 is explicitly local integrity, not
  authentication.
- `tools/launch-client.ps1` keeps the developer `target\debug\benilla.exe` path as default and
  adds opt-in `-ReleaseRoot`/`-UseReleaseSet`. Release mode validates before launch, keeps data,
  config and addons in user-owned roots, logs the version/manifest/previous identity, and on a
  failed startup health check stops the process it started, rolls back once and does not start a
  second client.
- `tools/release-set.tests.ps1` is self-contained under `F:\AI\tmp\opencode`.

Evidence:
- `powershell.exe -NoProfile -ExecutionPolicy Bypass -File .\tools\release-set.tests.ps1` —
  59 passed, 0 failed. No game data, credentials or live client was touched; no launcher run.
- Owner approval still needed for the default 15-second health window, automatic stop of the
  just-started process, and `-ConfigRoot` semantics. These are the current published defaults,
  not owner-approved release policy.
- Wave 4 W commit: `c9151421`, pushed to `origin/main`.

## Wave 3 — scoped delivery, recorder export guard and coherent reload

- `crates/benilla-app/src/wrath.rs`: one `retire_scoped_delivery` barrier now covers world
  transfer, logout/session end, start/stop and shutdown; outstanding receipts, WM proposals,
  parked links and publisher handles are answered once at the boundary, and old-scope replies
  cannot mutate the adopted scope. `WrathWmActions::retire_all/retire_scope/refuse_unadmitted`
  and `SessionReplies::adopt_scope/retire_scope/superseded` carry the terminal state.
- `crates/benilla-app/src/quest_trace.rs`: manual, background and `AppExit` exports share one
  condvar writer gate; failed spawn releases it; the final snapshot cannot be dropped or
  interleaved.
- `crates/benilla-assets/src/profile_source.rs`: staged content revisions validate DBC/model/
  texture/UI inputs off the active revision and commit whole; failed candidates leave the old
  chain active.
- `crates/benilla-app/src/ui_script/lifecycle.rs`: `ReloadUI` stages saved state and a candidate
  VM before the single teardown boundary; refusal keeps the live VM and records the attempt.

Evidence:
- `cargo test -p benilla-app scoped_delivery_tests` — 4 passed.
- `cargo test -p benilla-app export_gate_tests` — 4 passed.
- `cargo test -p benilla-app a_refused_reload_keeps_the_live_vm_and_its_saved_state` — 1 passed.
- `cargo test -p benilla-app a_ready_candidate_reload_crosses_one_boundary_and_clears_the_refusal_record` — 1 passed.
- `cargo test -p benilla-assets profile_source::tests` — 9 passed.
- Remaining limits: profile and `WorldAssets.chain` are swapped adjacently under two locks, so
  one instant of mixed visibility remains possible across threads; the successful commit was
  not exercised against a real MPQ archive; a post-boundary FrameXML failure is not rollback-able;
  saved-state staging only runs with a registered `mpq://` profile reader. No live reload was run.
- Wave 3 ledger commit: `a83a4d04`, pushed to `origin/main`.
- Wave 3 T2/H2 commit: `1b44a70c`, pushed to `origin/main`.

## Wave 4 — movement and Souls-combat seams

- `crates/benilla-hotpatch/src/lib.rs`: added `MovementModuleV1` staging/validation/commit/
  unload with typed refusals, finite/step/speed/facing/flag checks and no pose-history impact.
- `crates/benilla-app/src/player/controller.rs`: `evaluate_movement_intent`, bounded
  `apply_movement_correction` and flag merging; `wrath_stream_movement` remains the sole writer
  and an absent processor preserves baseline behavior.
- `crates/benilla-app/src/combat_mode.rs`: typed combat proposal/settlement state machine and
  `evaluate_combat_intent`/`apply_authoritative_combat_result`; server results alone settle a
  window, with duplicate/late/death/disconnect handling and no local stamina/damage debit.

Evidence:
- `cargo test -p benilla-hotpatch movement_` — 6 passed.
- `cargo test -p benilla-app movement_module_tests` — 6 passed.
- `cargo test -p benilla-app souls_seam_tests` — 7 passed.
- Remaining limits: no production movement host/controller adapter yet (the `player::controller`
  module is private and the Bevy hotpatch bridge is absent), so the resource is normally absent
  and the controller runs `Disabled`; no movement DLL was built or loaded and no packet was
  observed. Combat has no module runner or server mode/ack receiver; the mode remains refused by
  default and no action is sent. No client or live session was run.
- Wave 4 M/E commit: `8260cf79`, pushed to `origin/main`.
- Final joined-tree evidence: `cargo check --workspace --all-targets` passed; log
  `.scratch/project-b/check-final-2026-11-05.log`.

### Mechanical compile repair

`crates/benilla-assets` test fixtures were missing the newly required `LooseFiles::extra_addons`
field on `main`; all 12 initializers now supply `Vec::new()`. This is a source-compile repair,
not profile/reload behavior.
