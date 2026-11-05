# Project B architecture traceability baseline

Last updated: 2026-10-26, Europe/Moscow. Review anchor: `df8c7527`. Engine pin: Bevy 0.18.1. Baseline authorized by the owner: approved completion plan, existing contracts/issues and current source, in place of missing GDD/studio scaffolding.

This is a first traceability baseline, not an exhaustive stock-client feature inventory. It covers 17 queue requirements, 17 mandatory release families and three experimental platform families from `D:/WOW/PROJECT_B_COMPLETION_PLAN.md`. These 37 anchors overlap intentionally: a vertical proof can exercise several release families. They must not be summed into a product-completion percentage.

At review start there were zero formal GDDs, zero ADRs and no TR registry in the expected directories. Formal existing-ADR coverage is GAP for all 37 anchors. Existing prose contracts and actual code still provide architectural evidence; “no ADR” does not mean “no implementation.” ADR-0001 was authored afterward as Proposed and is not counted as accepted or independently reviewed coverage. No registry was created or altered.

## Matrix

Legend: Partial = component source exists, not accepted behavior; Broken = a concrete source contradiction was found; Unknown = this review did not establish the implementation/acceptance; Gap = required artifact absent. All runtime acceptance below remains unestablished in this review.

| Anchor | Approved requirement | Existing issue / story link | Source evidence | Implementation assessment | Existing ADR coverage |
|---|---|---|---|---|---|
| C01 | Real world session reaches native receiver | [16](../../.scratch/project-b/issues/16-real-session-to-receiver.md) | wm-native publisher; wm_native receiver/store | Partial; live receiver acceptance pending | GAP |
| C02 | WM consumes native state with fallback | [17](../../.scratch/project-b/issues/17-wm-consumes-native-state.md) | service runtime, context loader and fallback | Partial; production context proof pending | GAP |
| C03 | Exhaustive full replacement inventory | [18](../../.scratch/project-b/issues/18-stock-replacement-inventory.md) | Approved family list; this preliminary matrix | Gap; exhaustive behavior enumeration not complete | GAP |
| C04 | Manifest-selected route terrain | [19](../../.scratch/project-b/issues/19-profile-route-in-application.md) | profile source and chain; route pins | Partial; rendered acceptance pending | GAP |
| C05 | Render and animate WotLK actor | [20](../../.scratch/project-b/issues/20-wotlk-actor-rendered.md) | M2 v264, external SKIN and display chain | Partial; visual/corpus proof pending | GAP |
| C06 | Route water and collision | [21](../../.scratch/project-b/issues/21-route-water-and-collision.md) | MH2O and world asset paths | Partial; structures/navigation route proof pending | GAP |
| C07 | Render live replicated entities | [22](../../.scratch/project-b/issues/22-live-entities-rendered.md) | WorldDigest → ECS/GuidIndex/ObjectStore | Partial; visible connected lifecycle pending | GAP |
| C08 | Move/jump and reconcile | [23](../../.scratch/project-b/issues/23-local-movement-and-correction.md) | Player controller → wrath_stream_movement | Broken flag translation, R01 | GAP |
| C09 | Travel, communicate and reconnect | [24](../../.scratch/project-b/issues/24-route-travel-and-reconnect.md) | Parked character selection; movement path; bounded session reply delivery (R04) | Partial; route, communication and recovery proof pending; queue/reconnect source only, runtime UNKNOWN | GAP |
| C10 | Baseline attack and real spell | [25](../../.scratch/project-b/issues/25-ordinary-combat-slice.md) | Command tap and gameplay codec | Partial; R01/R03/R07 and connected outcome proof | GAP |
| C11 | Quest, loot, reward persistence | [26](../../.scratch/project-b/issues/26-quest-loot-reward.md) | Quest codec, item/container ECS, lossless Wrath inventory owner (`wrath_inventory`), typed item payload (`wrath_item`) and donor projection | Partial; R02 source-repaired at HEAD (`read_item_info` reads 2 damages and rejects oversized stats); R06 bank/bag and R06 item payload (12 slots, create-played-time, item text by guid) source-added, not run; persistence/re-entry pending | GAP |
| C12 | Death/recovery and refusals | [27](../../.scratch/project-b/issues/27-death-and-recovery.md) | Existing donor owners; Wrath scope lifecycle | Unknown complete Wrath path | GAP |
| C13 | WM typed action with override | [28](../../.scratch/project-b/issues/28-wm-typed-player-action.md) | Existing WM action owners and native observation source | Unknown complete client action/result path | GAP |
| C14 | Immutable typed Spellwright publication | [29](../../.scratch/project-b/issues/29-typed-live-graph-publication.md) | Publication contracts/plan; full implementation not audited here | Unknown at current runtime/source integration identity | GAP |
| C15 | Live A/B selection and rollback | [30](../../.scratch/project-b/issues/30-live-selection-and-rollback.md) | Declared live-rule contracts; no current proof inspected | Unknown; active-instance pinning/rollback evidence required | GAP |
| C16 | Client asset/Lua reload and rollback | [31](../../.scratch/project-b/issues/31-client-live-content-reload.md) | Candidate-first profile reload; existing UI owners | Partial; coherent consumer adoption/rollback not established | GAP |
| C17 | WM publishes client content revision | [32](../../.scratch/project-b/issues/32-wm-client-content-publication.md) | Assigned publication/receiver seams | Unknown complete versioned apply/result | GAP |
| Release account | Startup/account/character workflows | Issue 18; behavior children pending | Auto boot plus parked enumeration/pick | Partial; create/delete/customization/error matrix open | GAP |
| Release world | Identity, visibility, phases and appearance | Issue 18; issues 22/24 cover a subset | Pure reducer, field knowledge, generations | Partial; broad instance/phase/content coverage open | GAP |
| Release assets | Required rendering/audio/format corpus | Issue 18; issues 19–21 cover a subset | Profile, M2/SKIN, ADT/MH2O and renderer source | Partial; complete corpus and behavior acceptance open | GAP |
| Release movement | Walk/fall/swim/fly/transport/forced motion | Issue 18; issues 23/24 cover a subset | Shared controller and Wrath wire adapter | Partial with concrete R01 defect | GAP |
| Release combat | All required class/resource/combat state | Issue 18; issue 25 covers a subset | Spell/attack codecs and existing spell owners | Partial; auras/cooldowns/class semantics remain | GAP |
| Release pets | Companion control/ownership/persistence | Issue 18; behavior children pending | No complete Wrath pet path established by this review | Unknown | GAP |
| Release economy | Inventory/equipment/bank/services/trade/mail/auction | Issue 18; issue 26 covers a subset | Item/container ECS, lossless Wrath bank/bag owner, donor projection | Partial with R06 (bank/bag source-migrated; item enchant tail deferred); R02 source-repaired; broad service paths open | GAP |
| Release progression | Talents/glyphs/dual spec/skills/reputation | Issue 18; behavior children pending | Catalog support and donor UI owners | Partial; catalog parsing is not progression behavior | GAP |
| Release professions | Learn/gather/craft/material/refusal workflows | Issue 18; behavior children pending | Skill/focus/mechanic catalogs, donor trade-skill owners | Partial; authoritative Wrath behavior not established | GAP |
| Release interaction | Quest variants, objects, loot rules and services | Issue 18; issue 26 covers a subset | Gossip/quest/loot codec, narrowed object templates | Partial; variant/service/object semantics open | GAP |
| Release social | Chat/friends/guild/party/raid/loot distribution | Issue 18; issue 24 covers a subset | Channel catalogs and existing donor handlers | Unknown full Wrath behavior coverage | GAP |
| Release instances | Dungeon/raid/LFG/BG/arena lifecycle | Issue 18; behavior children pending | Generic scope/transfer foundation only in this audit | Unknown complete workflows | GAP |
| Release expansion | Runes/vehicles/phasing/achievements/calendar | Issue 18; behavior children pending | No full feature-chain audit here | Unknown | GAP |
| Release UI | Stock 3.3.5 UI/API/events/security/addons | Issue 18; early UI-host slice | Addon interface 11200, donor identity/API shapes | Incompatible donor contract found, R05 | GAP |
| Release controls | Bindings/macros/options/action bars/local state | Issue 18; behavior children pending | Existing persistence and action-button owners | Partial; Wrath reconnect/reload behavior pending | GAP |
| Release extensions | Exact required WM/addon builds | Issue 18; C13–C17 cover parts | Declared WM addon target list | Unknown exact build/workflow coverage | GAP |
| Release operations | Bounded queues/install/update/recovery/performance | Issue 18; operational children pending | Lifecycle ownership, profile rollback seam, bounded session-reply delivery (R04.a/R04.b) | Partial with R03/R04; packaging/performance evidence missing; session overload source-observed and repaired at source, runtime UNKNOWN | GAP |
| Platform combat | Negotiated server-authoritative experimental combat | Completion plan section 9 E | Offline model and existing server extension concepts | Model/source only; runtime choice/proof open | GAP |
| Platform building | Functional persistent building with collision/navigation | Completion plan section 9 B | Existing spawn owners/dynamic GO collision references | Conditional capability; navigation/lifecycle proof open | GAP |
| Platform generation | Generated spaces through authoritative owners | Completion plan section 9 G | Isolated procgen-v2 data generator | Data only; durable codec/consumer/geometry decisions open | GAP |

