# ACTIVE FILE CLAIM: Astra — WotLK spell catalog

Status: IN PROGRESS. Owner: Astra. OpenCode remains integration lead.

The owner requested prompt commits and explicit file claims to prevent double work. Prior range/form support is committed at `85fd3a8c`; this is the next, separate slice.

Reserved files (do not edit concurrently):

- `crates/benilla-formats/src/spells/mod.rs`
- `crates/benilla-formats/src/spells/layout.rs` (new)
- `crates/benilla-formats/src/spells/dispel_types.rs`
- This checkpoint.

The existing main Spell.dbc loader assumes the 173-column vanilla layout; the WotLK layout has 234 columns. This slice adds explicit source-column selection for the existing catalog model, retaining localized text, effects and source metadata without passing WotLK rows through vanilla offsets. The required dispel-type name table is included. No application, network, action-policy or shared DBC helper edits are claimed.

Primary local source: AzerothCore `src/server/shared/DataStores/DBCStructure.h::SpellEntry` and `DBCfmt.h::SpellEntryfmt`. Extended WotLK-only mechanics remain compatibility work, not proof of complete spellcasting.

No builds, tests, linters, new tests, probes or runtime operations are authorized by this assignment. Source changes will be committed with explicit file paths and their untested status. Preserve all other staged/dirty work.
