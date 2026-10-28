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

Follow-up `49de390e`: failure to load the separate Wrath ItemRandomSuffix table no
longer discards successfully loaded positive ItemRandomProperties rows. The existing
startup owner warns with the suffix load error; negative suffix lookups remain
unavailable. Failure of the positive table still prevents catalog admission. This is
source-inspected only; no table load, build or test was run for this change.

Further source trace at `cf4a7794`: `ui_loot::resolve_item` and `receive_line`,
`ui_loot_roll::snapshot`, and the inspect, trade and auction row builders still call
the four-field vanilla `item_link_full`. Wrath's `ItemLink::parse` instead reads
three gem words and a reserved word before the signed property and factor, matching
the checked AzerothCore `HyperlinkTags.cpp` item tokenizer. Consequently a nonzero
roll in one of these vanilla-shaped links is read as a gem, and the real property is
lost. Carry the actual `UiScript` target through these producer owners and render a
target-shaped link; preserve observed factors rather than inventing missing ones.
The complete set of producer paths and live behavior remain unverified. OC owns
protocol/world replication, so do not repair dropped wire fields in this UI slice.

`ee45bf7a` carries the active UI target to loot-window rows and item-receipt chat
links, using a shared target-aware formatter. Wrath links place the signed property
and receipt factor in their target positions; vanilla output is preserved. Window
rows still have no retained suffix factor and emit zero. Loot rolls, inspect, trade
and auction remain separate producer work. Source-only: no build or test has run.

`2ca39ead` additionally selects the active target for group-loot snapshot links.
That owner retains the property but not the suffix factor, which remains zero.
Group-loot announcements are a separate localized format-string path and remain
unchanged pending evidence of the Wrath format contract. No build or test ran.

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

At this checkpoint negative ItemRandomSuffix was still absent. The later catalog and
name work below adds partial support; native descriptor/link losses and complete stat
projection remain open. Neither checkpoint establishes full item-system acceptance.

### Negative-suffix retained data and shared names (ae6b8953, source only)

The existing RandomPropertyCatalog now retains a separate private ItemRandomSuffix row
set when the positive-property table selects the known 12340 layout. Its strict 29-field
schema follows AC DBCStructure.h / DBCfmt.h: ID, sixteen localized names, mask/internal
name, five enchant IDs and five allocation percentages. Rows retain enchant/allocation
data even when the selected locale has no name. No cross-language name fallback is added.
Originally the selected Wrath catalog required both tables. Follow-up 49de390e retains
positive properties if the suffix table fails and reports the failure at startup; negative
suffix lookups remain unavailable. Donor tables and from_rows fixtures retain an empty
suffix set.

suffix_name(signed_id) resolves positive IDs only in the property table and negative IDs
only in the suffix table. The existing get/iter/len/is_empty contract still describes the
positive table. scaled_suffix(negative_id, factor) returns five (enchant_id, amount) pairs
with the AC u32 AllocationPct * factor / 10000 arithmetic, including explicit zero slots.
It does not infer an item seed, apply stats or format enchant text. The public operation
needs the caller's real retained factor; raw-link/loot factor-loss repairs remain open.

The app's existing item_display_name now asks suffix_name, so all callers of this shared
formatter can join an observed negative suffix using their existing ITEM_SUFFIX_TEMPLATE.
The donor constructor's negative-ID/plain-name behavior remains unchanged because its
suffix set is empty. There is no new resource, cache, allocation owner or native mutation.

At this checkpoint still not connected: negative suffix names in Lua's raw GetItemInfo
path (subsequently connected below); substituting scaled amounts into localized enchant text; restoring property slots
7..11 in native item descriptors; preserving the suffix factor in loot replication.
Do not report this as complete negative-suffix tooltips, item stats or server behavior.
No build/test/new case, table read/probe, generator or runtime execution occurred.

### Raw-link Lua suffix names (719d1b6d, source only)

Wrath GetItemInfo now resolves a negative random-property ID through the existing
lent CallHost -> ScriptCallHost -> ItemSteps -> RandomProperties catalog. ItemSteps
adds one optional read-only access to that existing resource. No second suffix cache
or all-table VM push is introduced. The catalog's actual selected-locale suffix joins
the cached base item name through the existing VM ITEM_SUFFIX_TEMPLATE, and the link
formatter retains the signed property, factor, gems and render level supplied by
the query. Positive properties and donor mode retain their existing paths.

When the host/catalog/localized suffix is unavailable, the negative-property query
returns no GetItemInfo result, rather than presenting the base name as a completed
rolled name. GetItemIcon still reads the cached icon independently. No amount, seed
or stat is inferred from the suffix label. New SystemParam admission, exact query
ABI, cached/uncached addon behavior and multilingual output remain unverified.

The local AC spellitemenchantment_dbc.sql and itemrandomsuffix_dbc.sql contain schema
definitions but no row content establishing client text-token substitution. The
server allocation arithmetic establishes numeric amounts, not how native 12340
renders its localized enchant strings. No guessed token-replacement renderer was
added. Native property-slot and loot-factor loss still block complete item projection.

No builds/tests/new cases, imports/extractors, table loads, generators or runtime
operations occurred. The next concrete Questie dependency found is its compatibility
wrapper's call to the absent global GetItemCooldown(itemID), independently of the
GetContainerItemCooldown and GetSpellCooldown APIs already present. Coordinate that
item query with OC's active cooldown campaign before changing its presentation owners.

### Raw item cooldown query (cd071ceb, source only)

Installed QuestieCompat.lua:201-205 calls the absent global GetItemCooldown(itemID),
and TrackerLinePool.lua:619 consumes its GetTime start/duration/enable triple. The
Wrath item installer now registers that binding and delegates through the lent host
to existing SpellQueries. That owner already carries Items, Cooldowns, Spells, UiClock
and the adopted Wrath phase; no resource parameter or recovery state was added.

