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
