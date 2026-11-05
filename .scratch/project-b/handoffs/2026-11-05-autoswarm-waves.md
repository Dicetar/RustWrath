# Autoswarm waves — 2026-11-05

## Baseline and branch

- Owner baseline: `origin/main` at `a8c3ff547aeddd984d8886c2e0f01821c5183d1d`.
- Integration branch: `integration/autoswarm-waves-2026-11-05`.
- Pre-existing local work was preserved on `wip/preserve-2026-11-05-pre-autoswarm` before the
  fast-forward; it was not discarded or stashed.
- Worker output was reviewed before wiring; workers did not run Cargo, git, or a client. The
  Overseer owned shared-file edits and the Cargo slot.
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

### Mechanical compile repair

`crates/benilla-assets` test fixtures were missing the newly required `LooseFiles::extra_addons`
field on `main`; all 12 initializers now supply `Vec::new()`. This is a source-compile repair,
not profile/reload behavior.