The query requires an active adopted world and a cached template. For an on-use item
it resolves the template's actual use spell/category, reads Cooldowns::info_item with
the item entry and category override, then projects through its existing ui_triple.
The binding converts ms to seconds and enable to the legacy numeric flag. Known
expired/cold recovery yields (0,0,1); a known template with no use spell yields the
existing container query's no-leg (0,0,0). A missing host/world/template/spell definition/
clock/recovery resource stays unavailable. It sends no query packet and does not infer
permission or ownership of a physical copy from an entry query.

Held/on-event enable facts come from the existing owner; no local clock, countdown or
retry mechanism was created. Exact 12340 raw non-use-item/invalid-ID/held-start ABI and
unavailable-data behavior still need acceptance, as do category/GCD/initial cooldown
cases. This query does not implement SetCooldown or sweep rendering, and does not
modify OC's active widget/recovery source. Donor registration is unchanged.

Installed QuestieCompat.lua SHA-256:
D40F086185C11B681438AC7311DE56F029B70A7DEAF5E9094251F6A5939F7F9D.
No builds/tests/new cases, imports/probes, generator or graphical/live runtime ran.

### Wrath secondary-stat presentation (50654b1c, source only)

The app template view already retains ItemInfo.stats verbatim, but the donor renderer
visits only 0/1/3..7 and silently drops every target rating/secondary value. The new
Wrath-only equip-stat block maps AC ItemTemplate.h ItemModType 12..39 and 41..48 to
their actual 12340 GlobalStrings.lua keys: defense/avoidance/hit/crit/haste ratings,
resilience/expertise, attack power, regeneration, penetration, spell power and block
value, including the target's deprecated healing/damage IDs. ID 40 is explicitly not
in 3.3 per the AC enum; no modern mastery/PvP-power ID was invented from addon strings.

The block consumes existing wire amounts, retains source order, and uses the existing
green on-equip/wrapped presentation adjacent to spell equip lines. It does not compute
rating percentages, scale heirlooms, apply gameplay bonuses or reconstruct negative
suffix stats. Primary-stat, resistance and donor paths were left unchanged. Exact
native ordering/color/combining rules and complete target line law remain unverified.

Installed zzzmopitemtooltips/core.lua rewrites many ITEM_MOD_* globals from a numeric
sentence to %c%d plus the SHORT label. The new helper uses existing fill_exact with
sign/absolute-value arguments or the stock numeric argument, so a %d hole cannot
accidentally print the sign string. The strict formatter retains positional/escaped
specifier behavior; an unsupported format remains unavailable rather than guessing.
The addon retains a separate spell-power path for scaled heirlooms; that behavior is
not implemented or accepted by this change.

Primary string source read:
https://github.com/wowgaming/3.3.5-interface-files/blob/d0339b17b0221db76e6acd2dc2915d224a5b62ca/GlobalStrings.lua
Installed zzzmopitemtooltips/core.lua SHA-256:
218579C6A110284CCF736D8839B01375B16A4E9A87FF114FBD84060626A84ABB.
No install code or string values were copied into the repo; only identifier mappings
and formatting logic were implemented. No builds/tests/new cases or runtime checks
occurred. One bounded check/existing-UI-test pass has been requested explicitly; a
pending request is not authorization and no execution starts without the human answer.

### Keyring loss attribution corrected and UI/direct paths repaired (c095756b)

The prior bank/keyring limitation wording grouped two different facts. Current source
proves Wrath push_player_inventory_fields already retains all 32 keyring GUIDs via
64 dwords (raw 496 -> donor 648), and wrath_item_pos translates all donor wire slots
81..112 to 86..117. AC Player.h declares keyring 86..118 exclusive and a fixed capacity
of 32. The native keyring data was not lost. Bank still drops four seats and one bag.

The actual keyring limits were in UI/item consumers: the donor level ladder capped
the feed at <=16; GetContainerNumSlots rejected container -2; wire_pos rejected seats
17..32; HasKey compared donor BagFamily 9, whereas AC uses BAG_FAMILY_MASK_KEYS 0x100.
Stock 12340 ContainerFrame.lua:GetKeyRingSize reads GetContainerNumSlots(-2), inspects
the occupied seats, then sizes its visible rows. It must run against a true capacity,
not be replaced by a hard-coded visible-window size.

c095756b adds target-aware size/position/key-family helpers. Wrath feed and raw free-
seat/item-count queries retain all 32 positions. GetContainerNumSlots admits -2 only
for Wrath and reads the actual container snapshot. Direct item move/split/autostore/
use/repair/destroy/equip paths and their bind-answer replay pass the current UI target
to position mapping; seat 32 maps to donor 112, then the existing native adapter maps
it to server 117. The helper does not map invalid seat 33 onto another inventory band.
No native/protocol source or server action was edited or executed.

Donor level sizes, wire_pos entry point and old counters retain their original limits
and existing cases; no new assertions/cases were added. HasKey's donor helper remains
as the existing test entry point; production now calls the target-aware helper.

Remaining callers intentionally not claimed as migrated: legacy merchant/trade/action
owners still call the donor wire_pos, and their existing count/find-item walkers still
use donor keyring limits. Those need coordinated target-aware integration for late-seat
selling/trading/action lookup; the direct item repair is not whole-keyring acceptance.
Scaled suffix/tooltip, full bank and other recorded item gaps remain open. No compilation,
tests/new cases, generators, table loads or runtime runs occurred. The bounded verification
request is still pending; source progress does not constitute authorization.
