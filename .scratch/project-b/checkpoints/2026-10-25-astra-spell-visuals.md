# RELEASED FILE CLAIM: Astra — WotLK spell visual catalogs

Status: SOURCE IMPLEMENTED; UNCOMPILED / UNTESTED. Delivery is the commit containing this checkpoint and its two source files. OpenCode remains integration lead. This slice follows spell-catalog delivery `4d9973b2`.

Files held exclusively during the slice, now released to OpenCode:

- `crates/benilla-formats/src/spell_visual/mod.rs`
- `crates/benilla-formats/src/spell_visual/chain_effects.rs`
- This checkpoint.

Both source files were clean when claimed. No application/render/protocol files are claimed. Existing production `load_spell_visual_catalog` remains the integration path.

Scope: explicit vanilla/WotLK table decoding for the existing visual model. Keep known common stages, kit effects, procedures and beam prefix usable without misreading shifted columns. Unsupported added WotLK rendering behaviors stay explicit compatibility gaps.

Sources: local AzerothCore `DBCStructure.h::SpellVisualEntry` and primary WoWDBDefs definitions for SpellVisual, SpellVisualKit, SpellVisualEffectName and SpellChainEffects, specifically the build ranges containing 3.3.5.12340. Do not infer a compatible layout from additional trailing columns alone.

No tests, builds, imports, linters, new tests, probes, game or visual execution are authorized or run by Astra. Delivery will be a scoped commit with source-review evidence and limitations.

## Delivered

The existing `load_spell_visual_catalog` loads explicit supported layouts, with file-specific refusal for unsupported field counts:

| Table | Vanilla | WotLK | Consumed behavior |
|---|---|---|---|
| SpellVisual | 16 fields | 32 fields | WotLK channel is 6; missile gate/model/attachment/sound are 7/8/10/11; strike sound and flags are 12/13. The added StateDoneKit at 5 is not misread as ChannelKit. |
| SpellVisualKit | 35 fields | 38 fields | Common effects map from 3-8 and 11-13; world/sound/shake from 14/15/16; procedure keys begin at 17 and float parameters at 21. Added weapon effects are not misbound to special slots. |
| SpellVisualEffectName | 5 fields | 7 fields | Model/name string columns remain 2/1; WotLK area/scale columns are typed as floats. |
| SpellChainEffects | 8 four-byte fields | 48 fields, 177-byte rows | Explicit packed reader validates header/body, size arithmetic and texture offsets; reads the supported first eight beam fields without applying an invalid 192-byte schema. |

The vanilla schema/record helpers used by existing fixtures retain their signatures and behavior. No dependencies, public struct fields, application startup paths, render scheduling or action interfaces were changed. The current production caller is `benilla-app/src/creature_anim/spell_visual.rs::load_spell_visuals`.

## Evidence and limitations

Source reviewed against these primary definitions, using their build ranges containing 3.3.5.12340:

- [SpellVisual](https://github.com/wowdev/WoWDBDefs/blob/master/definitions/SpellVisual.dbd)
- [SpellVisualKit](https://github.com/wowdev/WoWDBDefs/blob/master/definitions/SpellVisualKit.dbd)
- [SpellVisualEffectName](https://github.com/wowdev/WoWDBDefs/blob/master/definitions/SpellVisualEffectName.dbd)
- [SpellChainEffects](https://github.com/wowdev/WoWDBDefs/blob/master/definitions/SpellChainEffects.dbd)

Compared the main visual prefix with local AzerothCore `DBCStructure.h::SpellVisualEntry`. Reviewed schema slot counts, typed fields, shifted indices, existing fixture-helper callers and the scoped Git diff. Checked that the byte and DBC helpers used by the packed reader are already dependencies. These are source reads, not executed checks.

The current renderer model is a subset: WotLK StateDone/caster-impact/target-impact and added area kits; weapon-effect slots, start animations and kit flags; effect scale constraints; and additional beam color/wave/joint controls are not implemented here. Legacy area fields are zero because their exact WotLK counterpart was removed; unrelated WotLK kit IDs are not substituted for model IDs. The first eight beam fields retain the current legacy shape/timing interpretation, which still needs WotLK behavior acceptance. Existing beam parameter/procedure dispatch and missile-flag/attachment semantics likewise require runtime compatibility evidence. This is partial catalog/render compatibility, not proof that every WotLK spell visual works.

When separately authorized, prioritize vanilla regression; real WotLK four-table load; channel versus StateDone index distinction; special versus weapon effect slots; procedure type/float decoding; a packed beam row and bad texture offset; malformed/unsupported row refusal; then one actual cast/missile/impact in the connected application. No tests were created or run by Astra.

All source changes in this slice are complete and ready for OpenCode to consume from the shared checkout. Preserve the outstanding renderer gaps in issue 18's compatibility accounting.
