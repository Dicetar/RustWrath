# First connected loop behavior and evidence inventory

Date: 2026-10-26, Europe/Moscow. Owner of inventory: Astra. Implementation owner for every client row: OpenCode, using existing server/client owners. Initial source anchor: `c42cbe2d`; reconciled through `e0be0d95` for OC's cooldown/channel, aura, item-query and death deliveries. Later in-flight OC work is excluded until a cohesive handoff is reviewed. This reconciliation checks affected source seams, not every newly added parser variant.

Purpose: refine entry, movement, combat, inventory/quest, death and re-entry into independently assessable behaviors. These 56 local behavior IDs are inventory references, not new issue numbers or TR-registry entries. They are a subset of approved issue 18 and the release requirement; they are not a complete stock-client denominator. Preserve the original issues and the Proposed status of ADR-0001.

## Evidence and acceptance rules

All rows have acceptance **UNKNOWN** in this inventory. No build, test, new test file/case, socket, game, renderer, receiver or WM operation ran. Source support is marked Partial; a static contradiction is Broken; a missing Wrath tap arm is Missing; insufficient inspection is Unknown. Existing or reported tests are identified separately, never counted as accepted connected behavior.

Every behavior requires a pinned source/binary/profile, intended account/character and server-side outcome. A UI animation, sent packet, item-push notification, or matching static SQL does not establish authoritative state or persistence. Reconnect/persistence refers to the same declared character, not whatever character happens to be first in an enumeration.

Case notation: N = normal flow; R = refusal/malformed/stale/duplicate flow; T = transfer/scope transition; P = logout/re-entry/persistence. Where a behavior itself is transient, P means clean reconstruction/reset rather than saving the transient action.

Pinned scenario from OC: quest 6, Deputy Willem 823/base spawn 79942, Garrick 103/base spawn 80247, required item 182, reward choice 0/item 6076, Fireball 133, eligible human mage. Quest 5261 is the explicit-accept control because quest 6 has AUTO_ACCEPT in base SQL. Real character state, actual live DB/spawns/loot/flags, reachability and source/profile identity are not established by this inventory.

## Source map

Paths below are relative to `D:/Projects/project-b-client` unless an absolute server path is given. Use named functions at the pinned commit because line numbers move during OC work.

| Ref | Existing owner / source |
|---|---|
| S1 | `crates/benilla-app/src/wrath.rs`: WrathControl, ownership/pick admission, parked run loop, roster_from |
| S2 | `crates/benilla-protocol-wrath/src/gameplay.rs`: parse_char_enum, typed builders/readers, ServerEvent |
| S3 | `crates/benilla-protocol-wrath/src/session.rs` and `reducer.rs`: phases, delivered fields, world/session identity, authoritative replica |
| S4 | `crates/benilla-app/src/wrath.rs`: wrath_apply_world, build_digest, scope_is_older, wrath_dispatch_events |
| S5 | `wrath.rs`: wrath_tap_commands, shared WrathOutbox, wrath_stream_movement; `benilla-app/src/player/**` |
| S6 | `wrath.rs`: push_player_inventory_fields, push_item_fields, apply_item_world; existing ItemObject/ObjectStore/Inventory/BagCache owners |
| S7 | `gameplay.rs`: read_item_info, loot/item-push/quest readers; `benilla-protocol-wrath/src/cast_failure.rs` |
| S8 | `benilla-app/src/spell/**`, `ui_action/**`, `net/handlers.rs`, donor quest/loot/inventory owners, selected SessionEvent handlers |
| S9 | `benilla-app/src/net.rs`: stock ClientCommand variants, including item mutation and corpse/recovery commands |
| S10 | `benilla-ui/src/script/{target.rs,addon_gate.rs,event.rs,wrath_spell.rs,wrath_unit.rs}`, `framexml.rs`, `loader/**`, `benilla-app/src/ui_script/**`: selected host identity/API/event contract; Astra source follow-up through `b6d427e8` |
| S11 | Pinned server: `D:/WOW/WM_BridgeLab/src/azerothcore/src/server/game/Handlers/{Item,Spell,Quest,Movement,Character}Handler.cpp`, `Entities/Item/ItemTemplate.h`, `Entities/Unit/UnitDefines.h` |
| S12 | `docs/architecture/architecture-review-2026-10-26.md` R01–R10, latest OC delivery checkpoints and the scenario checkpoint |

