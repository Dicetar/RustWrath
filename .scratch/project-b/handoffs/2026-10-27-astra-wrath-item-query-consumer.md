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

### Supersession: live bags and own equipment now source-connected

Astra continued after the original handoff, with exact clean-file claims:

- 535ac85f adds ui_items::object_link_in_target and connects all five production container
  walks (backpack, carried bags, bank, bank bags, keyring) through resolve_slot_in_target.
  UiTarget comes from the actual script; render level comes from the current player store.
  The original donor resolver and its existing tests are unchanged. Cached and uncached
  links retain their respective fields and follow the existing bag diff/event owner.
- 98e00eed adapts feed_char's completed own-equipment/equipped-bag/bank-bag snapshots through
  the same formatter. Template-only ammo uses UiScript::template_item_link, which delegates
  to the same target-aware template helper as quest/crafting links, with no fake item object.

The formatter reads the authoritative projected item's permanent enchant, socket slots 2..4,
signed property and seed. Wrapped items retain the existing zero-instance-fields behavior.
No ambient WOW_WRATH lookup or payload-length heuristic selects the format.

Both commits are source-only and uncompiled. Their scope is presentation, not native item
authority, negative-suffix scaling or complete bank support. The following original dependency
paragraph still applies to unmigrated inspect/trade/loot/auction surfaces; its live-bag and
own-equipment portion is superseded by these commits.

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

### Native projection counter-evidence for OC

Source inspection of wrath.rs at 98e00eed confirms explicit truncation, not speculation:

- push_player_inventory_fields copies 48 dwords from 12340 bank base 402: only 24 of 28 slots.
- It copies 12 dwords from bank-bag base 458: only six of seven bag slots.
- push_item_fields copies 21 enchant dwords from base 22: seven of twelve enchant slots.
  The first permanent/socket slots used by the new link formatter survive, but property
  enchant slots 7..11 do not. Do not describe the resulting live item view as lossless.
- The descriptor projection separately preserves the property ID/seed, so links can retain
  their numeric identity without proving the missing suffix/stat presentation.

The matching UI baseline also has BANK_SLOTS=24, BANK_BAGS=6 and BANK_BAG_SLOT_COUNT=6.
Simply increasing a UI constant would walk the wrong descriptor band; OC needs a coordinated
native/view/command mapping change before the extra slots can be presented or acted on.
None of these native mappings or bank capacities was edited by Astra in this continuation.

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

### 2026-10-28 tooltip callback identity preservation

fdfd077a repairs the concrete GetItem callback dependency in installed WM_MulticlassHunt.lua
lines 100-101: OnTooltipSetItem reads the link to recognize item 910031. The source views
already had links, but several tooltip setters dropped them. GetItem also preferred the base
template name over the instance's suffixed name.

The existing ItemBuild now retains source-view links from loot/roll blocks, trade sides,
auction listings, merchant/buyback rows, owned action items, auction-sell items and outgoing
mail items. HeldObject carries the same link from its container/equipment view, including
the existing uncached-link case. Cache rebuilds update it alongside the held instance rather
than keeping an older link while replacing the item's other fields.

Wrath GetItem returns the instance/source name before the base template name. A real
template-only build may use the existing target-aware template link formatter. A block or
instance with no source link does not receive a fabricated zero-enchant/zero-property link.
The getter can use an existing held-object link when the build had not retained one.

This preserves the source-view bytes as supplied; it does not upgrade the legacy formats
still emitted by unmigrated native/app producers. Inbox block sources without a source link
remain incomplete. Callback timing, clear/hide behavior, server permissions and the native
item authority were not changed. OnTooltipSetItem still fires through its existing owner
after the item build is retained.

Installed WM_MulticlassHunt.lua SHA-256:
56E15F9AB6C4375C6E4CC4DDC493B8998F1277342F415406B2E31606CD9DE0CD.