## Operational delivery (R04)

Addresses review finding R04 (unbounded gameplay-reply channel, 64/tick drain), re-prioritised to
P1 by the 2026-11-05 release plan; the dated review's P2 text stays intact. Anchors below are at
this integration HEAD; the package-B HEAD and pre-change (`46fd2f0c`) line numbers are quoted in
the contract document. Contract: [bounded session delivery](bounded-session-delivery.md).

| Row | Producer | Queue | Consumer | Overload terminal behaviour | Scenarios | Evidence |
|---|---|---|---|---|---|---|
| R04.a session-reply delivery | `run_session_with_cancel` (`wrath.rs:7457`) via `translate_gameplay` (2699) and the `send_glue` closure (7473) | `SessionReplies` (`wrath.rs:300`, created per worker at 1328): FIFO cap 4096, 2 reserved terminals, coalesced keys server-clock / played-time / gossip-menu / vendor-stock | `take_session_events` (1459) -> `wrath_dispatch_events` (4626) -> `route_wm_action_outcomes` (4916) -> `net::handlers::dispatch` (`net/handlers.rs:178`) -> glue/UI messages | coalesce observations -> <=50 ms cancel-checked read backpressure -> controlled `SessionEnded` + forced `Disconnected` at the hard cap; never a silent drop of an ordered reply while the session continues | steady state; stall/burst; refusal (`OutboxFull`/`ScopeChanged` unchanged); transfer; reconnect | SOURCE-OBSERVED (baseline) / PROPOSED and implemented at source (this policy) / NOT RUN: compiled, focused check, receiver, visible |
| R04.b terminal under overload | worker terminal paths (`wrath.rs:1362-1391`, `8337`) and panic synthesis (1911) | reserved terminal slot, delivered before the FIFO, one edge per kind | `wrath_dispatch_events` -> `on_disconnected` (`net/session.rs:345`) -> glue | terminal delivered ahead of the backlog; batch truncated at the terminal; reply/observation backlog discarded once the session ends; a clean logout's `LoggedOut` and its `Disconnected` teardown are both delivered | terminal with 0 backlog; with >4096 backlog; during transfer; during AppExit; worker panic | SOURCE-OBSERVED / implemented at source / NOT RUN |

