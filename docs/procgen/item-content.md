# Generated item content authoring

Source implementation in `content.rs` and `content_allocation.rs`, under the active six-hour campaign. No build, new test cases, generator invocation, WM import or runtime operation has been performed. This is an intermediate subsystem, not completion of that campaign or production acceptance.

## Existing owners

The adapter follows the inspected `D:/WOW/wm-project/src/wm/items/models.py` ManagedItemDraft/ItemStatLine, item validator/publisher, workbench DraftEnvelope, reserved/models.py and ReservedSlotDbAllocator. The native field meanings and column types were inspected in the BridgeLab AzerothCore ItemTemplate.h and base db_world/item_template.sql. No WM/server source is changed and no allocator/publisher is implemented here.

## Item plan and conversion policy

`ItemContentPlan::build(candidate, supported_build, mapping_policy)` revalidates the retained candidate and builds a diagnostic item plan. The policy pins candidate/revision/player, bonding/required-level/class/race rules and per-kind base template/display identities. Kind mappings retain database snapshot, base record, asset profile and display record SHA-256 locators and an inherited-fields review. These are caller-supplied reviewed pins, not independent database/archive verification. Weapon class/subclass/inventory combinations and supported armor shapes are checked explicitly.

Every generated stat requires a per-kind mapping with destination, positive scale, units/interpretation, review locator and numeric policy. Targets are an existing 12340 item stat type, armor, weapon damage, or explicitly unsupported. Missing/unsupported meanings remain per-item diagnostics. There is no default assumption that generic Power means spell power, CritChance means rating, or CritDamage has a stock item-stat equivalent.

Exact conversion requires reconstruction of the original source f32 bits. Optional `nearest` conversion requires an explicit maximum error in source units; the plan retains the original Stat, full mapping, target number, reconstructed bits and exactness flag. Integer overflow, excessive error, zero WM stat lines and duplicate target aggregation refuse the item. Source provenance remains lossless even when an explicitly chosen server conversion is quantized; quantized server values must not be described as identical semantics. Weapon damage stores the mapped value as equal min/max, without inventing a spread or DPS/delay conversion. Damage mapping requires a weapon and an explicit school; an armor item's generic Damage bonus has no inferred meaning.

The plan retains every item and its issues. A blocked item has no unallocated draft. `require_all_ready` refuses the entire subsequent allocation request when any item remains unresolved. No silent item/stat omission or implicit partial batch occurs. Plan encode/decode rederive the complete body from candidate and policy. Proposal numeric fields retain typed float bits until final materialization into the existing owner's decimal JSON.

The adapter clears inherited stats/spells, random/scaling fields, damage/armor/resists, sockets, quest/page/lock/reputation/skill dependencies and other explicitly enumerated generated-content defaults. It sets StatsCount to the actual mapped stat count. Delay/durability/material/sheath are explicit mapping fields. Remaining inherited base fields still require the pinned review and WM final-row preflight; this is not proof that cloning an arbitrary template is safe. Missing required database columns must be caught by the owner, not treated as successful conversion. Class/race masks and template/display availability are live owner responsibilities.

## Allocation and draft materialization

`AllocationRequests::build` emits inert requests matching the existing allocator's entity type, arc key, character GUID, source quest and notes arguments. The allocation key includes the mapping/build revision fingerprint, original item identity and source index. Changed mapping revisions therefore do not silently bind to the previous request key. The existing owner must perform deduplication and actual reserved-slot allocation; this library never calls it or derives a destination ID.

`ReservationEvidence` is a retained handoff containing plan identity, owner revision/report digest and actual ReservedSlot records. Validation requires one unique fresh `staged` item slot per source index, matching allocation key/player, positive non-base destination ID and no quest binding. Active/retired/free slots are refused. These checks do not attest database truth or visible-ID history; the owner must verify those before publication. A partially allocated batch remains a reconciliation task, not permission to allocate its successful requests again.

`AllocatedContent::build` produces ordinary workbench-compatible item envelopes from the validated plan and reservations. The nested draft contains the actual owner-reserved item entry; no invented ID or client-authoritative loot is introduced. It clears spells/stats, uses mapped fields and carries provenance notes. The outer package remains `requires_owner_preflight: true`. Verification rederives the envelopes; decimal damage values are compared after conversion to their actual server f32 representation. The typed conversion audit remains the exact source of their bits.

Publication, reload/client queries, inventory grant, delivery, snapshot/rollback and reserved-ID retirement remain with existing WM owners. No delivery action is emitted here. A reserved slot or draft file is neither a published template nor an item held by a player. Newly client-visible failure/retirement must follow the existing fresh-ID policy rather than recycling a previously visible ID.

## Prepared verification and known source gaps

Pending authorized checks: compile; mapping-plan and allocated-envelope roundtrip; exact versus explicit bounded rounding; zero/overflow/unsupported/duplicate target refusal; wrong item kind/display/base pins; stale/duplicate/missing/active/retired reservation refusal; all-ready batch refusal; actual WM validation/final-row comparison; retained ownership and authoritative item persistence. None was executed or written as a new test case.

The existing generator's v2 generic part pools can attach weapon-oriented Damage bonuses to armor and unrelated weapon kinds. This adapter exposes those unsupported meanings rather than fixing them by projection. A version-aware generator repair and retained-v2 compatibility remain work in the six-hour campaign. Source item plans, allocation requests and WM drafts do not close baseline inventory fidelity or full stock-client replacement.