No tests/new cases, build, linter, import, game/addon execution or runtime check was performed.
Rust partial-move/type correctness, rebuild behavior and Hunt's actual tooltip decoration
require later authorized evidence; source connection is not addon acceptance.

### 2026-10-28 class restrictions and heirloom titles

3bc849ae fixes the renderer's donor-only class list in Wrath mode. CLASSMASK_ALL_PLAYABLE
in pinned AzerothCore SharedDefines.h includes IDs 1..9 and 11, including Death Knight (6).
The renderer now uses that mask and the existing locale-bound class-name projection, rather
than CLASS_NAMES' nine English donor labels. Consequences from source:

- A Death Knight-only mask (0x20) has a requirement label instead of an empty list.
- The donor all-class mask (0x5df) is correctly a restriction in Wrath, because it excludes DK.
- The full Wrath playable mask (0x5ff) has no class-restriction line.
- Missing/stale-locale/empty required labels produce an explicit unavailable error before
  item lines are emitted; no fallback English or silently omitted allowed class is inserted.

The helper reads the male labels from the already seated projection, then releases Model
before Lua-facing line rendering. This is not a second catalog. Exact 12340 native gender-label
selection ABI has not been established by this source change. Race-restriction labels still
use the donor eight-race list; Blood Elf/Draenei and localized race coverage remain open.
Item permission checks and server authority are untouched.

The same commit maps Wrath heirloom quality 7 to the established Artifact color for cached
tooltip titles. f11e74e3 delegates all six pending-template title producers to the same
target-aware palette. Name-only builds retain their existing white override, and vanilla
quality-7 fallback remains Common. No cache-wait, event, template or item-state owner changed.

These commits are source-only, uncompiled and untested. No new cases, renderer/GUI run,
catalog extraction or other execution was performed. Existing regression results predate them.

### 2026-10-28 race-restriction supersession

8f61e99e supersedes the preceding donor-race-list gap in source. Existing
ChrRaces::localized_labels supplies the selected-locale male/base names for playable IDs
1..8, 10 and 11. Goblin (9) is not treated as playable. UI bootstrap reuses an existing
ChrRaceTable, or loads/inserts the same parsed catalog ahead of startup as the class path does.
Both production metadata seating and the from-chain fixture entry point seat the projection.

UiScript::set_localized_race_names validates a candidate before replacing a typed VM host-data
snapshot: current Wrath locale, nonempty labels, unique IDs and all ten playable IDs. No Lua
global or second catalog is introduced. The existing set_locale owner retires this projection
when locale changes, alongside class-label retirement. VM destruction drops it normally.
OC's dirty Model and loader files were not edited.

The tooltip renderer uses the full playable mask 0x6ff and observed names. Blood Elf-only
(0x200) and Draenei-only (0x400) restrictions can now produce labels; donor mask 0xff no longer
counts as unrestricted in Wrath. Missing locale-bound labels fail before line emission.
Vanilla keeps its eight-race mask and existing labels. Race/class permission checks and the
server's authority are untouched. Exact native gender-label selection remains unproven;
the projection deliberately uses the catalog's male/base column, with no cross-locale fallback.

Source references are the already parsed 69-column ChrRaces layout and pinned AzerothCore
SharedDefines.h race IDs (Blood Elf=10, Draenei=11). No extractor, build, test/new case,
import, graphical/live/socket/WM/server/DB operation was run. In particular, the stricter
from-chain metadata admission and non-English catalog coverage still need authorized evidence.

### Spellmaking cooldown query follow-through

74a250ba closes another concrete installed-addon call mismatch: WM_SpellmakingV3.lua:296
queries GetSpellCooldown(id), while the donor binding required (bookSlot, bookType).
The Wrath installer now accepts the raw ID/name forms. Name lookup uses the existing book
resolver and retains player/pet identity. Numeric IDs query the existing spell catalog and
player Cooldowns owner directly, including spells not present as a visible book rank.

