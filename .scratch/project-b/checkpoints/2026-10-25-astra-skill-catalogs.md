# RELEASED FILE CLAIM: Astra — WotLK skills and spell requirement names

Status: SOURCE IMPLEMENTED; UNCOMPILED / UNTESTED. Delivery is the commit containing this checkpoint and its three source files. OpenCode remains integration lead. This slice follows spell visual delivery `a2cde9bb`.

Files held exclusively during the slice, now released to OpenCode:

- `crates/benilla-formats/src/skill_lines/mod.rs`
- `crates/benilla-formats/src/spell_focus.rs`
- `crates/benilla-formats/src/spell_mechanic.rs`
- This checkpoint.

These source files were clean when claimed. No application/action/protocol, shared DBC helpers, fixture/test files or other catalog files are claimed.

Scope: explicit vanilla/WotLK layouts in existing production skill/spell-name loaders. Preserve default APIs, vanilla behavior, optional-table fallback and unchanged race/class rows. Read shifted localized text/icon/order columns correctly and account for the removed ability column without losing the retained CharacterPoints values.

Sources: local AzerothCore `DBCStructure.h`/`DBCfmt.h` and the primary WoWDBDefs skill/focus/mechanic definitions for builds including 3.3.5.12340. No runtime behavior acceptance is implied by schema support.

No builds, tests, new tests, linters, probes, imports, services or visual/game execution are authorized or run by Astra. Source edits and handoff will be committed using this explicit file allowlist, then the claim released.

## Delivered

| Table | Vanilla layout | WotLK layout | Consumed mapping |
|---|---|---|---|
| SkillLine | 22 fields | 56 fields | Name starts at 3 in both; description starts at 12/20; icon is 21/37. |
| SkillLineAbility | 15 fields | 14 fields | Main ability/rank/mask columns remain 0-11. Removing Abandonable at 12 shifts the retained CharacterPoints pair to 12/13; the legacy training-point read follows its second word at 14/13. |
| SkillLineCategory | 11 fields | 19 fields | Name begins at 1; sort index follows the locale block at 10/18. |
| SpellFocusObject | 10 fields | 18 fields | ID + eight/sixteen localized names + flags. |
| SpellMechanic | 10 fields | 18 fields | ID + eight/sixteen localized state names + flags. |

Each required loader rejects unsupported field counts with a table-specific message. The existing DBC parser still checks record size and body bounds. Optional skill categories retain their existing empty-map fallback for unreadable/unsupported data. The unchanged eight-column SkillRaceClassInfo loader remains in use. No public API, application/UI files, protocol state, locale selection, shared DBC helpers, dependencies or tests were changed.

The existing production call paths are `ui_spellbook::load_skill_lines`, `ui_action::load_spell_mechanics`, and the focus catalog consumed by trade-skill/cast-refusal presentation. This is a parser compatibility contribution, not a separate startup path.

## Source evidence and validation limits

Compared with local AzerothCore `DBCStructure.h::SkillLineEntry`, `SkillLineAbilityEntry`, `SkillRaceClassInfoEntry`, `SpellFocusObjectEntry`, and corresponding DBCfmt declarations. Cross-checked the 3.3.5 build definitions:

- [SkillLine](https://github.com/wowdev/WoWDBDefs/blob/master/definitions/SkillLine.dbd)
- [SkillLineAbility](https://github.com/wowdev/WoWDBDefs/blob/master/definitions/SkillLineAbility.dbd)
- [SkillLineCategory](https://github.com/wowdev/WoWDBDefs/blob/master/definitions/SkillLineCategory.dbd)
- [SkillRaceClassInfo](https://github.com/wowdev/WoWDBDefs/blob/master/definitions/SkillRaceClassInfo.dbd)
- [SpellFocusObject](https://github.com/wowdev/WoWDBDefs/blob/master/definitions/SpellFocusObject.dbd)
- [SpellMechanic](https://github.com/wowdev/WoWDBDefs/blob/master/definitions/SpellMechanic.dbd)

Reviewed the scoped Git diff and every schema-helper call site after their private signatures changed. No build, test, linter or runtime operation was run. Real profile data and the resulting spellbook/skill-pane/refusal strings remain unverified. Current locale selection remains the original eight locales; understanding a sixteen-slot record does not expand that policy. Existing race/class mask, rank-chain, profession and training-point UI semantics are preserved and are not independently established as complete 3.3.5 behavior by this patch.

Future checks, only when authorized: vanilla regressions; WotLK line name/description/icon resolution; distinct category sort values; correct ability successor and shifted CharacterPoints; focus/mechanic labels; malformed layout refusal; optional category fallback; then actual stock spellbook/skill-pane behavior in the connected client.

Astra's edits are complete for this file claim. OpenCode may consume the committed files directly; no cherry-pick or repeated implementation is needed.