## Inventory ownership (R06)

Addresses review finding R06 (four Wrath bank slots and one bank bag dropped by the shared
inventory projection, plus narrowed item enchant/suffix/socket semantics). The dated review's P1
release-gap text stays intact; the 2026-11-05 release plan re-prioritises the projection to P0
fidelity. Status below is source-level; no compilation, focused case, receiver or visible
gameplay was run. Contract: [lossless Wrath inventory](rfc-lossless-wrath-inventory.md).

| Row | Writer | Readers | Normal / refusal / reconnect / persistence | Status | Evidence |
|---|---|---|---|---|---|
| INV-01 bank slots 1..28 | `build_digest` -> `WrathInventory::apply` (`wrath_inventory.rs`), one caller `wrath_apply_world` | bank container feed, `item_watch`, `PlayerInventory` view | 28 delivered slots / absent field = unknown / new create repopulates / server-owned | SOURCE-REPAIRED (was 24/28) | SOURCE-OBSERVED; NOT RUN |
| INV-02 bank bags 1..7 | same | bank-bag containers 5..11, `BagCache` 11 seats | 7 delivered bags / unbought slot absent / repopulate / server-owned | SOURCE-REPAIRED (was 6/7) | SOURCE-OBSERVED; NOT RUN |
| INV-03 7th bank-bag purchase | `PLAYER_BYTES_2` byte 2 through the window | `GetNumBankSlots` full at 7, `ui_bank` purchase, DBC price row | buy -> count 7 / insufficient funds unchanged / re-read / persists | SOURCE-REPAIRED (cap/full were 6) | SOURCE-OBSERVED; NOT RUN |
| INV-04 `GetContainerNumSlots(-1)` | `ui_items::feed` | stock BankFrame | 28 rows / banker-closed semantics unchanged / reload re-push | SOURCE-REPAIRED (was 24) | SOURCE-OBSERVED; reference `ContainerFrame_Scripts.cpp:115`; NOT RUN |
| INV-05 bank bags 5..11 | `ui_items::feed`, `benilla-ui` container model | ContainerFrame, bag buttons | 7 bags / 12th absent / reload | SOURCE-REPAIRED (was 5..10) | SOURCE-OBSERVED; NOT RUN |
| INV-06 deposit slot 25..28 | `wire_pos_in_target(Wrath335)` -> tap | `CMSG_AUTOBANK_ITEM` | 39..66 / server refusal surfaces / no replay / server | SOURCE-REPAIRED (rejected before) | SOURCE-OBSERVED; NOT RUN |
| INV-07 withdraw slot 25..28 | same | `CMSG_AUTOSTORE_BANK_ITEM` | 39..66 / refusal / no replay / server | SOURCE-REPAIRED | SOURCE-OBSERVED; NOT RUN |
| INV-08 bank bag 7 deposit/withdraw | same | `CMSG_*_BANK_ITEM` | wire 73 / refusal / no replay / server | SOURCE-REPAIRED | SOURCE-OBSERVED; NOT RUN |
| INV-09 bank-bag button pickup | `BankButtonIDToInvSlotID`/`ContainerIDToInventoryID` (`benilla-ui`, target-aware) + `wire_pos_in_target` + `bag_verbs` | `EQUIPMENT_BAG` moves, `GetContainerNumSlots(11)`, `GetBagName(11)`, withdraw and swap | internal 67..73 / Lua 68..74 in Wrath (donor 63..68 / 64..69); out-of-range refused; reload | SOURCE-REPAIRED; installed-FrameXML/visible pickup remains runtime acceptance | SOURCE-OBSERVED; NOT RUN |
| INV-10 `HasKey` | `has_key_in_player` | keyring visibility, lock openers | finds keys in 25..28/bag 7 / missing template transient false / reload | SOURCE-REPAIRED (scan was 24/6) | SOURCE-OBSERVED; NOT RUN |
| INV-11 repair/cooldown scans | `ui_merchant::repair_costs`, `char_stats` bands | repair cost, tooltip cooldown | 28/7 / no vendor refusal change / reload | SOURCE-REPAIRED | SOURCE-OBSERVED; NOT RUN |
| INV-12 bank events | `item_watch::typed_bank_change`, `BagCache` 11 | `PLAYERBANKSLOTS_CHANGED`, `BAG_UPDATE(11)` | fires for 25..28/bag 7 / no event on unknown / cleared on transfer | SOURCE-REPAIRED | SOURCE-OBSERVED; NOT RUN |
| INV-13 item enchant slots 1..12 | `WrathEntity.item_fields` window + `wrath_item::WrathItemRow`, `push_item_fields` donor 7 only, `ItemRead` | tooltip/bind/link/charges/durability/item glow/item_watch 8..12 notice | 12 retained / donor 43..47 never overwritten / re-create keeps the row | SOURCE-IMPLEMENTED (S3); donor still 7 | SOURCE-OBSERVED; NOT RUN |
| INV-14 `ITEM_FIELD_CREATE_PLAYED_TIME` | `wrath_item::WrathItemRow::create_played_time` | none named (stored only) | retained only if S3 lands / n/a / re-create | SOURCE-IMPLEMENTED (S3), no consumer | SOURCE-OBSERVED; NOT RUN |
| INV-15 item text by guid | `ui_item_text::WrathItemTexts::read` → `ClientCommand::ItemTextByGuid` | `MessageReader<WrathItemText>` → `ItemTextOpen`/`feed_item_text` | request/response / `has_text == false` closes / reconnect retires | SOURCE-IMPLEMENTED (S3); `0x200` readable bit | SOURCE-OBSERVED; NOT RUN |
| INV-16 presence/clear | `WrathFieldWindow` + `WrathItemRow`/`ItemRead` | migrated readers | unknown != explicit clear; delivered zero clears; sparse omission retains | SOURCE-REPAIRED (dense zeros before) | SOURCE-OBSERVED; NOT RUN |
| INV-17 reconnect/transfer retirement | `wrath_apply_world` scope-loss block + `retire_wrath_item_texts` | owner, `BagCache`, `ItemLane`, containers, item rows, item-text cache | clear old scope / no stale event / new create | SOURCE-REPAIRED (owner and text cache join the existing clear) | SOURCE-OBSERVED; NOT RUN |
| INV-18 item/container ownership | `apply_item_world` + `WrathInventory` address map | `GuidIndex`, `ObjectStore`, `WrathItemRow`, typed address map | guid/generation identity / a bag move changes one address only / contents not reparented | SOURCE-REPAIRED (S3 row rides the same entity) | SOURCE-OBSERVED; NOT RUN |

