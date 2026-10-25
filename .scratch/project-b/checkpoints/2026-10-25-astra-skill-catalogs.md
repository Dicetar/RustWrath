# ACTIVE FILE CLAIM: Astra — WotLK skills and spell requirement names

Status: IN PROGRESS. OpenCode remains integration lead. This slice follows spell visual delivery `a2cde9bb`.

Exclusive files until delivery:

- `crates/benilla-formats/src/skill_lines/mod.rs`
- `crates/benilla-formats/src/spell_focus.rs`
- `crates/benilla-formats/src/spell_mechanic.rs`
- This checkpoint.

These source files were clean when claimed. No application/action/protocol, shared DBC helpers, fixture/test files or other catalog files are claimed.

Scope: explicit vanilla/WotLK layouts in existing production skill/spell-name loaders. Preserve default APIs, vanilla behavior, optional-table fallback and unchanged race/class rows. Read shifted localized text/icon/order columns correctly; do not treat removed training-point fields as new data.

Sources: local AzerothCore `DBCStructure.h`/`DBCfmt.h` and the primary WoWDBDefs skill/focus/mechanic definitions for builds including 3.3.5.12340. No runtime behavior acceptance is implied by schema support.

No builds, tests, new tests, linters, probes, imports, services or visual/game execution are authorized or run by Astra. Source edits and handoff will be committed using this explicit file allowlist, then the claim released.
