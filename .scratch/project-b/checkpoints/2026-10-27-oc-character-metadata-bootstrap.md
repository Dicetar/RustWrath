# Wrath character metadata and early UI bootstrap — claim and working record

Date: 2026-10-27. Owner: OpenCode (formats/catalog/bootstrap lane). Checkout
`D:/Projects/project-b-client`, branch `wotlk`. Reviewed start `a1c73297`; claim-time HEAD
`17af60cb` (Astra's real-data class-name binding/projection and handoff, both preserved).

Evidence: Astra's admitted Wrath smoke at `404cea29` failed at `Constants.lua:87` because
`FillLocalizedClassList` was nil. Astra's `17af60cb` installs the binding and an immutable
`LocalizedClassName` projection; OC owns the actual catalog rows and bootstrap.

## File claim (exact)

Editing:
- `crates/benilla-formats/src/chr_classes.rs` — 3.3.5 layout, all locale/gender name blocks,
  filename, power/spell-family/flags metadata; localized class-label projection.
- `crates/benilla-formats/src/chr_races.rs` — 3.3.5 layout, locale/gender name blocks.
- `crates/benilla-formats/src/locale.rs` — Wrath 9-locale index from the configured tag.
- `crates/benilla-formats/src/characters/customization.rs` — layout-aware ChrRaces and
  CharStartOutfit reads for the create catalog; keep the 1.12 path and guards.
- `crates/benilla-app/src/chr_classes.rs` — app projection helper (checked u8 ids) and the
  bootstrap seating entry point.
- `crates/benilla-app/src/ui_script/lifecycle.rs` — seat the projection after VM construction
  and before GlueXML/FrameXML load, in both glue and in-game startup. Astra's claim covers only
  `benilla-ui/**` and `ui_script/wrath_ui_slice_tests.rs`; her handoff says OC owns bootstrap.
- This checkpoint and a consumer handoff.

Not editing: `crates/benilla-lua/**`, `crates/benilla-mlua/**`, `crates/benilla-ui/**`,
`crates/benilla-app/src/ui_script/wrath_ui_slice_tests.rs`, WM, procgen, owner verification logs.

## Pinned target layouts (source evidence)

AzerothCore `5e741c37` `chrclasses_dbc.sql` / `chrraces_dbc.sql` are generated from the 3.3.5
DBCs and give the physical column order; `DBCfmt.h`/`DBCStructure.h` corroborate the server-read
columns:
- `ChrClasses` (60 fields): 0 ID, 2 DisplayPower, 3 PetNameToken, 4..19 Name, 20 mask,
  21..36 Name_Female, 37 mask, 38..53 Name_Male, 54 mask, 55 Filename, 56 SpellClassSet,
  57 Flags, 58 CinematicSequenceID, 59 Required_Expansion.
- `ChrRaces` (69 fields): 0 ID, 1 Flags, 2 FactionID, 4/5 male/female display, 6 ClientPrefix,
  11 ClientFilestring, 12 CinematicSequenceID, 14..29 Name, 31..46 Name_Female, 48..63 Name_Male,
  65/66 FacialHairCustomization, 67 HairCustomization, 68 Required_Expansion.
- Locale columns: the client's nine-locale order (enUS, koKR, frFR, deDE, zhCN, zhTW, esES,
  esMX, ruRU), matching AzerothCore `Common.cpp` and Astra's Wrath table; the SQL dump's later
  column labels (enGB/enCN/...) are a generator list and are not trusted beyond column zero.
- `PetNameToken` is an integer column in the 3.3.5 dump; the client token ("PET"/"DEMON") is
  recovered only when the value is a valid string-table offset yielding exactly those keys.
  Otherwise the existing `"PET"` fallback stays (no guessed enum mapping).
- Gender labels: gendered column when the selected locale has one; otherwise the base `Name`
  column in the same locale (the DBC's own default). Never another language or the other gender.

## Bootstrap rules

- Seat the projection after the target VM exists (locale set) and before GlueXML/FrameXML loads.
- Missing table, 1.12-only catalog, unsupported locale, class id > 255, or a row without a
  Filename is an explicit refusal/diagnostic; never a fabricated complete table.
- The fixture and production use the same `benilla_formats` catalog + app projection helper; no
  test-only stand-ins.

## Implemented (source, no execution)

- `ChrClasses` selects the 60-field 12340 layout from the header, reads DisplayPower (2),
  PetNameToken (3, accepted only when it resolves to the literal PET/DEMON), the three 16-slot
  name blocks (4/21/38, mapped from the nine UI locales), Filename 55, SpellClassSet 56, Flags
  57, CinematicSequenceID 58 and Required_Expansion 59; the 17-field 1.12 path is unchanged.
- `ChrRaces` selects the 69-field 12340 layout (Flags 1, FactionID 2, displays 4/5, file 11,
  intro 12, the 16-slot names at 14/31/48, tokens 65/66/67, expansion 68); the 29-field 1.12
  path is unchanged.
- `Locale::wrath_index` (UI index) and `Locale::wrath_dbc_column` (physical DBC slot) are
  separate; all 12340 loaders map UI index -> DBC slot.
- Class labels resolve the gendered column's non-empty string, else the same-locale base; the
  app projection refuses an empty catalog, non-12340 tables, unsupported locales, ids past
  `u8`, duplicate ids/Filenames, empty/NUL Filenames and missing labels.
- `ui_script::lifecycle` seats the projection after `set_locale`/VM construction and before
  `load_glue` and before the FrameXML load.
- Creation catalog: Wrath playable races derive from `ChrRaces` Flags bit 0x1; 12340
  `CharStartOutfit` uses the 77-field/296-byte `dbbbX` + 24 items + 24 displays + 24 inventory
  types layout; `CharSections` uses 0x1 = PLAYER on 12340 versus the 1.12 0x1 = unavailable
  filter. Wrath appearance guards are structural (no 12340 rows are vendored).
- Explicit gaps: 12340 relic slot is unavailable (false, documented); Death-Knight-only
  CharSections children are not folded into the regular dials; live smoke still required.