## Entry and character selection — issue 24 and full account gate

| ID | Behavior | Source assessment | N and R cases | T and P cases / dependencies |
|---|---|---|---|---|
| E01 | Authenticate the intended account | Partial, S1; credentials from protected environment | N correct account/endpoints; R invalid/absent credentials and unreachable server produce a usable visible result | P no credential disclosure or stale worker; account ownership must be pinned |
| E02 | Decode and display the real roster | Partial, S1/S2; count-driven 23-slot enumeration | N zero/one/multiple records and appearance; R truncated record/count or failed enumeration does not invent a character | P fresh roster after logout; no prior-scope appearance reuse |
| E03 | Enter the intended character on first boot | Partial; Auto selects the first record in S1, not the scenario's declared mage | N explicit intended selection; R empty roster/unsuitable first record cannot silently become the scenario character | P same GUID must be reselected; initial selection remains a full-client gap |
| E04 | Re-enumerate in parked character select | Partial, S1: Enumerate → CharacterList with cause | N request yields current roster; R stale asks outside CharSelect are refused without killing the parked link | P repeat after clean logout and changed roster; O1 prerequisite |
| E05 | Validate an Enter World pick | Partial, S1 validates against the current enumeration | N picked known GUID sends player login; R removed/unknown/stale GUID does not enter another character | T prior session retired first; P fresh roster validation each time |
| E06 | Establish received self/world state | Partial, S3/S4 login phase, self GUID and digest | N server confirms map/position and player ownership; R login failure/incomplete self state stays ineligible | T no old-map entities/observations; P received state rebuilt, not cached as authority |
| E07 | Present the actual player and nearby actors | Partial, S4 plus profile/M2/SKIN source | N actual GUID/display/appearance/body and player camera; R missing assets diagnosed instead of accepted cubes | T remove/recreate by world and object generation; P consistent appearance restoration; issues 19–22 |
| E08 | Keep donor and Wrath UI host contracts explicit | Partial source integration, S10; explicit target/dialect/interface, production selection and selected addon API paths; full host remains incomplete | N 3.3.5 API/event/addon identity matches behavior; R unsupported interfaces fail explicitly | T no stale glue/game host state; P addon/settings state restored; ADR-0001 remains Proposed |

## Movement and targeting — issues 23–25

| ID | Behavior | Source assessment | N and R cases | T and P cases / dependencies |
|---|---|---|---|---|
| M01 | Walk/run forward/backward and stop | Partial, S5; ordinary direction bits coincide | N server accepts direction/pose/stop and controller agrees; R rooted/control-lost/inactive input sends no invalid move | T old outbox cleared; P fresh time/flags/self GUID after entry |
| M02 | Strafe and turn | Partial, S5 edge opcode selection | N left/right/stop transitions reach authoritative pose; R simultaneous edges do not leave stuck motion | T scope-specific controller; P controls return neutral |
| M03 | Jump and land | Broken, S5/S11/R01: donor falling bit reused as Wrath falling | N correct Wrath falling flag, launch/fall tail and landing; R malformed/invalid jump refused without invented landing | T clear local movement history; P no old fall state |
| M04 | Swim and leave water | Broken, S5/S11/R01 maps swimming to flying | N real liquid boundary, correct swim flag/pitch/speed; R no unauthorized flight/unsupported tail | T new-map water/controller state; P correct received pose; issue 21 |
| M05 | Apply server correction at rest | Partial, S4/S5 correction rule | N authoritative correction updates stopped player; R do not classify a falling player as stationary via wrong masks | T server world identity before correction; P fresh correction origin |
| M06 | Reconcile while moving | Unknown full behavior; S4/S5 only partial rule inspected | N bounded disagreement during input; R corrections/teleports do not oscillate or bypass validation | T teleport/loading cancels incompatible prediction; P reconstruct from server |
| M07 | Traverse route terrain, WMO and water | Partial source dependencies; no accepted route | N Willem→Garrick and back across required structures/tiles; R blocked/colliding path does not tunnel/fly around | T adjacent tile loads coherent; P correct location/reachability; issues 19–24 |
| M08 | Use transport/forced/flight motion | Missing/partial selected path: S5 returns for donor transport; broader modes not established | N explicit transport block/forced move acknowledgements; R unavailable transport/flight authority refused | T transfer/re-entry handles mover ownership; release family remains blocking beyond first route |
| M09 | Select/clear an observed target | Partial, S5 SetSelection and S3 target state | N server target/clear reflected; R unstreamed/stale GUID not admitted | T target clears with incompatible scope; P no retained stale target |
| M10 | Bound motion without losing actions | Broken, shared outbox R03; reply queue R04 | N movement can coalesce only replacement poses; R overload returns defined results rather than removing quest/cast/logout | T terminal barriers survive backlog; P no old intent replay; prerequisite for reliable loop |

