# RELEASED FILE CLAIM: Astra — WotLK spell catalog

Status: SOURCE IMPLEMENTED; UNCOMPILED / UNTESTED. Delivered in the commit containing this checkpoint and its three source files. Owner: Astra for this completed slice; files are now available to OpenCode, who remains integration lead.

The owner requested prompt commits and explicit file claims to prevent double work. Prior range/form support is committed at `85fd3a8c`; this is the next, separate slice.

Files claimed exclusively during this slice, now released:

- `crates/benilla-formats/src/spells/mod.rs`
- `crates/benilla-formats/src/spells/layout.rs` (new)
- `crates/benilla-formats/src/spells/dispel_types.rs`
- This checkpoint.

The existing main Spell.dbc loader assumes the 173-column vanilla layout; the WotLK layout has 234 columns. This slice adds explicit source-column selection for the existing catalog model, retaining localized text, effects and source metadata without passing WotLK rows through vanilla offsets. The required dispel-type name table is included. No application, network, action-policy or shared DBC helper edits are claimed.

Primary local source: AzerothCore `src/server/shared/DataStores/DBCStructure.h::SpellEntry` and `DBCfmt.h::SpellEntryfmt`. Extended WotLK-only mechanics remain compatibility work, not proof of complete spellcasting.

No builds, tests, linters, new tests, probes or runtime operations are authorized by this assignment. Source changes will be committed with explicit file paths and their untested status. Preserve all other staged/dirty work.

## Delivered

- The existing `load_spell_catalog` now selects exact 173-column vanilla or 234-column WotLK schemas. `layout.rs` maps the fields already consumed by the catalog to the source version's actual columns. It preserves float types, localized names/ranks/descriptions, effect arrays, cooldown/cost/range indices, learn effects and languages. Unknown layouts fail explicitly. The normal DBC parser retains record-size/body validation.
- WotLK's removed CastUI field is not misread from Mechanic or SpellPriority. Removed BaseDice/DicePerLevel are adapted to the source roll's unit-die semantics; these are derived display inputs, not extra on-disk columns.
- `SpellDisplay::school` stays a primary index for existing callers. `SpellCatalog::school_mask(id)` retains the entire WotLK mask, and `spell_family_flags_96(id)` retains all three words; legacy constructors retain their previous signatures and default the third word to zero.
- `load_spell_dispel_types` supports vanilla's 12 columns and WotLK's 21: the latter has sixteen localized name slots, flags, Mask, ImmunityPossible and InternalName. Mask/InternalName are read from 18/20, not vanilla 10/11. This loader is called by the main spell catalog, so its old schema was an additional prerequisite failure.

## Source evidence and limits

Compared the reader with local AzerothCore `DBCStructure.h::SpellEntry`, `DBCfmt.h::SpellEntryfmt`, `SharedDefines.h::GetFirstSchoolInMask`, and `SpellInfo.cpp::SpellEffectInfo::CalcValue`. Dispel layout checked against the primary [WoWDBDefs definition](https://github.com/wowdev/WoWDBDefs/blob/master/definitions/SpellDispelType.dbd), build range `3.0.1.8303-3.3.5.12340`. Reviewed the source diff and catalog construction sites. No code was compiled or executed; no tests were added or run.

This delivers catalog reading, not complete WotLK spell semantics. Existing vanilla action/immunity/modifier consumers still need hybrid-school/full-family-mask decisions, added WotLK attribute/aura/rune rules, negative-die tooltip edge cases and 3.3.5 level-scaling behavior. The locale owner still exposes only its original eight selections even though this reader understands sixteen stored string slots. Further auxiliary DBC incompatibilities and real-asset acceptance remain possible; do not call the entire UI/catalog path runtime-accepted from this source patch.

OpenCode can consume these files directly without cherry-picking or reimplementing the layouts. Relevant future authorized checks: vanilla catalog regression; a real 234-column spell row with nonzero visual/name/rank/cost/effect values; hybrid-school and third family-word retention; 21-column dispel table; absent/removed field behavior; unsupported/truncated layout refusal. Astra edited no application/network/protocol/WM files in this slice.