Follow-up row (not this package): the raw player window stays `0..1171`, so
`PLAYER_FIELD_COMBAT_RATING_1` (1231..), `PLAYER_FIELD_MAX_LEVEL` (1279) and
`PLAYER_FIELD_DAILY_QUESTS_1` (1280..1304) read 0. Extending the window to `PLAYER_END` (1326)
would change several UI reads and needs its own consumer evidence; it is deliberately not
bundled with R06.

## Decision and test linkage

Proposed ADR-0001 addresses the boundary underlying C07–C13 and the release world/movement/combat/economy/progression/UI/controls/operations families. It does not implement those requirements or cover all C14–C17 transaction decisions. Independent review and explicit acceptance remain required. A Proposed ADR is not a satisfied implementation dependency.

Current local issues are the project's story/acceptance sources, not `production/epics` stories. Inline test modules exist in protocol gameplay, Wrath application, reducer and procgen files; donor UI gates exist under `benilla-app/src/ui_script`. This review did not establish a complete requirement-to-test mapping and did not run tests. File existence is never labeled a passing test. The earlier OC reports predate later source revisions and include exclusions/failures.

The next inventory pass must link independently assessable behaviors to exact source/decision/issue/test-case/evidence identities and classify normal, invalid, transfer, reconnect and persistence scenarios. No test files/cases may be created or run without an applicable explicit grant. Preserve existing issue IDs and any later registered TR IDs; do not duplicate issue 18 as a separate unrelated tracker.