## Combat and resources — issue 25

| ID | Behavior | Source assessment | N and R cases | T and P cases / dependencies |
|---|---|---|---|---|
| C01 | Start/stop baseline attack on selected enemy | Partial, S2/S5/S8 | N authoritative attack start, damage and stop; R dead/out-of-range/facing refusal matches UI | T old victim/actions invalidated; P attack does not resume accidentally |
| C02 | Cast Fireball 133 at the actual target | Partial, S2/S5; unit target builder | N learned eligible spell follows server start/go; R missing spell, dead/stale target and resource shortage show real refusal | T cancel old target/cast; P current spell rank must come from server |
| C03 | Preserve full received spell target blocks | Partial source from ccb25a5a, S2/S8 | N unit/item/object/source/destination and conditional payload decoded; R unsupported/malformed masks cannot shift later fields | T caster/target scope retained; P transient casts cleared |
| C04 | Explain cast failure by meaning | Partial, S7 maps Wrath reasons and arguments | N correct message/argument for known failure; R unknown reason logged/generic, never foreign numeric passthrough | T old failure cannot cancel new-scope cast; P no persistent stale error |
| C05 | Interrupt the right caster | Partial, S2/S8 caster-keyed SpellFailedOther | N self or nearby caster animation/cast clears; R remote interruption must not fail player's different cast | T old caster GUID/generation excluded; P pending cast cleared |
| C06 | Render server melee/non-melee outcomes | Partial source from 3755ed88, S2/S8 | N real damage/absorb/resist/block and combat log; R conditional hit flags and malformed sub-blocks handled | T stale damage cannot change new world; P health reconstructed, not replayed logs |
| C07 | Render periodic effects and resources | Partial source from 3755ed88; selected periodic forms | N damage/heal/energize/mana-leech reflect server values; R unknown aura log shape rejected | T ticks follow scope; P persistent effects require aura reconstruction, not log history |
| C08 | Reconstruct and update cooldowns/channel timers | Partial, 3ef23b0f adds deltas/self-channel events; initial spell cooldown payload still discarded | N initial plus deltas and channel end; R rejected cast does not invent/reset cooldowns | T cancel appropriate timers; P current server cooldowns survive re-entry accurately |
| C09 | Reconstruct and update aura state | Partial, 160d648e adds reducer/digest/projection; durations not presented and slots 48+ dropped | N add/update/remove stacks, duration, caster and dispel identity; R unsupported slots/flags refused | T scoped cleanup; P initial aura state restored from server |
| C10 | Observe combat completion and objective credit | Partial, S2/S3/S8 plus quest update | N actual Garrick death and server quest state; R no credit/damage synthesized from attack animation or loot UI | T stop old combat; P server quest state, health/power and item state retained |

## Quest and inventory — issue 26 and full economy gate

