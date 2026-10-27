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
