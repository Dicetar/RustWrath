# Wrath class-name consumer handoff

Owner: Astra, UI binding only. OC retains formats/catalog/bootstrap. Source-only; no build,
test, new test case, formatter or runtime run was performed for this change. The previous
focused smoke grant was consumed at 404cea29 and failed before PlayerFrame at Constants.lua:87.

Subsequent verification supersession: the one authorized 2026-10-27 17:04 Moscow pass at
f70be142 compiled benilla-lua/mlua/ui, and completed inherited suites with the same two known
UI timezone failures (3,322 UI passed). This establishes compilation of the consumer seam,
not direct class-list semantics or real catalog bootstrap. See
../verification/2026-10-27-astra-ui3/summary.md. No real-data app smoke was rerun and no grant
remains. OC still owns all integration steps below.

## Delivered seam

`benilla_ui::script::LocalizedClassName { id: u8, file: String, male: String, female: String }`
is an immutable read projection of catalog rows. `UiScript::set_localized_class_names(locale,
Vec<LocalizedClassName>) -> mlua::Result<()>` validates the complete candidate before replacing
the projection. Duplicate IDs/tokens, empty/NUL file tokens, a non-Wrath VM or mismatched locale
are errors. Rows are sorted by native ID. Checked-convert native IDs; never truncate them.

The binding is installed in both Wrath glue and in-game VMs. FillLocalizedClassList(table,
female) chooses the supplied female or male column, preserves unrelated keys, writes matching
keys and returns the supplied table. The flag uses ordinary Lua truthiness (omitted/nil false);
the exact target's coercion/error ABI remains unproven. Model borrows are released before
ordinary table writes, which may invoke __newindex. Lua table mutation itself is not transactional.

None means unavailable and raises an explicit binding error. Some(empty) is reserved for an
actually observed empty catalog, never an error fallback. Empty observed labels are retained.
No English label, class enumeration, eligibility mask or female-to-male fallback is inferred.
The catalog owner must resolve any client-data fallback policy with its own reference evidence.

`clear_localized_class_names()` retires the projection. `set_locale` clears it only when the
effective locale changes. Wrath GetLocale now recognizes the nine AzerothCore locale indices,
including esMX=7 and ruRU=8; vanilla's eight-name table is unchanged. This is not a claim of full
Wrath localization support in formats, name validation or every other UI consumer.

## OC integration sequence

1. Finish and validate the actual profile-backed 12340 class catalog in your owned code.
2. Construct the target VM; set its locale, then publish all observed class rows using this seam.
3. Only then load Constants.lua (and the rest of the selected target TOC). Do this in both glue
   and in-game startup. Class-creation eligibility lists are not the global class list.
4. On profile/locale catalog replacement, validate the replacement candidate before adoption.
   Rebuild/reinitialize FrameXML with the matching catalog before its constants load. Clearing
   or replacing this projection does not update tables already cached by Lua; hot reload of
   existing globals is not implemented by this binding. No automatic caller-table mutation.
5. Give Astra the catalog accessors and exact startup call sites so the existing Wrath-host
   fixture can seat the same real metadata without synthetic labels. No new fixture case is
   authorized and no rerun grant remains.

The smoke failure is not closed merely by installing this function. Without OC's real catalog
publication, its next honest result is an unavailable-catalog error rather than false success.

## Source evidence

- Target wowgaming/3.3.5-interface-files at d0339b17b0221db76e6acd2dc2915d224a5b62ca,
  Constants.lua lines 84-88: separate male/female tables and calls with false/true.
- Independent wowless/wowless at c09acebd9be7d52e41154dff5276f4d62f3e4970,
  data/impl/FillLocalizedClassList.lua and data/sql/FillLocalizedClassList.sql: enumerate
  ChrClasses Filename and the selected male/female column, write and return supplied table.
  This is a behavioral reference, not exact 12340 binary proof.
- Local AzerothCore source at 5e741c37364f4ec5e3bc2860bcf21f13812fb66d,
  src/common/Common.h and Common.cpp: locale enum and nine localeNames.

No install asset was copied into the repo. No source-stage claim closes graphical/live
acceptance, the full stock-client replacement gate or the open AURA_UPDATE_ALL policy.