| ID | Behavior | Source assessment | N and R cases | T and P cases / dependencies |
|---|---|---|---|---|
| I01 | Open Willem gossip/quest menu | Partial, S2/S5/S8; gossip menu ID retained and delivered | N correct text/options/quest rows; R stale NPC/menu or code rejected explicitly | T menu invalidated by scope; P freshly queried menu |
| I02 | Query quest 6 with AUTO_ACCEPT behavior | Partial, S2/S5; static flags only | N actual server auto-accept path recorded; R already-completed/ineligible/full log unchanged | T quest state owner stays server; P real quest log rebuilt |
| I03 | Explicitly accept control quest 5261 | Partial builder/parser, S2/S5 | N wire divider and server acceptance; R duplicate/ineligible accept cannot duplicate state | T no queued old-scope acceptance; P authoritative log retained |
| I04 | Show objective progress and held item count | Partial, S3/S4/S6/S8 | N kill/item/log updates reach stock surfaces; R stale update cannot invent completed objective | T inventory/log projection coherent; P real counts after re-entry |
| I05 | Open Garrick loot response | Partial, S2/S7 | N item 182 slot/type/property payload shown; R no-loot/wrong GUID/server refusal visible | T close invalid corpse window; P corpse availability re-queried, not cached |
| I06 | Autostore the chosen loot slot | Partial, S2/S5/S6 | N removed slot plus actual item object/player slot update; R full bags/duplicate/stale slot does not create an item | T no stale loot command; P actual item retained once |
| I07 | Query complete item template | Partial; 430c4ce7 repairs R02 with two damage triplets. Stats count still clamps at 64; full malformed/count fidelity not established | N item 182/6076 fields, strings and use data align; R count/truncation rejected instead of clamped/misaligned | T template identity stable but scope references rebuilt; P correct tooltip after reconnect |
| I08 | Replicate item/container create/update/delete | Partial, S3/S6 | N GUID, stack, durability, enchant and owner fields map; R missing/deleted owner/item cannot resolve as another object | T preserve only appropriate received inventory, teardown scope safely; P reconstruct from server |
| I09 | Resolve bags and count quest material | Partial, S6 and existing Inventory/BagCache owners | N item 182 in backpack/bag reaches held count; R missing container/slot and split updates handled without duplicate count | T cache invalidated/rebuilt with owner identity; P exact counts after re-entry |
| I10 | Use/equip/swap/split items | Missing Wrath tap arms at anchor: S9 variants fall through S5 default | N existing UI commands reach correct 12340 builders/results; R locked/full/ineligible/duplicate operations visibly refused | T no old mutations replay; P final authoritative slots/counts retained |
| I11 | Access complete required inventory/bank attributes | Partial with deliberate loss, S6/R06 | N all 28 bank slots/seven bank bags and required enchant/suffix data addressable; R unsupported state remains a blocker | T owner/generation fidelity; P no hidden/truncated persistent state |
| I12 | Request completion/reward panel | Partial, S2/S5/S8 | N held head enables actual server completion; R missing head/ineligible choice does not fabricate panel success | T pending panel/cast scope invalidated; P quest state from server |
| I13 | Choose reward 0 and receive item 6076 | Partial with R02/R03 dependencies | N one authoritative pants object, slot and completed quest; R repeated/invalid reward request gives no duplicate reward | T admitted/result correlation; P retained reward/consumed head/completed quest |
| I14 | Abandon/fail/restore quest state | Partial/unknown; complete abandon/timer-failure/log-removal source chain was not established in this inventory | N abandon/timer failure and log removal; R invalid quest/stale slot refused | T quest owner not replaced by UI cache; P final log/credit remains authoritative |

## Death and recovery — issue 27

| ID | Behavior | Source assessment | N and R cases | T and P cases / dependencies |
|---|---|---|---|---|
| D01 | Enter authoritative dead state | Partial; e0be0d95 adds PLAYER_FLAGS ghost projection to existing death owner | N server death disables invalid actions and presents actual state; R zero/unknown health not conflated with a complete death transition | T corpse/ghost/world state coherent; P dead state restored |
| D02 | Release spirit | Partial, e0be0d95 adds one-byte Wrath RepopRequest, S9/S5 | N release reaches server and ghost state; R alive/duplicate release refused | T map/ghost transition acknowledged; P correct ghost/corpse state |
| D03 | Locate corpse | Partial, e0be0d95 adds CorpseQuery request/result, S9/S5/S2 | N server location/map identifies own corpse; R absent/wrong corpse does not invent a marker | T cross-map corpse representation; P correct corpse identity |
| D04 | Reclaim corpse with delay/range rules | Partial, e0be0d95 adds reclaim and delay path | N eligible reclaim restores server state; R premature/distant/wrong GUID refused | T stale corpse scope excluded; P no duplicate recovery |
| D05 | Accept/decline resurrection | Partial, e0be0d95 adds offer/answer/self-res/healer paths; SMSG_RESURRECT_FAILED remains silent | N actual request, answer and resource state; R stale/declined/invalid resurrector respected | T no obsolete request acceptance; P server life state restored |
| D06 | Refuse incompatible combat/item actions while dead/ghost | Partial local donor owners; complete Wrath refusal path Unknown | N allowed ghost actions; R attack/cast/loot/equip restrictions follow server and selected UI | T override/transfer clears pending actions; P no deferred invalid commands after revival |

