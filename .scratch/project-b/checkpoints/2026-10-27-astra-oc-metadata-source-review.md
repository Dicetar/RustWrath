# OC metadata source review — locale remap counter-evidence

2026-10-27, Astra. Source-only review of current owner WIP on wotlk, observed after
53b2b938/563e3f18. This is not a completed-implementation or test review; the seven OC-owned
app/formats files and the dirty class-name handoff were read and preserved, not edited/staged.

## [P1] Do not use generated SQL column labels to remap DBC locale slots

Current `crates/benilla-formats/src/locale.rs::Locale::wrath_dbc_column` returns
`[0, 2, 3, 4, 6, 8, 9, 10, 11]` for the UI's nine locale indices. Both pending
`chr_classes.rs::table_from_wrath` and `chr_races.rs::table_from_wrath` use that mapping to
select physical columns. The pending handoff calls this a 12340 locale-column correction.

The pinned AzerothCore source at 5e741c37364f4ec5e3bc2860bcf21f13812fb66d contradicts it:

1. `src/common/Common.h::LocaleConstant` is enUS=0, koKR=1, frFR=2, deDE=3, zhCN=4,
   zhTW=5, esES=6, esMX=7, ruRU=8. `Common.cpp::localeNames` has that same order.
2. `src/server/shared/DataStores/DBCfmt.h` declares sixteen consecutive `s` fields for
   the class/race base-name block. `DBCStructure.h` stores those as `name[16]`.
3. `src/common/DataStores/DBCFileLoader.cpp::AutoProduceStrings` walks physical format
   fields in order and reads `getRecord(y).getString(x)` into each next string pointer.
   `DBCFileLoader.h::Record::getString` reads that physical field's raw offset. There is
   no enGB/enCN/enTW insertion or per-language column remap in this binary reader.
4. `src/server/game/World/World.cpp::World::DetectDBCLang` iterates i over TOTAL_LOCALES,
   tests `race->name[i][0]`, and records `localeNames[i]` and bit `1 << i` as that DBC
   language. This is an active consumer of the race-name array, not an unused/commented
   struct declaration. It requires the first nine physical name slots to match the locale
   constants directly. The default locale is assigned from this same index.

The generated SQL table may label sixteen string columns with a generic regional-language
list. Its labels cannot override the actual binary loader and its active indexed consumer.
For example, the pending koKR→2 mapping reads the slot the server identifies as frFR; frFR→3
reads deDE; ruRU→11 reads a slot outside the server's supported nine-language index range.
English slot zero would still work, so an enUS-only smoke could miss this defect.

## Required owner correction

Use direct physical slots 0..8 for the supported nine-locale projection, or supply stronger
12340 binary/data evidence that explains the contradiction before retaining another mapping.
Keep the full sixteen-column layout/flags and gender/base offsets; only the proposed locale
indirection is contradicted here. Reconcile the pending catalog docs and consumer handoff so
Astra's fixture does not adopt the false interleaving contract. Preserve the existing vanilla
locale path and fail explicitly for unsupported/missing localized data under the agreed policy.

This finding does not authorize a binary extractor, live DBC probe, new tests or execution.
The requested focused regression grant is still unanswered. Existing enUS/compile evidence
cannot prove non-English locale selection. No source patch was made to OC's WIP and no direct
message was sent to another task. The full-replacement gate remains open.

## Narrow repair claim after owner commit

OC subsequently landed this mapping in b543578d. The three affected formats source files
are now clean; remaining owner WIP is customization.rs and lib.rs. Astra claims only
locale.rs::wrath_dbc_column and the associated locale/class/race source comments for an
identity-slot correction, retaining the helper signature, layout branches and catalog owners.
No table parsing, gender fallback, app/bootstrap, runtime or dirty owner file is claimed.
The target indices are derived from the loader/active consumer above, not SQL column labels.
The owner's committed handoff section describing interleaved slots is superseded by this
counter-evidence and repair record. No new test cases or execution are authorized.