Call chain: wrath_spell -> lent CallHost::spell_cooldown -> ScriptCallHost -> SpellQueries ->
the existing player/pet cooldown list's info/ui_triple, projected with the same UiClock pair
as feed_spellbook. A missing host, catalog row, active world or required resource returns nil,
not an invented ready tuple. A resolved cold read returns (0, 0, 1). Held reads retain the
existing query-time start and disabled flag; active/expired shaping shares cooldown_tuple
with the original book binding. Explicit book-slot/pet-book calls retain that original path.

No cast, query packet, timer, retry state, new cooldown list or resource owner was added.
No test cases/assertions were changed. This source is uncompiled and untested. The added
read-only SystemParam access still needs a granted host/app check; source inspection alone
does not prove Bevy admission or live query timing. Initial-cooldown coverage, server facts,
Wrath cooldown widget rendering and complete Spellwright/native extension acceptance remain
separate requirements.

Installed WM_Multiclass/WM_SpellmakingV3.lua SHA-256:
C6BDF814772A8F9EA8EED56DE0AD65CA3942563E885F76D018487D170FAFB35C.

### Remaining loot projection loss found during continuation

At 74a250ba, the Wrath loot-response decoder itself reads and discards _random_suffix
(protocol-wrath/src/gameplay.rs). Its LootItem and the downstream ProtocolLootItem carry no
factor; the donor shape's documentation assumes the server always sends zero. The wire has
the separate word, but it is already gone before wrath.rs translates the event. Migrating
only ui_loot's hyperlink formatter cannot restore it. OC must preserve the word from the
decoder through both contracts before any complete rolled-loot link/tooltip claim. No loot
protocol or adapter file was edited in this continuation.

The new source needs a UI crate check and target-specific evidence for numeric/bare/escaped
item queries, eleven-value ordering, signed property preservation, gem/property separation,
spell-link late-cache rebuild and donor behavior. No cases were created and no grant inferred.
An app-level test must additionally seat a real Wrath item feed; a synthetic Lua query cannot
prove the producer migration or the graphical/item-mod behavior.

### Tooltip subject continuation (2c101af6, source only)

Installed Questie-335 Modules/Tooltips/Tooltip.lua:389-391 invokes GetUnit, GetItem and
GetSpell on every GameTooltip update. GetUnit and GetSpell were not registered; this is
a direct call failure before its object-tooltip classification can finish. Its
TooltipHandler.lua:18 also consumes the returned unit token to read UnitGUID, and the
installed gearscorelite.lua:146 consumes the unit name. Questie's Tooltip.lua:343 hooks
OnTooltipSetUnit, which the existing unit renderer did not emit.

The Wrath-only getters now read subjects retained in TooltipState, without a second
registry or changes to OC's Model. A unit query requires the displayed GUID still to
match the retained token's current snapshot; it does not accidentally identify a new
target while an old tooltip remains. Its name uses the existing UNKNOWNOBJECT policy
on an observed unit whose name is absent. A spell query returns its actual ID and
observed view/fallback name; unresolved names stay nil. The existing spell callback
sees that identity, including after a delayed view answer. Talent renders seat their
resolved spell identity too. The existing content clear retires both subjects, so
SetOwner/hide and content replacement cannot retain an old item/unit/spell subject.
The unit renderer now emits OnTooltipSetUnit through the existing Wrath-gated dispatcher.

Questie Tooltip.lua SHA-256:
9741D0C43A9AAB1BF81D55CA51209B9DFB968E6D582208C44F9350489E05D25D.
Only local addon call sites establish this requirement; the exact native 12340 getter
implementation, alternate-token selection after retargeting and aura-as-spell identity
have not been recovered. No aura behavior was guessed. No new tests were written and
no build/test, addon, graphical or live run occurred. This does not establish addon
initialization or playable-alpha acceptance, and the full replacement gate stays open.

### Bagnon container return shape (03b3e7b6, source only)