## Logout, transfer and re-entry — issues 24/26/27 and operational gate

| ID | Behavior | Source assessment | N and R cases | T and P cases / dependencies |
|---|---|---|---|---|
| R01 | Request/cancel clean logout | Partial, S2/S5/S1 | N actual logout response/countdown/cancel; R combat/invalid logout refusal visible | T cancel old queued actions; P char select and next entry coherent |
| R02 | Retire old worker/publisher before admission | Partial source through df8c7527, S1 | N cancel/join/drain precede replacement; R timeout keeps tracked retirement, panic produces terminal diagnostic | T no overlap; P one worker/publisher ownership chain |
| R03 | Return to character select and re-enter same GUID | Partial parked source, S1 | N fresh roster/pick/login; R removed/stale GUID refused without killing link | T terminal events drained before new channel; P item/quest/state checked on intended GUID |
| R04 | Handle connection loss separately from logout | Partial explicit no-retry parked/lost policy | N visible disconnected UI; R no invisible redial/worker overlap or accidental first-character fallback | T teardown incompatible state; P future recovery must choose correct identity |
| R05 | Process map/world transfer and acknowledgements | Partial, S3 phases and S4 scope | N correct loading/worldport acknowledgement; R update tail/stale commands rejected | T same-map new instance still changes world generation; P current world reconstructed |
| R06 | Reject stale replies without accepting premature new scope | Partial tagging/ordering, S4; newer replies currently allowed | N matching reply delivered once; R older/forced/newer interleavings scrutinized | T no old damage/menu/item update or terminal into new world; P no cross-session replay |
| R07 | Survive bounded overload/shutdown | Partial lifecycle; Broken action overflow and unbounded replies, S1/S5/R03/R04 review | N prompt stop/cancel and coherent visible terminal; R backlog never silently discards critical actions or grows without bound | T terminal barrier survives queued traffic; P no lingering worker/intent |
| R08 | Retain quest reward and restore usable UI | Unknown connected persistence; depends on I04–I13 and E08 | N same character has quest completion, head consumed and pants retained; R interrupted/duplicate/failed turn-in preserves actual server state | T correct caches cleared/refilled; P spellbook/cooldowns/auras/action bars/options consistent |

## Existing check linkage and missing proof

Source-only linkage, not passing results:

- E02: `gameplay.rs::tests::{char_enum_count_then_build_enum_data_records,char_enum_parses_two_records_and_takes_the_count_along,char_enum_refuses_a_record_cut_short}`.
- E04/E05/R03: `wrath.rs` parked-command tests and `the_wrath_record_translates_onto_the_stock_roster_record`.
- C02/I03/I05: gameplay target/accept/loot parser tests plus `wrath.rs::command_tap_tests::stock_commands_become_the_335_messages`.
- R02/R05: `wrath.rs` cleanup regression `terminal_scope_loss_clears_ecs_and_input_without_a_digest`; reducer/session inline tests exercise component invariants, not full reconnect/persistence.
- Donor UI shape/surface gates validate their declared 1.12 reference. They are not Wrath host acceptance.

The earlier OC build/test reports apply only to their recorded revisions and exclusions. None of these source references proves current compilation, actual server packet acceptance, visual correctness or the pinned scenario. Most rows do not yet have a reviewed normal/refusal/transfer/persistence test chain. That absence must stay visible rather than count as WORKING.

## Dependency-ordered implementation children

These are refinements under existing issues, not duplicate tickets or new permission grants:

1. Issues 23/25/26: repair R01 movement flags and R03 critical-command admission; verify the source repair for R02 item alignment in 430c4ce7 under an applicable check grant. M03/M04/M10/R07 remain concrete source blockers; I07 acceptance is unverified.
2. Issues 24/26: establish intended-character boot/re-entry selection and bounded terminal/scope ordering (E03/R02–R06). Do not prove persistence on a different enumerated character.
3. Issue 25: build on 3ef23b0f/160d648e; finish initial cooldown reconstruction and aura duration/slot/flag fidelity, then rejected/interrupt cases and outcome/UI mapping (C02–C10). An initial aura/descriptor projection is not full Wrath coverage.
4. Issue 26: complete actual item/container count and reward ownership; add required item mutation intents and full bank/attribute fidelity as separate cohesive children (I06–I13).
5. Issue 27: validate the e0be0d95 release/corpse/reclaim/resurrection path, complete explicit refusal/edge cases and then connected recovery (D01–D06).
6. Issues 19–24: finish profile/model/WMO/liquid/collision route dependencies in parallel source work; do not replace the connected route with a camera demo.
7. Issue 18: selected-mode UI/addon host behavior and the unenumerated broad baseline families remain release blockers. The first-loop inventory does not close them.

