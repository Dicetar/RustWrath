# Wrath chat language and tag fix

Status: landed
Owner: OpenCode / DeepSeek V4.1 Flash (took over the argon branch)
Branch: argon/say-chat-log
Issue: none
Next: owner batch build and the three checks below; source review is complete and no Cargo, test or client run was made.

Merged `origin/main` `7114694f` (2026-11-17) into the branch without conflicts. Worktree:
`D:\Projects\wow\worktrees\argon-say-chat-log`. The branch had no task note; this one is the
claim and the review surface.

## What was wrong

- `crates/benilla-formats/src/languages.rs` read `ChrRaces.dbc` `BaseLanguage` at field 8 for
  every target. In 3.3.5 the 69-field row puts `BaseLanguage` at 7 and `CreatureType` (7 =
  Humanoid) at 8, so every race resolved Common.
- The 3.3.5 `Languages.dbc` has 18 fields (a 16-locale name block), but the reader used the
  1.12 eight-locale column index, so non-enUS locales read the wrong column or empty.
- 12340 `SMSG_MESSAGECHAT` sends `chat_tag` as a bitmask (AC `Player.h:838-843`:
  `0x01` AFK, `0x02` DND, `0x04` GM, `0x08` COM, `0x10` DEV). The donor seam carries an
  enum, so GM and combined tags were wrong.
- Draenei (race 35, language 35) had no name, and the default language seed depended on the
  roster pick being present, which a world-entry VM may not have.

## What changed

- `languages.rs`: the 69-field `ChrRaces.dbc` layout reads `BaseLanguage` at 7; the 18-field
  `Languages.dbc` resolves the locale through `Locale::wrath_dbc_column`, falling back to the
  English column. The loader is split into parseable functions so the synthetic DBC test can
  exercise both layouts.
- `wrath.rs`: `chat_tag_112` maps the 12340 bitmask to the donor enum with GM > DND > AFK;
  COM/DEV have no donor tag and read NONE.
- `event.rs`: Draenei language 35.
- `ui_unit.rs`: `seed_default_language` falls back to the self player's projected
  `UNIT_FIELD_BYTES_0` race when the roster pick is absent.
- This review adds `wrath_chat_tag_bitmask_becomes_the_vanilla_enum`; the branch already had
  `wrath_chr_races_reads_base_language_at_field_7_and_16_locale_languages`.

## What was checked

- 3.3.5 `ChrRaces.dbc` layout: 69 fields, BaseLanguage 7, CreatureType 8 (the DBC/wowdev
  layout; AC's compact `ChrRacesEntry` in this fork does not name BaseLanguage, so it is not a
  counter-reference). The synthetic row in the branch's test sets field 7 to Orcish and field 8
  to 7/Humanoid and asserts Orcish, which fails on the old field-8 read.
- `Languages.dbc` 18-field row: the branch's test builds a ruRU name at the `wrath_dbc_column`
  position and reads it back; the 1.12 path keeps `LOCALES_112` and `loc_str`.
- AC `Player.h:838-843` for the tag bits; the donor `chat_tag` enum is the destination.
- `git diff --check` passes on the merged tree. No Cargo, test execution, client launch or CI
  run was made; compile and tests are **not run**.

## Named limits

- The branch's language test uses synthetic DBC bytes; the installed 3.3.5 `ChrRaces.dbc` /
  `Languages.dbc` rows are the owner's data check.
- No new locale table is invented: the 16-language order comes from the existing
  `LOCALES_335` / `Locale::wrath_dbc_column` owner.
- Chat tags COM/DEV are dropped by design; the donor enum has no spelling for them.

## Owner check list

1. On a non-human race, `/say` and watch the language header: it is the race's language, not
   Common (e.g. Orcish, Draenei).
2. Set AFK, DND and GM and send a chat line: the prefix matches the highest set tag (GM over DND
   over AFK).
3. Enter the world without a pending roster row (a reconnect/world-entry path) and check the
   default chat language still seeds from the character's race.