GetContainerItemInfo was still donor-only: five returns, despite installed Bagnon
utility/itemSlotInfo.lua:18, utility/itemEvents.lua:87/118 and the pinned 12340
ContainerFrame.lua:684 reading the seventh as an item link. Bagnon uses it for
SetItem/hasItem, cache identity, slot-change comparisons and ITEM_SLOT_* messages.
The Wrath path now appends the loot flag and the existing ContainerSlot link (cached
or uncached), preserving its real instance fields and the upstream format as supplied.
The donor five-value path and empty-slot return count are unchanged.

Lootability reads ItemTemplateView.flags & 0x4, the AzerothCore ItemTemplate.h
ITEM_FLAG_HAS_LOOT bit. A template miss records only the existing item_stat_reads;
the app feed services that through Items::template(..., Ask::Read), without a query
packet. No new cache or inventory simulation was added. Existing first-five tuple
semantics were retained; exact native 12340 binding recovery and runtime acceptance
remain open. No claim is made that all upstream link producers are now Wrath-shaped.

Installed Bagnon utility/itemSlotInfo.lua SHA-256:
7B187BAED73C602B5FDEF0A46A273BF6D6315EE61FE36655D2E2DC204C8E0EEB.
Primary UI call site:
https://github.com/wowgaming/3.3.5-interface-files/blob/d0339b17b0221db76e6acd2dc2915d224a5b62ca/ContainerFrame.lua#L684

Remaining concrete inventory dependencies found in the same source trace:
- Bagnon utility/itemEvents.lua:179 calls GetContainerNumFreeSlots for the bag family;
  that API is absent and ContainerState currently carries no family projection. Do not
  invent zero/general-bag classification for every specialist bag.
- Questie TrackerLinePool.lua:559/628/631 calls GetItemCount(itemId, nil, true) for
  quest-item charges; no API is registered. The existing donor inventory walk/count
  helpers have their own scope/whole-item rules, and full Wrath bank replication still
  narrows upstream. Neither a sum over UI bag snapshots nor action_count alone proves
  the target include-bank/include-uses contract.
- Stock 12340 Cooldown.lua calls native SetCooldown. The current factory maps Cooldown
  to Frame and OC's fallback global only sets Lua fields/show state. The inherited
  donor M2 cooldown renderer does not establish a native Wrath Cooldown implementation.
  A faithful widget/render repair requires coordinated edits to OC's active widget/model
  work; no no-op SetCooldown or replacement of that dirty work was introduced here.

No builds, tests/new cases, generators, addon or graphical/live runs occurred.

### Bag-space owner integration (8e919cec, source only)

Wrath GetContainerNumFreeSlots now delegates through the already lent CallHost and
ScriptCallHost to ItemSteps. That owner reads actual GUID seats from ObjectStore/Objects
and the real bag_family mask from Items::template(..., Ask::Read). An occupied GUID whose
item object has not arrived still counts occupied; a missing bag object, template, seat
field or host remains unavailable rather than becoming an empty ordinary bag. A known
unequipped bag returns (0,0). Bank access follows the existing UI bank gate. No new ECS
parameter, UI snapshot field, cache, registry or mutation/query packet was added.

This supplies Bagnon's itemEvents.lua:179 dependency using existing ownership. It does
not repair the upstream donor projection: 24 bank seats, six bank bags and the current
level-sized keyring remain the available native view. The seventh Wrath bank bag and
full bank/keyring capacity need the native owner's lossless projection before complete
bank compatibility can be accepted. No compilation/tests/new cases or runtime acceptance
ran; the OC smoke reported in the next handoff predates this change.

### Item-count integration (0d2029f7, source only)

Installed Questie TrackerLinePool.lua:559/628/631 requires GetItemCount(itemId,nil,true)
for quest-item uses. Installed Skillet ShoppingList.lua:137 and SkilletStitch-1.1.lua:
177/179 require link queries and optional bank counts. No GetItemCount global existed.
The Wrath item installer now resolves numeric IDs, item links and uniquely cached exact
names, then delegates through CallHost/ScriptCallHost to ItemSteps and the existing
inventory GUID traversal. It adds no inventory owner, cache, mutation or query packet.