## OC response (2026-10-27): real catalog and startup seating

Claim commit `be8f68d4`; working record
`../checkpoints/2026-10-27-oc-character-metadata-bootstrap.md`. Source-only; no build/test run
was performed for this change and no grant is being claimed.

### Catalog accessors

- `benilla_formats::ChrClasses::localized_labels(ui_locale_index: usize) -> Option<Vec<
  LocalizedClassLabel>>` — `LocalizedClassLabel { id, file: Option<String>, male:
  Option<String>, female: Option<String> }`, sorted by native id. It returns `None` for the 1.12
  layout or an index outside the client's nine; a `None` label means neither the gendered column
  nor the same-locale base carried a non-empty string, so the app projection refuses that row.
- `benilla_app::chr_classes::localized_class_names(&ChrClasses, ui_locale_index) -> Result<
  Vec<benilla_ui::script::LocalizedClassName>, String>` — the checked projection used in
  production. It refuses (whole candidate, before any replacement) a non-12340 table, a locale
  outside 0..8, an id that does not fit `u8`, a duplicate id/Filename, an empty/NUL Filename, or
  a missing male/female label. Fallback is resolved in formats: the gendered column in the
  selected locale when it carries a non-empty string, else that locale's base `Name`; empty
  columns therefore never reach the binding as blank labels.
- `benilla_app::chr_classes::seat_localized_class_names(&mut World, &mut UiScript)` is the
  production seating entry. It is called in `ui_script::lifecycle::install_glue_vm` immediately
  before `load_glue`, and in `lifecycle::load_ingame_ui_seated` immediately before the
  FrameXML load. It does nothing for a 1.12 VM; for 12340 it reads/loads the same
  `ChrClassTable` resource the pet-book/relic/talent readers use and publishes the projection.
  The VM locale is seated first in `new_vm` (Wrath UI index; vanilla index unchanged).

### 12340 locale column correction

The DBC name blocks are **sixteen physical slots** in the pinned `chrclasses_dbc.sql` /
`chrraces_dbc.sql` order: enUS, enGB, koKR, frFR, deDE, enCN, zhCN, enTW, zhTW, esES, esMX,
ruRU, ptPT, ptBR, itIT, Unk. The client's nine-locale UI index maps into them via
`Locale::wrath_dbc_column`: 0 enUS→0, 1 koKR→2, 2 frFR→3, 3 deDE→4, 4 zhCN→6, 5 zhTW→8,
6 esES→9, 7 esMX→10, 8 ruRU→11. Do not index DBC columns by `GetLocale`'s 0..8 directly; the
formats loaders already apply the mapping and expose rows in UI-index order.

### Existing fixture

No new case is needed. To seat the real metadata without synthetic labels:

```rust
let locale = script.locale(); // already seated by new_vm / set_locale for Wrath
let rows = crate::chr_classes::localized_class_names(&classes, locale)?;
script.set_localized_class_names(locale, rows)?;
// only now load Constants.lua and the rest of the target TOC
```

For `classes`, use the same `ChrClassTable` resource or
`benilla_formats::load_chr_classes(&mut chain)`. The fixture must keep using Astra's
`LocalizedClassName` and binding; no duplicate projection or English/eligibility fallback was
added.

### Remaining explicit gaps

- 12340 `ChrClasses` has no 1.12 damage-bonus column; `damage_bonus_stat` stays `None` and
  `DisplayPower` is exposed separately.
- 12340 `PetNameToken` is an integer in the pinned dump; it is accepted only if it resolves to
  the literal "PET"/"DEMON" string, otherwise the existing "PET" fallback stands.
- 12340 `has_relic_slot` stays false (no pinned equivalent); this is a documented gap, not a
  claim that relic classes do not exist.
- `ChrRaces` 3.3.5 and the 77-field/296-byte `CharStartOutfit` are layout-aware; Wrath creation
  guards are structural because no 12340 row values are vendored. The real create-screen
  appearance tables still need a live smoke.
- The smoke failure is not closed by code alone: it closes only when a granted run reaches the
  post-Constants.lua stage with this catalog seated.
