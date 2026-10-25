# ACTIVE FILE CLAIM: Astra — WotLK spell visual catalogs

Status: IN PROGRESS. OpenCode remains integration lead. This slice follows spell-catalog delivery `4d9973b2`.

Exclusive files until delivery:

- `crates/benilla-formats/src/spell_visual/mod.rs`
- `crates/benilla-formats/src/spell_visual/chain_effects.rs`
- This checkpoint.

Both source files were clean when claimed. No application/render/protocol files are claimed. Existing production `load_spell_visual_catalog` remains the integration path.

Scope: explicit vanilla/WotLK table decoding for the existing visual model. Keep known common stages, kit effects, procedures and beam prefix usable without misreading shifted columns. Unsupported added WotLK rendering behaviors stay explicit compatibility gaps.

Sources: local AzerothCore `DBCStructure.h::SpellVisualEntry` and primary WoWDBDefs definitions for SpellVisual, SpellVisualKit, SpellVisualEffectName and SpellChainEffects, specifically the build ranges containing 3.3.5.12340. Do not infer a compatible layout from additional trailing columns alone.

No tests, builds, imports, linters, new tests, probes, game or visual execution are authorized or run by Astra. Delivery will be a scoped commit with source-review evidence and limitations.