No new behavior is accepted by this inventory. Source changes use exact ownership claims and cohesive commits. Build/check/graphical/lab/WM evidence remains separately gated. Full stock-client replacement remains mandatory; use no completion percentage derived from these 56 rows.

## Reconciliation and provenance

Astra UI reconciliation through `dd316dd3`: production glue/game creation selects an explicit Wrath target, addon dependency gates use interface 30300, and the host selects the Lua 5.1 dialect while retaining vanilla defaults. Source now connects addon chunk name/private namespaces, explicit handler arguments plus legacy globals, named XML handlers, post-hooks, frame attributes (including XML), shared sizing, selected tooltip/spell/resource APIs and targeted addon whispers. E08's original fixed-version contradiction is superseded by these inspected paths. Acceptance remains UNKNOWN: coroutine support, complete collector/client API fidelity, secure execution, native cast/aura UI consumption, atomic cross-host asset reload, full installed-addon loading and runtime/rendered behavior are not established. This updates one source-support row, not the 56-row denominator or any release percentage. OC's current uncommitted native contract is not accepted by this update.

Loader continuation through `b6d427e8`: explicit Wrath template lists now flow through runtime admission, recursive expansion and inherited layout levels (`16b37928`), inline anchor offsets survive deferred resolution (`1bc1f73c`), and parentKey publishes existing child wrappers before frame OnLoad (`b6d427e8`). Consumers were traced from installed Bartender4 and Questie/GatherMate XML plus the target ActionBarFrame.xml/UI.xsd. Secure action authority and animation/specialized-builder completeness are not established by these source repairs. No execution evidence or acceptance status changed.

Native UI continuation: `8ee9a66f` consumes `f8a1993b`'s seven-resource observed-state accessor and active type; `4b08d118` adds Wrath-only cast listener admission consistently across registration, unregistration and all-events registration. These do not deliver native cast getters/events or establish connected resource behavior. The Astra UI-host checkpoint records the remaining cast clock/identity contract and the lossy mutually exclusive target-context projection found in the new native source. Compilation, event delivery and all behavior acceptance remain UNKNOWN.

Source-inspected follow-ups: 3ef23b0f adds cooldown list/event/clear/item deltas and player-gated channel events; 160d648e adds ordered aura state and a shortened descriptor projection; 430c4ce7 reads the correct two item damage records; e0be0d95 adds the death command/result path and ghost flag projection. Initial spellbook translation still supplies empty cooldowns; aura durations and upper slots remain gaps. Movement flags and shared action/movement eviction remain as described in the original source review. No build, test or live proof was added by this reconciliation.

The initial inventory/audit/handoff documents were swept from the shared Git index into OC's e0be0d95 commit while Astra authored them. They are Astra-owned content despite that commit title. Follow-up 07e1aefe refines the handoff schema names; this reconciliation is a later explicit-path document commit. History is preserved and no OC source is reset or amended. Do not infer the author of each staged document from the mixed commit title.

2026-10-27 Astra source continuation supersedes the earlier absence of coroutine machinery and cast getters/events: `7897e642`, `6a9965c1`, `1368a420` and `f4552cf0` add rooted real thread values, suspended contexts/upvalues/GC and the explicit Wrath coroutine library. `1fbd4010` connects UnitCastingInfo/UnitChannelInfo to the existing scoped native mirror; `9cf1594b` feeds its native cast/channel lifecycle into the hosted Wrath event path using the same attempt IDs. This establishes source paths only. These commits have no compiler/test/runtime evidence. Coroutine edge behavior, scheduler/host admission, reload/transfer/teardown, exact cast timeline fidelity, unknown interruptibility, complete unit-token/event coverage, aura UI, secure execution and installed-addon behavior remain open. The detailed cases and limitations are in the existing Astra UI-host checkpoint. No behavior row is promoted to WORKING, no full-replacement gate closes and no percentage is derived from these commits.