## Required next decisions and dependencies

1. Lossless Wrath owner/UI boundary (ADR-0001, Proposed).
2. Critical-command admission, movement coalescing, reply backlog and scope-barrier policy.
3. Mode-aware stock UI/addon API/event contract and migration of narrowed item/combat owners.
4. Content revision staging/adoption/rollback across archive, catalog, GPU and UI consumers.
5. Durable generated request/artifact import and existing-owner publication contract.
6. Still-open owner decisions: restart boundary, experimental combat runtime, building navigation and runtime geometry.

There is no accepted ADR graph to topologically sort yet. This list is a recommended implementation dependency order derived from the approved plan and source findings, not a claim that nonexistent ADR dependencies have been accepted. No cross-ADR conflict could be proven with zero existing ADRs; the important conflicts here are implementation versus approved requirements and stale contract descriptions.

## Known uncovered surfaces

This matrix has not enumerated every opcode, API/global/event/argument, format variant, installed addon build, class/spec, instance mechanic or operational workload. Exact addon builds and live character/database state remain unresolved. These are open inventory inputs, not exclusions from release. No full-chain-complete or project-completion percentage is supplied.

See [the source review](architecture-review-2026-10-26.md) for prioritized findings and evidence limits, and [the Proposed ADR](adr-0001-wrath-compatibility-boundary.md) for the recommended boundary.
