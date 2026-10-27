# Wrath item queries and spell-link tooltip consumer handoff

Source delivery only. No compilation, new test cases, test runs or runtime operations were
performed. OC's in-flight chat/layout/bootstrap files are preserved. Full replacement and
rendered connected gameplay acceptance remain open.

## Committed Astra behavior

- b4ba2d25: Wrath GameTooltip:SetHyperlink accepts bare/escaped spell links through the existing
  tooltip_spell::set_spell_by_id renderer. It retains existing cache asks, waits, clear and late
  answer handling; no second tooltip store. Installed WM_MulticlassSpellbook.lua:33 calls it;
  WM_SpellmakingV3.lua:305 also post-hooks this method.
- 99928786: Wrath GetItemInfo returns name, colored link, quality, item level, required level,
  type, subtype, stack count, equip token, icon, sell price. GetItemIcon reads the same template
  view. Both use item_templates/item_stat_reads, with no new network or catalog owner.
  Unknown cache entries return no GetItemInfo values / nil icon; no guessed template appears.
  The donor query body and nine-value tuple remain selected for Vanilla112.
- Wrath quality 7 uses Artifact's light yellow, as the pinned AzerothCore ItemQualityColors
  table specifies; donor quality 7 still takes its existing Common fallback.
- 86e2b776: one shared Wrath ItemLink parser keeps the permanent enchant, three gem enchant IDs,
  reserved word, signed property, suffix factor and render level in their 12340 positions.
  Tooltip gem fields cannot become donor random-property IDs. Supported positive property
  lines use the existing catalog projection, with property slots mapped from donor 2..6 to
  Wrath 7..11. Named gem lines use socket slots 2..4. Raw source_link is retained for rebuild.
- 91c381f1: quest-log reward, craft-reagent and trade-skill product/reagent template link
  producers use the target-aware helper. The same payload formatter builds Wrath GetItemInfo's
  result link; template links have zero instance fields.

## Exact integration dependency for OC — do not count as complete

`benilla-app/src/ui_items/mod.rs::item_link_full` still emits four-field 1.12 links.
`object_link` supplies those links to the live container feed at `ui_items/feed.rs`, and the
same helper feeds inspect/trade/loot/auction surfaces. The new Lua parser deliberately does
not reinterpret short arbitrary Wrath input as a donor payload. A live-object producer must
emit the real nine-field payload, using the authoritative item's socket enchant IDs and signed
property/seed. The existing `.item_enchant(2..4)` projection needs inspection against the Wrath
item-field mapper before adopting it; do not assert lossless item replication from this handoff.

Carry UiTarget through the actual presentation owner rather than detecting a payload by its
length or selecting a protocol from an ambient environment variable in a generic formatter.
Do not change donor helpers globally: their existing callers/tests assert the donor layout.
No live-item feed, native item mapping, app producer or test assertion was edited by Astra.

Other open item behavior:

- Negative properties index ItemRandomSuffix and require the factor/scaling data path. Their
  signed ID and factor survive the query link, but their suffix text/stat scaling is not
  implemented by RandomPropertyView. No abs(id) lookup into the positive catalog is used.
- GetItemInfo lookup by localized item name is not implemented by this adapter.
- The shared tooltip renderer remains donor-oriented: socket visuals/bonuses/requirements,
  scaling item level and other Wrath line-law differences are not accepted by named gem lines.
- No current-source ItemRandomProperties/SpellItemEnchantment catalog acceptance was executed;
  names absent from those existing feeds cannot be invented by the adapter.
- GetItemInfo misses retain the donor's cache-read behavior. App feed_item_stats only serves
  a read when Items already has it. This change does not introduce automatic item-query sends.

## Primary source basis

Installed addon sources were inspected read-only, not copied:

- WM_Multiclass/WM_MulticlassSpellbook.lua:33 and :55 (spell tooltip/link).
  SHA-256 FDD215FF216F05E388BE94D8348CD7BE11026A4B664709A1EAAD62F2AB45381E.
- WM_Multiclass/WM_SpellmakingAdvanced.lua:518 reads GetItemInfo's icon at result 10.
- WM_Multiclass/WM_SpellmakingV3.lua:250 queries GetItemIcon for reagent icons.
- Bagnon/libs/LibItemSearch-1.0/LibItemSearch-1.0.lua:203 reads item level at 4 and equip slot at 9.
- Selected 3.3.5 FrameXML ItemRef.lua:183 passes its link to ItemRefTooltip:SetHyperlink;
  ContainerFrame.lua:661/686 and MerchantFrame.lua:488/506 query escaped item links.
  Public reference pin d0339b17b0221db76e6acd2dc2915d224a5b62ca:
  https://github.com/wowgaming/3.3.5-interface-files/tree/d0339b17b0221db76e6acd2dc2915d224a5b62ca
- Local AzerothCore source pin 5e741c37364f4ec5e3bc2860bcf21f13812fb66d:
  game/Chat/HyperlinkTags.cpp item::StoreTo establishes the link field positions and sign meaning;
  game/Entities/Item/Item.h establishes socket slots 2..4 and property slots 7..11;
  server/shared/SharedDefines.h ItemQualityColors establishes the heirloom color.
  The parser's introductory sign comment contradicts its branch bodies; the actual negative
  branch looks up ItemRandomSuffix, positive ItemRandomProperties. The adapter follows the code.

## Later verification scope, only after an explicit grant

The new source needs a UI crate check and target-specific evidence for numeric/bare/escaped
item queries, eleven-value ordering, signed property preservation, gem/property separation,
spell-link late-cache rebuild and donor behavior. No cases were created and no grant inferred.
An app-level test must additionally seat a real Wrath item feed; a synthetic Lua query cannot
prove the producer migration or the graphical/item-mod behavior.