Native aura/resource continuation: `703d238b` reads full native slots into Wrath aura tuples and filtered queries; `d11013d9` reuses the existing aura tooltip builder/cache with native identity and caster level; `b7ec8f41` reuses the established reverse unit-token resolver; `ce6ac75b` connects native full-slot change notifications; `1783b6e0` admits runic-power listeners and emits scoped power/type notifications from the existing native seven-pair read model. These are Wrath adaptations of the inherited vanilla client, not implementations of previously absent whole client subsystems. Aura expiration/identical refresh anchors, RAID/stealability/consolidation policy, out-of-range group raw resources, individual rune state/cooldowns, focus/vehicle token support and connected UI/mod behavior remain incomplete. Native expirations are unavailable rather than renewed from query time. These commits remain uncompiled and untested; no behavior acceptance is promoted.

Native aura availability review at db63ec50: both producers currently derive Known/empty from an existing object entry whose aura vector may only have been default-initialized. The reducer does not record receipt of an empty full snapshot, distinguish full replacement from delta merge, or retain refresh clocks/revisions. The checked AzerothCore self-init path sends create followed by a forced aura snapshot; remote empty snapshots can be omitted. Exact source counterexamples and the native-owner repair contract are in the UI-host checkpoint. Therefore unknown-versus-observed-empty, complete full-snapshot reconciliation and aura timing are not accepted from schema/helper round-trip evidence. No live failure is claimed.

Verification supersession at e3c83d90: OC's adc8a631 now propagates per-generation aura availability and full-snapshot receipt edges through both producer paths; it gates the UI query until observation. Astra's newly authorized one-time pass confirms the current app compiles with warnings. Completed Lua/mlua suites passed; the UI suite has 3321 pass / 3 fail / 1 ignored, matching the prior census-path/timezone assertions. It is not a green full suite; doctests/later targets were not reached. Exact logs are retained in the 2026-10-27 verification folder. Initial cooldowns, identical delta refresh timing, expiration/replacement policy, actual Wrath host test admission, native/live/UI integration and rendered acceptance remain open. No behavior row or full-replacement gate is promoted from these checks.

Later source-only UI continuation: c1de419f adds aura-availability notification when slot payloads stay empty. 679c92ec adds Wrath-only strsplit/strtrim/strjoin/strconcat through the existing VM/string owners; GetCVarBool now reads the existing applied CVar mirror in explicit Wrath mode. The string/CVar compatibility contracts use target FrameXML consumers plus explicitly identified independent Elune/Wowless behavioral references, not exact target-binary proof. These later changes have no compiler/test/runtime evidence. The receipt-bound aura expiration/event/tooltip integration remains in three claimed uncommitted Astra UI files, pending OC's native receipt dependency. Wrath spellbook rank-collapsed tab counts and slot translation, passive/selection API names and broader CVar defaults remain open; no installed-mod, connected UI or replacement acceptance is promoted.

Verification/source supersession: the new explicit one-time pass at 29ff13f9 checked benilla-lua/mlua/ui successfully and ran existing suites once. Completed Lua/mlua suites passed; UI 3322 passed / 2 failed / 1 ignored. The census caller-path assertion now passes; the same two timezone assertions remain. Total 237.7s, no timeout or reruns; doctests/later targets were not reached. This compiles the committed string/CVar/allocation repairs without proving their complete target behavior. OC's 6feb5d4d supplies the native receipt dependency; Astra's c002a798 now commits the three previously pending aura UI consumers. That later integration is excluded from this verification pin and remains unverified by Astra. See .scratch/project-b/verification/2026-10-27-astra-ui2/summary.md and the UI-host checkpoint. No connected, rendered or full-replacement acceptance changed.

O5 follow-up at `920273d5`: route checkpoint adds static tile/model/icon/visual pins, but changes no parser/application source. The alleged WMO +8 header shift mixes whole-chunk and payload offsets; the current Rust reads agree with the cited extractor when both strip the eight-byte prefix. See [Astra's O5 counter-evidence](../checkpoints/2026-10-26-astra-o5-source-review.md). Do not shift the header or treat it as an unresolved version decision on this evidence. E07/M07 remain source-partial with exact WMO/doodad/kit-texture pins and rendered acceptance open; M03/M04/M10/R07 retain their observed movement/admission blockers. O5 preparation does not accept any behavior row.