The shared traversal now accepts a predicate. Existing donor callers still select
item_is_whole, preserving action/reagent/quest behavior. The new possession query does
not discard broken items: AC PlayerStorage.cpp Player::GetItemCount at the existing
5e741c37 source baseline counts owned entries without a durability/whole-item gate.
That is server-side evidence for possession, not recovery of the native Lua binding.
Bank inclusion walks the already retained arrays even when the banker is closed;
it does not claim to restore the four bank seats/seventh bank bag lost upstream.

Uses are stack count times the absolute first-instance charge count when the existing
ItemInfo::has_finite_charges says the template is charged, otherwise stack count.
This reuses the donor action query's template/instance facts, with stack multiplication
matching the published ClassicAPI backport's includeUses description. The exact 12340
native positive-charge/depleted/stacked-charge behavior is not yet independently
recovered or accepted. Missing required item objects/templates/fields or a missing
lent host returns unavailable, rather than asserting a known zero. Ambiguous/uncached
names remain unavailable. Numeric IDs with an observed empty inventory can return zero.

The existing donor helpers and assertions were not changed; the shared traversal body
was retained with its original predicate selected by the old entry point. No test case,
build/test, generator, addon or runtime run occurred. The OC PlayerFrame smoke predates
this commit and does not exercise this query. Questie/Skillet behavior remains unproven;
in particular their expectations during unavailable host/data periods need acceptance.

Reference consulted for the backport's stated contract (not exact 12340 evidence):
https://octowow.st/git/brues/ClassicAPI/src/commit/f119f448ed45e6b09240ce46f725f3b092cbfbf9/docs/API.md

### Property/enchant catalog admission repairs (82999308, fd5d7479)

Tracing the unimplemented negative-suffix family exposed two earlier admission blockers:
the random-property loader accepted only 16 donor fields; the enchant loader accepted
only 24. AC DBCStructure.h / DBCfmt.h at the existing source baseline define 24 and 38
respectively for 12340. The strict shared parser rejects mismatched field counts.
entities.rs:988 logs a property-load failure and does not insert RandomProperties;
its enchant branch similarly omits Enchants on failure. feed_random_properties requires
both before pushing the table, so repairing only the property loader is insufficient.

82999308 selects the exact 24-field ItemRandomProperties layout: unchanged ID/internal
name/five-enchant positions, sixteen localized names at 7..22 and mask at 23. The
existing selected-locale mapping chooses the 12340 locale slot. The donor schema and
eight-locale path remain intact; other counts still fail strict schema admission.

fd5d7479 selects the exact 38-field SpellItemEnchantment layout: charges at 1,
effect groups at 2..13, sixteen names at 14..29, name mask 30, visual 31, flags 32,
then GemID, condition and three requirement words. It reads names/visual/flags at
these target offsets rather than donor offsets 13/22/23. AC Item.cpp uses the
field-32 ENCHANTMENT_CAN_SOULBOUND bit 1. Existing catalog interfaces and consumers
are unchanged; no app loader, native descriptor or renderer was edited. Target
interpretation of other enchant-flag bits is not independently established here.

Source evidence is the pinned AC structures/formats plus the current strict parser,
startup calls and UI feed. No actual DBC read, import/probe/extractor, build/test/new
case, generator or runtime execution occurred. Real wm-hd catalog admission, localized
names, linked/held-item tooltips and enchant glows remain unverified.

Negative ItemRandomSuffix is still absent: its separate 29-field table carries
five enchant IDs and allocation percentages; a seed-scaled amount/name implementation
still needs a complete supported contract and the native descriptor/link losses already
recorded above. The positive-property repair must not be reported as negative-suffix
support, complete stat projection or full item-system acceptance.
