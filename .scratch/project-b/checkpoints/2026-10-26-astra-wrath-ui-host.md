# ACTIVE Astra claim — Wrath scripting and installed-addon compatibility

Owner priority correction: faithful current AzerothCore/client/mod replacement first, optional ER combat/UI next; procgen stopped. This is implementation inside the six-hour campaign, not a new short goal. Original checkpoint remains 15:00:47 Europe/Moscow.

Current source admission: 8998d3d1 plus OC-owned session/client/publisher edits. Preserve them and the old patch helper. Exact first-slice files: `crates/benilla-lua/src/{lib.rs,dialect.rs,lex.rs,number.rs}` and this checkpoint plus the six-hour package priority correction. Next dependent slices may claim parser/compiler/VM/prototype files and benilla-mlua/UI host/lifecycle files after source inspection, before editing. No test files/cases or checks are authorized.

Observed blockers: the interpreter is a deliberate Lua 5.0-compatible donor, not a ready Wrath Lua 5.1 host. Its parser rejects expression varargs, length and modulo; lexing rejects levelled long brackets and hexadecimal numerals. Libraries and vararg/generic-for VM semantics also differ. Merely advertising interface 30300 would be false completion. First implement an explicit language boundary while retaining donor defaults, then complete code generation/runtime/library paths before selecting it in production UI construction.

Primary reference for language behavior: official Lua 5.1 source, especially llex.c, lparser.c, lobject.c and lvm.c. This is a language-reference baseline, not proof of every WoW-specific 12340 patch. Actual installed-mod behaviors and client-specific differences remain independently source-grounded requirements. No interpreter/client/game execution has occurred.

First source milestone: explicit Dialect selection and a Lua-5.1 lexical path for levelled long strings/comments, CR/LF pairing and string line handling, full numeral-token rejection, hexadecimal conversion through the existing target-width converter and 5.1 quote style. Donor constructors still choose Vanilla50. The parser/VM/UI do not select the new path yet: expression varargs, operators, VM semantics and 5.1 libraries must be completed before production activation. No claim of functional Wrath addon hosting is made.

Next active exact claim adds `crates/benilla-lua/src/{compile.rs,parse.rs,proto.rs,vm.rs,reload.rs}` for dialect propagation, length/modulo code generation/dispatch and reload consistency. Existing constructor defaults and donor opcode behavior must remain intact. No UI activation is part of this intermediate operator milestone.

References inspected read-only: https://www.lua.org/source/5.1/llex.c.html and https://www.lua.org/source/5.1/lobject.c.html . The level-zero nested-long-string diagnostic follows the default 5.1 compatibility setting; the exact target client's build-specific setting remains an explicit verification item. No tests/new cases/builds/linters/runtime checks ran. This commit is a continuing campaign milestone, not its conclusion.

Second source milestone: explicit-dialect compile and bare-VM construction, length/modulo parsing and opcodes, verifier/relocation/debug register tracking, numeric coercion and dialect-preserving reload. Modulo follows the floor-based numeric rule; length uses raw table/string length and the ordinary metamethod path for other types, with nil as the second argument. Wrath power uses numeric exponentiation instead of the donor replaceable global. Donor default construction is retained. References: https://www.lua.org/source/5.1/lvm.c.html and https://www.lua.org/source/5.1/luaconf.h.html .

The UI remains on its old constructor until the remaining language/library/host work is ready. Vararg expressions and their calling convention, generic/numeric loops, library differences, constructor rules and installed-addon APIs remain active implementation dependencies. No compiler, interpreter, fixture or runtime acceptance is claimed for these commits.

Next source milestone implements expression varargs and fixed/all-result propagation through assignment, calls, returns and constructors. Lua-5.1 vararg calls preserve extras below the register base on the existing rooted stack. A recorded original function slot is used for returns/tail replacement; the Rust Frame grows from 64 to 72 bytes, with no claimed performance evidence. The implicit compatibility arg table is retained only when the function needs it; using `...` clears that need and leaves the compatibility local nil. Donor calls retain their old base/table behavior. The donor-specific tail-copy optimization is excluded for the shifted 5.1 convention. Primary references are the official lparser.c vararg flags and lvm.c OP_VARARG. Source inspection specifically checked the single-result return shortcut, all frame construction sites, shifted argument storage, nil padding and verifier/codegen paths. Execution remains unverified and UI activation is still pending.

Loop/constructor milestone: separate 5.1 numeric controls and user variable; generic iterator/state/control plus distinct output locals; no implicit table-to-next conversion; per-iteration closure scope; typed preparation/loop/iterator opcodes and their verifier/jump/debug paths. Constructor grammar/hints select the appropriate dialect, including 50-item list batches, 5.1 floating-byte hints, computed-key hash hints, trailing-multret hint treatment and stricter semicolons. Parameter/upvalue limits are dialect-aware. The existing donor paths remain selected by default.

Installed-source evidence for priority: WMBridge and WM_Multiclass declare Interface 30300. WM_Multiclass.lua uses `#hex` and modulo in packet validation; its resource UI also uses length. Installed Bagnon TOC identifies 2.13.3; Bartender4 identifies 4.4.2-2-g3b02ee4. These are filesystem/source facts, not addon load or behavior acceptance. No installed source/assets were copied into the repository. Full installed-addon inventory/API integration is still work in this campaign.

Active next claim adds `crates/benilla-lua/src/{heap.rs,lib_base.rs,lib_table.rs,lib_string.rs,lib_math.rs}` and `crates/benilla-mlua/src/lib.rs` for the real 5.1 library/string-method boundary, plus the existing claimed constructor/VM files. Coroutine continuations and collector differences must not be hidden behind successful-looking shims. UI activation and acceptance remain separate downstream work.

Library milestone: explicit dialect-aware state and mlua-adapter constructors; GC-rooted string metatable and working string-method index; select; ranged/nil-preserving unpack with the 5.1 C-stack bound; raw-length table operations/maxn and obsolete setn refusal; all-argument assert and one-result gcinfo; string match/gmatch/reverse, out-of-range find behavior and 5.1 substitution/table replacement semantics. Typed method installation no longer silently ignores the supported string metatable. Donor constructors still choose Vanilla50 and its original library path. These source changes do not implement coroutine continuations, all collector modes, complete string/math differences, or WoW-specific 3.3.5 APIs. Those remain active dependencies before UI activation/compatibility acceptance.

Source review inspected result-stack conventions, string-metatable collection roots, type-family validation in the mlua adapter, raw table lengths versus remembered n, integer range bounds and original-text retention for nil/false substitution outcomes. References also include the official lbaselib.c and ltablib.c. No build, lint, test case or interpreter run occurred. The six-hour campaign remains active.

Collector control milestone adds actual automatic-stop/restart state, explicit full collection, fractional-KB count and next-cycle pause pacing for the 5.1 path. Incremental step/step-multiplier and negative-pause behavior are explicitly refused and remain compatibility gaps; no dummy successful controls are installed. Donor threshold/count behavior remains on its existing branch. Source inspection of stack shrinking and GC roots found preserved varargs remain below the active base/top and are not cleared by the size check. This does not prove GC behavior through execution. Installed GarbageProtector declares Interface 30300 and intercepts collector calls; its complete behavior remains pending.

Active claim extends to UI script mod.rs, target.rs, model.rs, addon_gate.rs, addon.rs, glue_addons.rs, client.rs, glue_core.rs and stdlib.rs, then app ui_script lifecycle/addons/reference_ui after each dependency is inspected. Explicit UiTarget separates client contract from glue/game VmKind. Default constructors remain vanilla; the explicit Wrath constructor selects the new engine dialect and retains its string metatable. The addon gate accepts the owning interface and propagates it through every dependency, query and demand-load path. No production call site selects Wrath yet; build identity/library aliases and source-profile selection remain downstream work. Installed Wow.exe contains the unique build-date string Jun 24 2010, read as static bytes only. No launch occurred.

The loader claim also includes crates/benilla-ui/src/loader/mod.rs. Installed GarbageProtector's first line consumes addon name/private namespace through top-level varargs, exposing a second blocker beyond parser support. Wrath loaded chunks now receive those arguments while inside the existing AddOn_Load owner. A scoped file-load identity is separate from taint/security and restores across nested loads. Private tables live in the Lua registry for the VM session; startup/demand Lua files and XML included/inline chunks use the same path. Saved-variable files still execute without arguments. Vanilla keeps zero arguments. App refusal diagnostics read the selected interface. Source-only; no addon load, VM execution or tests occurred.

Next library slice adds actual 5.1 ranged string.byte (negative endpoints, empty intervals, C-stack result budget), fmod/modf, hyperbolics and math.huge on explicit Wrath states. Donor single-byte semantics and math table construction remain unchanged. modf preserves signed zero fractions for integral/infinite arguments. References: https://www.lua.org/source/5.1/lmathlib.c.html and https://www.lua.org/source/5.1/lstrlib.c.html . This is not full WoW-specific library parity; coroutine, incremental GC, bytecode dumping and remaining client-specific string/numeric details remain open. No execution checks ran.

Explicit-target identity now selects 3.3.5/12340/Jun 24 2010 for Wrath. The in-game API adds numeric interface 30300 as its fourth result, required directly by installed QuestieCompat, GatherMate and Ace libraries; vanilla keeps its three results. Glue retains its separate five-field shape, consistent with the login source consumer (version/build labels then identity), rather than applying the in-game signature there. This is consumer/static-binary evidence, not reference-client execution. Wrath-only stdlib aliases strmatch/strrev/gmatch and real wipe/table.wipe are installed; wipe clears entries in place and retains the table/metatable. No missing secure hook, print, coroutine or full later-client API is faked. Production construction still defaults to vanilla pending the source-profile/remaining-host work.

Production-source milestone: startup glue, boot strings and new/reloaded in-game VMs select UiTarget through the existing WOW_WRATH switch; explicit vanilla constructors remain the default for ordinary callers. The UI chain now uses the app's WM_ASSET_MANIFEST/WM_ASSET_PROFILE selection through the existing open_profile_chain API, including manifest-open refusal without donor fallback. This repairs the inspected split where render assets honored the selected profile but UI did not. The chain remains a separate process-lifetime snapshot, as before: atomic cross-host profile reload and locale fidelity are still open and no shared-reader/reload completion is claimed. New Wrath VMs log incomplete compatibility. This wiring makes the source reachable; it does not assert readiness, full Lua51/secure UI parity, successful FrameXML/mod loading, compilation or rendered acceptance. Remaining secure-hook/API/coroutine gaps continue in this campaign; no execution grant was inferred.

Iterator follow-up: source inspection found base pairs/ipairs still used donor global-generator lookup/direct-step semantics even on Wrath states. Wrath opening now installs closed-over stable generators, ignores extra arguments to the public ipairs factory, and uses raw indexed iteration through its own auxiliary. No generator global is added. Native closure upvalues keep the auxiliaries in the existing GC graph. The donor functions remain unchanged. Reference: official lbaselib.c pairs/ipairs implementations. No execution occurred.

Active exact claim adds script/posthook.rs. Installed WMBridge registers PickupContainerItem/PickupInventoryItem post-hooks; multiclass spellmaking registers tooltip/action post-hooks. Wrath-only hooksecurefunc now supports global and table-method forms, original argument/return preservation (including zero results and trailing nils), chained registration, original-error propagation without hook execution, and isolated hook errors retained through the existing diagnostic/error-dispatch owners. Captured functions/chains are Lua closures in the registry/GC graph rather than permanent Rust-held roots. The post-hook invocation gets the registration context without restamping the original function. This is a source implementation over the existing stack-based taint approximation: value taint, secure frames/combat lockdown and complete Wrath secure execution remain open; securecall/securecallfunction are not supplied as false authority bypasses. Hook ordering/error behavior and addon use remain unexecuted/unaccepted.

Claim extends to script/event.rs, script/tick.rs and loader/scripts.rs. Inspection disproved the old module summary's suggestion that handlers already receive explicit arguments: invoke_with_globals actually called every handler with zero arguments. Wrath now calls handlers with self plus their payload, with an explicit event name for OnEvent retained through held calls; vanilla retains its globals-only convention. Wrath XML handler bodies compile with named parameters and varargs; function= attributes resolve named/dotted functions without source evaluation, and conflicting body+function forms refuse visibly. Installed Pawn and Questie XML demonstrate function= consumers; installed WM callbacks demonstrate self/event/payload consumers. Remaining unsupported script kinds and event production are separate gaps. No event/FrameXML execution or test was run.

Claim adds script/object/events_regions.rs for Wrath HookScript over the existing handler dispatcher. Registered post-hooks are Lua-registry state, snapshot in registration order; the original remains GetScript's value, post-hooks remain after primary removal, and an initially empty handler uses SetScript semantics. Handler membership/tick/input presence includes surviving hooks. XML OnLoad and ordinary widget/event/update dispatch run hooks through the same convention; hook failures use existing error owners and do not discard the primary outcome. Primary-handler replacement and hook retention are the targeted 3.3.5 behavior, with target-client reference/runtime confirmation still required. This does not add later-client script kinds or secure-frame authority. Source-only, uncompiled and unexecuted.

Exact claim adds script/region_map.rs, script/object.rs, script/object/layout_methods.rs and script/region/layout.rs. Installed multiclass panels call SetSize during file load, before any network exchange. Wrath SetSize/GetSize now use the existing shared Region-map dispatch across Frame/Texture/FontString/title receivers. Both dimensions update in one layout operation, retaining frame drag-authored dimensions and measurement/invalidation ownership. Receiver guards recognize the target-specific shared functions; vanilla keeps its original closed method set. No visual/layout/runtime check occurred.

Exact claim adds script/tooltip/mod.rs, script/tooltip_item/{mod.rs,build.rs} and script/tooltip_spell.rs. Wrath tooltip content builders now produce OnTooltipSetItem/OnTooltipSetSpell, and those script kinds are accepted only on Wrath. Hunt's GetItem consumer receives the kept item name and exact source link for bag/inventory/hyperlink sources; no random-property/enchant link is reconstructed. Sources without a retained link return unavailable link data and remain a coverage gap. Spellmaking's SetSpellByID and book-slot alias route through the existing tooltip cache/builder, rather than a second spell source. Cache misses still use the existing asks/waits. Full tooltip/client API fidelity and runtime delivery remain unverified; no tests were added/run.

OC source review at 178b2582: 458466ec/c4c9e52e exist, but read_server_time in gameplay.rs consumes only one u32 despite the report claiming the next-reset word was consumed. The local QueryPackets.cpp TimeQueryResponse writer writes ServerTime then TimeResponse. SMSG_AREA_TRIGGER_MESSAGE is parsed as a C string directly, but local WorldSession.cpp SendAreaTriggerMessage writes a u32 length first, then the string. OC-owned code remains untouched; these two findings require owner repair. Packet framing is separate, so no stream-desynchronization reproduction is claimed. No OC verification script/AST parser/check was run or adopted as Astra execution evidence.

Exact claim adds script/wrath_spell.rs, script/host.rs and one read-only query method in app/src/script_calls.rs (not OC's protocol/session files). GetSpellInfo's nine-value Wrath shape is directly consumed by installed WM_SpellmakingV3, which edits values 7–9; custom panels also need its name/icon and GetSpellLink. These queries now use the lent host's existing SpellCatalog, cost/modifier/cast-time owners and authored range rows synchronously. No second catalog, query runner or retained gameplay snapshot is added. Book-slot and known-name resolution stay with the existing spellbook owner; numeric ids can query unlearned catalog rows. An unstreamed player leaves dynamic cost unavailable; absent host/catalog/id returns nil. This source slice does not claim effective target-dependent range parity, full name/link input parsing, known-spell state, complete Wrath resource projection, secure action attributes or successful addon execution. Those remain dependencies. No builds/tests/runtime execution occurred.

The next read-only query slice claims app/ui_script/wrath_queries.rs and its module registration. Metadata now has its own member in the existing disjoint call-host set, reading ObjectStore directly without a transient field-window clone. IsSpellKnown queries PlayerActions' full server-fed known set or PetBar's raw spell list, including entries filtered out of visible books. It does not infer learning from DBC existence. Missing owner data stays unavailable. This reuses the established query/mutation borrow discipline; no new resources, network path or persistent spell cache are introduced. Cast UNIT_SPELLCAST_* event payloads, UnitCastingInfo timing/cast-id provenance and secure action attributes still need richer owner contracts; donor CastBarEdge currently discards spell identity on terminal edges, so renaming old events would not be lossless. All compilation/runtime evidence remains UNKNOWN.

Claim adds script/object/attributes.rs and script/button.rs. Wrath frames now have registry-backed raw GetAttribute/SetAttribute and actual OnAttributeChanged notification after a changed value commits, with no model borrow across callbacks. Attributes can retain Lua values through the normal GC graph. Wrath clicks dispatch PreClick/OnClick/PostClick with self/button/down through the existing click owner and hardware-input gate; XML parameters and supported script kinds match. Vanilla retains its original methods and click payload. This is ordinary attribute storage plus callback order, not secure snippets, protected-template admission, combat lockdown enforcement or an attribute-to-cast implementation. The installed spellmaking SecureActionButtonTemplate path is still incomplete and must not be reported as playable. No GUI/game/client execution or tests ran.

Exact claim adds script/addon_message.rs, script/calls.rs and app/ui_chat/{input/send.rs,feed.rs,net.rs}. Installed WMBridge attempts CHANNEL then self-WHISPER; the old Lua binding rejected both and ignored target argument 4, despite OC's wire lane being source-wired. Wrath now has a typed addon-whisper call that retains recipient and unmodified prefix-TAB-payload through existing ClientCommand::Chat/LANG_ADDON. Inbound translated whispers report WHISPER instead of UNKNOWN. The local AzerothCore ChatHandler whitelist admits addon WHISPER but excludes CHANNEL and rejects text past 255 bytes; this binding refuses those unsupported/oversized structured sends instead of truncating or pretending channel support, allowing WMBridge's pcall fallback. Vanilla's binding/packet contract stays unchanged. Actual client-native boundary details beyond these source-backed constraints, WM delivery and receiver acceptance remain unverified. No socket/client/server/addon execution occurred.

Claim adds script/wrath_unit.rs and extends the existing read-query projection. Wrath UnitPower/UnitPowerMax read all seven player resource slots from current InWorld WrathWorldView raw player windows; raw getters retain protocol units, display getters scale rage/runic power by ten and happiness by a thousand. UnitPowerType also supplies the Wrath resource token. Transfers/terminal phases refuse the read without falling back to stale UI amounts. Other units can use the existing raw ObjectStore resource fields where supplied; their sixth/seventh resources still lack a lossless replicated window. No raw amount is reconstructed from a rounded value. Unknown/unavailable getters use the API's zero fallback; this is explicitly not evidence of an observed zero or complete alternate-unit resource support. Broader seven-resource replication/observedness still needs OC's lossless unit contract. No execution checks occurred.

Handler compatibility correction: installed Skillet XML still reads `this`, so dropping legacy globals from the Wrath handler branch was too strict. The engine/adapter now offer the existing rooted global save/restore operation with explicit arguments. Wrath handlers receive self/event/payload while this/event/argN compatibility is retained; legacy arg1 remains payload arg1, not the event name. Native and XML handlers/post-hooks share the convention. Argument handles stay rooted through raised-value conversion and the original donor no-argument entry stays available. No execution proof is claimed for nesting, error restoration or GC behavior.

Active XML attribute claim: loader/geometry.rs and loader/mod.rs, within Astra's existing UI ownership. Installed Skillet UI/RecipeNotes.xml declares recipe_link as an untyped XML Attribute, while its Lua reads it through GetAttribute. The merged element retains template children before instance children. Implement the schema's string/default, number, boolean and nil forms through existing SetAttribute before scripts/OnLoad; malformed declarations report faults. Schema reference: https://github.com/wowgaming/3.3.5-interface-files/blob/main/UI.xsd . Load ordering against the reference client, secure execution and addon behavior remain unverified.

Active inheritance claim: framexml.rs, loader/mod.rs, loader/regions.rs and script/object.rs. Installed Bartender4 actionbutton.lua requests ActionBarButtonTemplate. The target FrameXML source at https://raw.githubusercontent.com/wowgaming/3.3.5-interface-files/main/ActionBarFrame.xml declares that template with two comma-separated parents. Existing expansion, per-level layout traversal and CreateFrame admission all use one literal name. Add explicit Wrath list semantics across those owners, preserving the donor entry points and single-name behavior. Secure template execution, frame protection and action authority remain separate incomplete systems; template composition alone cannot establish them.

Inheritance implementation also claims target.rs for the immutable target-to-inheritance policy. Wrath expansion and per-level layout now traverse parents in list order with each parent's ancestry first. Case-folded path-local cycle tracking refuses cycles while retaining shared ancestors on separate branches. Runtime CreateFrame validates each listed parent before publishing a frame, and FontString font/template dispatch uses the same names. Vanilla's public document entry points and original single-chain walk remain unchanged. No build, new test, interpreter, addon load or graphical check ran; ordering and protection behavior remain runtime-unaccepted.

Anchor continuation claims loader/geometry.rs and loader/mod.rs. Installed Questie-335/Modules/WorldMapButton/QuestieWorldMapButtonTemplate.xml uses inline Anchor x/y; the Wrath UI.xsd declares those fields. They were ignored because DeferredAnchor retained only the Offset child. Retain inline script-unit coordinates as the fallback through the existing deferred resolution and XML layout owner; explicit Offset keeps the existing path. Donor inline-coordinate behavior is unchanged. Mixed inline/child precedence and rendered geometry remain unverified. Compact Size x/y already works in xml_length and was not changed.

Parent-key continuation claims loader/mod.rs and loader/regions.rs. Questie's same template declares Background and Icon region parentKey fields; GatherMate2 declares texture. The Wrath schema defines parentKey on LayoutFrameType. Add raw parent-wrapper publication through the existing GetParent owner before frame OnLoad/child use, using the final inherited declaration for shared region builders. This covers frames, layered regions and texture children using build_region; specialized region builders, animations and full target layout fidelity still require review. No execution evidence is added.

Native resource consumer claim: app/ui_script/wrath_queries.rs, app/script_calls.rs and UI script/host.rs + wrath_unit.rs. OC's in-flight WrathWorldView::resources(guid) now exposes independently observed seven-pair arrays and power_type. Replace the older padded player window/five-resource fallback with this existing owner and query the observed active type synchronously. This adds a consumer dependency on OC's currently uncommitted resource contract, not acceptance of its producer/lifecycle. Do not commit or edit OC's files. Cast UI timing remains blocked on a clock-domain contract: observed_at_unix_ms cannot be treated directly as GetTime milliseconds, nor can START's remaining time establish the full original duration by itself.

Cast event-admission claim: script/target.rs and script/object/events_regions.rs. The in-game event registry currently resolves only the donor table, silently dropping target cast registrations. Target CastingBarFrame.lua registers the ten cast/channel/interruptibility event names; installed multiclass spellmaking registers UNIT_SPELLCAST_START. Admit those names only in Wrath in-game VMs through one target-aware iterator/key lookup used by RegisterEvent, UnregisterEvent and RegisterAllEvents. This is listener admission, not fabricated event production, UnitCastingInfo, or native contract acceptance. Source reference: https://raw.githubusercontent.com/wowgaming/3.3.5-interface-files/main/CastingBarFrame.lua .

The native owner landed as `f8a1993b` during this continuation. `8ee9a66f` consumes its resource accessor, retaining independent observation availability through the host; the Lua zero fallback still does not prove observed zero. `4b08d118` admits target cast listeners; no cast event delivery/getter is claimed.

Critical review of `f8a1993b` keeps cast UI integration incomplete. First, `wrath_native.rs::target_context` chooses one field through an else-if chain: a cast with both unit and destination or source and destination loses the remaining coordinates/transport context in its active snapshot. The parsed SpellTargets is richer; the native snapshot needs that complete targeting record rather than a mutually exclusive summary if it is to serve as the lossless boundary. Second, producer Unix arrival time and GO server game time are explicitly different from GetTime's session clock. CastingBarFrame compares the getter's start/end times with GetTime and matches terminal payload argument 4 against castID. Correct integration therefore needs a defined monotonic arrival/timeline mapping, stable per-attempt UI identity and a defensible duration/interruptibility source. Wire cast_count alone must not silently become an asserted client-global line ID. These are source findings and required contract decisions, not reproduced runtime failures. OC's files remain unchanged by this review.

Specialized parent-key continuation claims loader/widgets.rs and loader/geometry.rs. Button label FontStrings, title regions and an EditBox's existing text region bypass build_region; publish their declared parent keys through the same wrapper owner before their remaining XML load. Font-only state records are not region wrappers and are unchanged. Coroutine inspection also confirms a substantial separate compatibility gap: Questie/Routes/GatherMate2/HandyNotes call create/resume/yield, but the engine has one execution stack and open-upvalue indices and the mlua Thread is a placeholder. Correct support requires resumable contexts, cross-context upvalues/GC roots and native-call yield boundaries; no successful-looking coroutine shim was added.

Event admission adds UNIT_SPELLCAST_SUCCEEDED to the same Wrath-only family after reading installed combatlogfix.lua, whose named handler consumes unit/name/rank/castID and then GetSpellInfo(name). This also exposes the remaining limitation of name-based spell metadata before spellbook availability; no arbitrary catalog rank/name winner was invented. Native success delivery and complete name-resolution fidelity remain open.

Environment repair claims benilla-lua/src/lib_base.rs. Official Lua 5.1 lbaselib.c confirms that getfenv returns the actual environment (without the donor's raw __fenv substitution), setfenv returns the changed function except for the numeric-zero thread case, and only getfenv defaults a missing/nil level. The current source retains the donor rules for every target. Correct the explicit 5.1 branch using the existing function-env generation/invalidation owner; preserve donor behavior. This is language environment semantics, not implementation of WoW value taint, secure frames or coroutine contexts. Source: https://www.lua.org/source/5.1/lbaselib.c.html . No execution grant has arrived; checks remain unperformed.

Saved-state repair claims script/saved.rs. saved_variables_bytes_for holds a Model app-data read borrow while globals.get can call __index, whose addon Lua can request a mutable Model borrow. Move that borrow inside the non-callback serialization phase. Numeric table keys currently accept integral Value::Number but cast them to i64; distinct finite keys beyond that range collapse into saturated keys. Preserve their numeric literals in Wrath while keeping donor key conversion; previously unsupported fractional/boolean/object key forms remain unsupported. No saved-variable files or account data are read/written by this source change; serialization/reload execution remains unverified.

Source review of OC clock follow-up `22b8690a`: adding an app-clock value is useful but is not yet a stable cast timeline. wrath_net_drain assigns cast.ui_now_seconds for every resolved native edge, including Delayed, while start_remaining_ms stays the START value. Reapplying the documented start-time formula after delay therefore shifts the origin by elapsed time as well as adding pushback. Preserve the initial START clock separately from update clocks (with unavailable clock distinguished from valid zero), and give channels equivalent stored START/update anchors if snapshots must survive UI reload. This review is pinned to the committed follow-up; OC's subsequent dirty wrath_native.rs is preserved and not treated as repaired/accepted. No runtime failure is claimed.

Installed-addon discovery claim: app/ui_script/addons.rs and reference_ui.rs. Current production discovery sees only local_state/AddOns plus twelve hard-coded vanilla archive addons; installed Wrath Interface/AddOns mods inspected in this campaign are otherwise absent. Wrath should read the selected data install's sibling Interface/AddOns directory, keep capture suppression, and use that same read root for loose art/font/script consumers. All enable/SavedVariables writes remain in local_state; the install stays read-only. Enumerate valid folder-name TOCs from the selected archive chain instead of the vanilla list, retaining first-registration precedence. Use an explicit archive-addon source so its relative TOC files/bindings gain the addon prefix without changing core FrameXML/GlueXML's already absolute paths. Vanilla discovery is unchanged. Existing loose Blizzard-folder suppression, missing-listfile visibility, independent UI/profile snapshot reload, startup scan cost and actual mod loading remain unverified limitations; no build, check or addon execution is authorized by this source work.

2026-10-27 explicit owner resume: Astra takes engine coroutine implementation while OC owns the playable-route/collision campaign. Exact initial claim is benilla-lua/src/{value.rs,table.rs,heap.rs,vm.rs,lib_base.rs}, a new engine implementation module if needed, and benilla-mlua/src/lib.rs. No app/world/WM file is claimed. First establish real thread values, heap-owned suspended stacks, closure/upvalue reachability, weak-table collection and rooted adapter handles; then add resumable control flow and the language library. Do not expose a fake coroutine library before continuation semantics exist. Preserve vanilla defaults. The previous single check/test grant was consumed; no new execution or test creation is inferred from resuming implementation. Use the codebase-design module/interface principles to keep execution ownership inside the engine rather than distributing it across addon call sites.

Coroutine storage milestone also claims benilla-mlua/src/conv.rs for rooted typed conversions. Thread values now use their own NaN-box tag/identity and heap arena; suspended contexts retain stack/frame/global/protected-call state. Suspended upvalues address that context rather than the active VM stack. Collection traces reached threads, honors weak thread keys/values, closes surviving upvalues before reclaiming an abandoned stack, and retains retired code while a suspended frame can reference it. Adapter marshalling no longer turns threads into nil. This is storage/GC groundwork: no creation/resume/yield library is installed yet, so installed-addon coroutine support remains incomplete. Coroutine memory bookkeeping uses the retained Rust thread record plus the existing stack/frame accounting, not a claim of exact 32-bit client lua_State size. References: official Lua 5.1 lgc.c, ldo.c and lstate.c. No build/test/interpreter/check script was run.

Coroutine continuation milestone (supersedes the preceding storage-only limitation): claim extends to benilla-lua/src/lib.rs, lib_coroutine.rs and vm/coroutine.rs. Explicit Wrath construction now installs create/resume/yield/status/running/wrap. Resume parks the complete caller context, roots active/parent threads, retargets open cells, and continues the suspended yield call with the next resume arguments. Ordinary completion/error closes cells and marks the coroutine dead; panic cleanup restores the resumer before propagating. Protected/native/metamethod reentry cannot yield across its Rust stack. Nested resumes share a Rust-stack origin and depth limit. The instruction budget and abort marker remain shared and an abort is propagated, never returned as resume(false). Lua errors retain arbitrary values; wrap adds caller location to string/number errors. No new capability is installed in the default vanilla constructor. Implementation source references are official Lua 5.1 ldo.c/lbaselib.c; this does not claim exact allocator accounting, WoW secure/taint behavior or host Thread::resume APIs.

This milestone is SOURCE-ONLY, uncompiled and untested. A future explicit verification grant must cover zero/trailing-nil values, repeated yield/resume, tail-position yield, errors/dead/running/normal states, nested resume and panic restoration, native/protected/metamethod yield rejection, budget abort propagation, cross-thread closure mutation, GC of suspended/abandoned cycles and weak thread keys/values, VM forks/hot reload, and installed-addon loading. No test files or test bodies were created and no builds/tests/linters/formatters/interpreters/check scripts/runtime were run. OC's parry BVH change a8a51af9 and checkpoint remain independently owned and preserved.

Coroutine failure-boundary follow-up: the internal resume result now distinguishes an error raised in the resumer from a coroutine refusal/execution error returned as false. Argument-stack exhaustion therefore raises once; cross-thread recursion refusal does not invoke the parent's xpcall handler. Running/normal/dead refusal text follows the public coroutine status. This corrects source behavior after comparing auxresume/lua_resume in the official 5.1 sources; execution remains unverified.

Native cast query continuation claim: UI script/host.rs, wrath_spell.rs and the app's existing ui_script/wrath_queries.rs and script_calls.rs query delegation. Consume only OC's committed read model; wrath.rs/wrath_native.rs remain read-only. Add scope-checked UnitCastingInfo/UnitChannelInfo with the target stock tuple shapes, native attempt serial for the cast ID, and GetTime-domain START/update anchors. Wire facts establish remaining time at START, not an exact pre-arrival start; show the observed interval without inventing server latency or haste. Cast delay slides that observed interval; channel updates preserve START and retime its end. Unknown interruptibility stays nil. Queries will not invent success, send commands, or infer a terminal from the UI clock. Event delivery is a separate continuation; current registration alone is not production delivery. Reference: target CastingBarFrame.lua at https://raw.githubusercontent.com/wowgaming/3.3.5-interface-files/main/CastingBarFrame.lua . No execution is authorized.

Native cast getters are now source-connected through the existing lent host and SpellQueries; script/mod.rs only re-exports the owned UnitCastView. Queries refuse an inactive world, mismatched session/world, absent clock/metadata or non-finite/reversed interval. Cast attempt IDs beyond Lua's exact integer range are refused. Channel START remains stable while its latest update retimes the end. The observed cast interval is a documented presentation limitation, not exact server-start reconstruction; metadata interruptibility, native event delivery and connected/visual acceptance remain open. No build/tests or runtime occurred.

Native cast event continuation claim: new app ui_script/wrath_cast.rs plus registration in ui_script/mod.rs. Read the existing WrathCastEvent stream after the net/unit feeds, shape native edges into UNIT_SPELLCAST_* through VmWith::feed so listeners have the lent query host. Preserve order, scope and attempt identity; no native reducer/transport or donor cast-report edit. Do not synthesize interruptibility or combat outcome from GO. Scope-loss cleanup may use bounded prior UI aliases only in that scope, never to clear a replacement world's bar. No test/runtime grant is inferred.

Native event feed source delivery: START/delay/launch/failure/interruption/channel edges now post the corresponding Wrath events with unit/name/rank/attempt ID through the existing hosted feed. GO posts SUCCEEDED plus STOP as client cast lifecycle, not damage/hit/WM action success. Scope loss posts only STOP/CHANNEL_STOP. Active-world session/generation checks reject stale events; teardown uses old aliases only while no replacement world is adopted. Metadata/aliases are per-VM, per-scope, per-caster/cast-or-channel/attempt and capped at 128 entries. Missing VM drains events rather than replaying them after reload. The source resolver covers the existing player/pet/target/mouseover/NPC/party/raid bases; focus, chain-token-specific notifications, exact client start reconstruction, interruptibility metadata and broad aura UI remain explicit gaps. A burst of edges is delivered after the net drain, so synchronous getters expose the final mirrored state rather than a historical per-edge replay. No acceptance is claimed; compilation, scheduler admission, hosted callbacks, reload/transfer/logout and rendered bars remain unverified.

Coroutine GC follow-up removes the redundant creation-time environment from thread headers: the active VM or parked context already owns the current globals, and the entry function owns its closure environment. A separately rooted initial environment would survive setfenv(0) unnecessarily, especially in the permanently rooted main-thread marker. No execution evidence was added.

Native aura continuation claim: new UI script/wrath_aura.rs, its script/mod.rs registration/export, script/host.rs, existing app ui_script/wrath_queries.rs and script_calls.rs delegation. Source facts: current native AuraSlot retains all visible slots, flags/charges/caster/max/remaining durations but no per-update revision/clock; repeated identical refreshes can be reduced as unchanged. Therefore neither query-time now nor movement/world revision can establish an honest expiration anchor. Current AzerothCore checkout D:/WOW/ACRB/source/azerothcore/src/server/game/Spells/Auras/SpellAuraDefines.h explicitly defines positive 0x10 and negative 0x80, and SpellAuras.cpp::_InitFlags/BuildUpdatePacket produces them. This contradicts the current WrathWorldView::auras comment that wire polarity is absent; its source is OC-owned and unchanged here. The vanilla descriptor projection drops slots >=48 and interprets different flag semantics, so it must not remain the Wrath Lua aura source. Consume native slots directly with proper Wrath tuples and filters, preserving unavailable expiration/policy data. Required upstream continuation: per-aura scope/object generation, refresh/removal identity or revision, and observation/monotonic UI anchors updated for every received aura edge, independently of movement and payload equality. No test/runtime authorization is inferred.

Native aura query source milestone: explicit Wrath now reads UnitAura/UnitBuff/UnitDebuff from all native slots through the lent query host, with the eleven-field target tuple, metadata, raw stack/charge count, actual wire polarity and caster aliases. Supports numeric index or name/rank, HELPFUL/HARMFUL, PLAYER and cancelability filters. CancelUnitBuff submits the existing self-aura cancel command only for a locally cancelable helpful aura; it does not delete native state. Native expiration clocks, stealability and consolidation policy remain unavailable (nil); absent wire duration uses the UI's 0/0 convention, which does not assert that the underlying aura is permanent. RAID filtering is explicitly unsupported until its native policy is established, rather than copying the donor's slot-based policy. Pet cancellation, focus/vehicle aliases, native high-slot change notifications and target aura tooltip selection remain open. Consumers assuming a numeric expiration for every timed aura are not yet supported: no mod/runtime acceptance is claimed. No build/test/linter/interpreter or live run occurred.

Native aura tooltip continuation claim: script/tooltip_aura.rs, script/wrath_aura.rs and app ui_script/wrath_queries.rs. Carry the native caster level through the read view, share the exact query/filter lookup for SetUnitAura/SetUnitBuff/SetUnitDebuff, and use the existing AuraKey/description cache/wait owner rather than a second tooltip builder. No native runtime file or spell catalog loader is claimed. Existing expiration and RAID-policy limitations remain; do not infer tooltip timer text from an unavailable clock.

Native tooltip source milestone: Wrath GameTooltip SetUnitAura/SetUnitBuff/SetUnitDebuff now share the native filtered lookup and hand native spell/level/stack identity to the existing aura description/wait pipeline, including slots beyond the donor window. The ordinary donor setters remain selected in vanilla mode. The receiver is validated before querying the host; the query releases model borrows before the tooltip builder can invoke Lua. Existing description-cache asks supply the requested native key on a miss; no parallel tooltip registry or builder was added. Duration text, RAID policy and runtime behavior remain unverified/open under the earlier contract limitations. No build, tests or runtime occurred.

Native unit-alias correction claim: app ui_script/wrath_cast.rs and UI script/wrath_aura.rs. The existing UnitGuids::tokens_of / UiScript::unit_tokens_of already owns the client's reverse mapping, player-presence guard, token ordering and first roster-row rule. Replace the two new hand-enumerated copies with that existing owner. This keeps native event and aura-caster aliases consistent with the field-event path rather than creating another unit-token policy. Scope/attempt state remains native-owned; focus/vehicle support is still outside the existing resolver and is not invented by this reuse.

Native aura notification claim: app ui_script/mod.rs, new ui_script/wrath_aura_feed.rs and ui_unit/field_events.rs::signal_unit. Suppress only the donor UNIT_AURA projection event in explicit Wrath mode; the new hosted feed compares full native aura lists for the unit resolver's current reachable GUIDs, scoped by VM/session/world/object generation. It posts UNIT_AURA through the existing hosted feed and token owner. It is UI change detection, not a second aura reducer or a clock: removal/scope cleanup drops display memos and no action or native state is changed. It cannot reconstruct identical refresh edges already discarded by the reducer; that still requires the upstream revision/clock contract.

Notification source milestone: registration runs after the existing unit feed. The memo uses held_unit_guids (bases and held target chains, not chain_end_guids alone), compares complete native slot records and object generation, moves unchanged snapshots and prunes no-longer-reachable entries. Event aliases come from the existing reverse resolver. The donor bridge's Unit notices 41/107 are suppressed only for Wrath; explicit Signal notices, including unit-flag visibility changes, remain on the original path. The narrower change is in field_events.rs::fire, not the shared signal_unit function. First snapshots and native list changes can notify beyond slot 47; identical refresh edges, historical per-packet replay, expiration clocks, RAID/stealability/consolidation policy, and native visual aura kits remain open. Source/Git inspection only; no compile, tests or execution.

Native power event claim: UI script/target.rs; app ui_script/mod.rs, new wrath_power_feed.rs and ui_unit/field_events.rs. Target UnitFrame.lua registers UNIT_RUNIC_POWER and UNIT_MAXRUNIC_POWER in addition to the donor power events; the target registry lacks both. The old numeric event arithmetic also cannot extend beyond the five donor kinds without colliding with health/max-mana IDs. Use the existing native seven-pair read model for current/max power and active-type notifications, respecting observed None versus Some(0), VM/session/world/object generation and the existing reverse resolver. Suppress donor power/type event projection only in explicit Wrath mode; preserve health and all vanilla-mode paths. Power kind 5 is an aggregate field, not the six individual rune readiness/type/cooldown facts; do not fabricate RUNE_POWER_UPDATE or GetRuneCooldown from it. Source references: target UnitFrame.lua and RuneFrame.lua at the wowgaming/3.3.5-interface-files repository. No execution/test grant is inferred.

Power projection suppression also claims UI script/inspect.rs for a point lookup on the existing resolver's held set. Gate suppression on a held object, preserving the established record/event path for out-of-range group units. This does not make native raw power getters work from group records; that remains a separate owner contract gap. Health event ID 22 is preserved, including its existing ambiguous donor arithmetic edge for aggregate power kind 5.

Native power event source milestone: explicit Wrath in-game listeners now admit the two runic-power events. A hosted feed over the current native resource owner emits current/max events for kinds 0–4 and 6 and active-type changes, only from available observed values; a valid zero is a change from unavailable. Its memo resets on VM/session/world/object generation and retains only resolver-held units. It generates no rune-slot readiness/type/cooldown facts. Donor power/type notices for held Wrath objects are suppressed to avoid competing projections; record notices and health paths remain intact. Aura bridge suppression now has the same held-object guard. Compilation, native/player/group event behavior and rendered bars remain unverified; no build/tests/runtime occurred.

Coroutine yielded-result ownership repair claim: benilla-lua/src/vm/coroutine.rs. After copying yielded values into the resume result, reset the suspended native frame top to its argument base, as moving yielded results out of the Lua thread does. Leaving them counted incorrectly retains them as GC roots and consumes the next resume's argument budget. Fresh admission also counts the separately retained entry-function slot. The saved yield frame still supplies destination/result count and Lua locals/open cells below it remain owned by the context. This is source reasoning against the existing poscall/GC invariants and Lua 5.1 auxresume; no interpreter/test is run while authorization is pending.

## Native aura availability counter-evidence — 2026-10-27

Pinned client source inspected: db63ec50. Native files are OC-owned and were not edited. Both local AzerothCore checkouts (D:/WOW/ACRB/source/azerothcore and D:/WOW/Azerothcore_WoTLK_Rebuild/source/azerothcore) report commit 5e741c37364f4ec5e3bc2860bcf21f13812fb66d and the same inspected ordering. This identifies source, not a verified running server build.

High-priority source defect: the claimed unobserved-versus-observed-empty aura distinction is not implemented end to end. reducer.rs::ObjectEntry has an aura vector but no observation state. WorldState::entry initializes it empty; CREATE clears it. Both producer adapters select unknown only when the whole player entry is absent. For any existing entry, app wrath_native.rs::auras_observation and the driver's duplicate helper unconditionally return Known, with complete determined only by the export bound. Thus a self create with no aura packet can already be exported as observed zero auras. The helper test passes an empty slice and labels it observed; it does not establish that the input was ever observed.

The checked server source supplies a concrete initialization boundary: Map.cpp sends the self create packet, then calls GetAurasForTarget(player, true). Player.cpp::GetAurasForTarget builds SMSG_AURA_UPDATE_ALL with the target GUID even for an empty forced snapshot. Other visible targets use the non-forced path and may omit an empty aura packet, so self and general visibility initialization must not be conflated.

Source counterexample (not executed): process the self create and request an export before its following forced aura snapshot. The current export is Known/empty. Next process a GUID-only empty AURA_UPDATE_ALL: reduce_aura_update runs no slot blocks, returns changed=false and emits no event. SessionState consequently has no observation revision for the newly received empty snapshot. A nonempty full snapshot is also dispatched to the same merge routine as a delta, so it does not remove held slots omitted from the snapshot. Identical refresh blocks likewise carry no new revision or timing anchor. Whether these windows occur in a live export cadence remains undemonstrated; the source contract itself lacks the necessary distinction.

Required native-owner repair: retain aura observation/coverage state per object generation; distinguish full snapshots from deltas; count an empty full snapshot and identical refresh as observation edges; advance an aura revision/receipt clock independently of movement/payload equality; export unknown until a defensible initialization/observation boundary, and route the new anchors into UI expiration conversion. Whether AURA_UPDATE_ALL must replace the held set or is client-side batch-merge semantics needs reference/contract confirmation: the server writer proves that the packet enumerates the current visible set, not how the closed client treats prior slots. Do not guess that policy from the opcode name. If complete replacement is the approved contract, parse/validate the full set before replacing it. Preserve the non-forced remote-unit initialization nuance rather than declaring all remote units complete from one partial slot. No second reducer or UI inference from query time should be added. Both producer copies must consume the same semantics. Existing receiver/schema acceptance does not prove this contract, and no previously reported test result is retracted or extended. No builds, tests/new cases, imports, services or socket/game execution were performed.

## Explicit one-time verification — 2026-10-27

After the human approved one bounded check/existing-test pass, Astra pinned e3c83d90405067ff88516b67e83f8fdc5f8a8ebb in a temporary detached checkout. This includes OC's adc8a631 repair: per-generation aura availability, empty/identical full-snapshot observation edges, both producer guards, digest propagation and UnitAura refusal before observation. That source repair addresses the default-empty availability defect; identical delta refresh timing and full replacement policy remain open. The unrelated dirty/untracked owner files were preserved.

One `cargo check --locked --offline -p benilla-app` passed with warnings. One `cargo test --locked --offline -p benilla-lua -p benilla-mlua -p benilla-ui` exited 101: completed Lua/mlua suites passed; UI 3321 passed / 3 failed / 1 ignored. The same census path assertion and two timezone assertions, with matching actual/expected values, were already retained in the 2026-10-26 pass. Doctests/later targets were not reached. Total 6m44.7s under the shared 30-minute deadline; no fixes/reruns/new tests. Temporary checkout was clean and removed. Logs and exact failures remain under .scratch/project-b/verification/2026-10-27-astra-once/summary.md. This verifies compilation and completed suite results only, not new Wrath runtime semantics or playable/rendered acceptance. The grant is consumed.

OC review/next assignment requested by the human: D:/WOW/project-b-system-design-review/grades/grade-20261027_091543-oc-native-alpha.md and its paired instructions/manual guide. The source review found that wrath_ui_slice_tests::player_frame_script uses UiScript::new (vanilla target), and its data gate can early-return; the reported one-test pass alone does not prove actual Wrath host/data admission. No OC test was modified or rerun by Astra.

Windows census portability repair claim: script/census.rs::Applying::turn, without editing test cases. Both authorized passes showed the caller key retaining a Windows crate path because the helper only strips `src/`, whereas Rust caller locations use backslashes in this build. Normalize separators before applying the existing src-relative key rule. This fixes the keying mechanism, not the expected assertion, and keeps Linux keys unchanged. No test/build rerun is inferred after the consumed one-time grant; this follow-up remains source-only.

Aura availability notification follow-up claim: app ui_script/wrath_aura_feed.rs only. The existing native owner now distinguishes unobserved from observed-empty, but the UI memo compares generation and slot payload alone. Include auras_observed in the presentation memo so a forced empty snapshot can notify UNIT_AURA even when both slot vectors are empty. Existing first-visible, alias, removal and scope behavior is retained; this does not supply identical refresh revisions or expiration anchors. OC owns those native contracts and its currently dirty reducer/session/wrath files remain untouched. Source/Git inspection only; the earlier verification grant is consumed and this repair is uncompiled/untested.

Wrath spellbook source counter-evidence at c1de419f: target SpellBookFrame.lua::SpellBook_GetTabInfo reads highestRankOffset/highestRankNumSpells (returns five/six) from GetSpellTabInfo when ShowAllSpellRanks is false; SpellBook_GetSpellID then calls GetKnownSlotFromHighestRankSlot. Our script/spellbook.rs binding returns only four values on every path, and source search finds no GetKnownSlotFromHighestRankSlot implementation. The same target file calls IsPassiveSpell and IsSelectedSpell, whereas the inherited binding names are IsSpellPassive and IsCurrentCast. GetCVarBool also has no registration in the inspected UI Rust source; UIParent.lua does not define it. These are source gaps, not observed live failures or proof that no other loaded Lua defines a helper. Reference: https://raw.githubusercontent.com/wowgaming/3.3.5-interface-files/main/SpellBookFrame.lua (SpellButton_OnModifiedClick, SpellButton_UpdateSelection, SpellButton_UpdateButton, SpellBook_GetSpellID and SpellBook_GetTabInfo), and UIParent.lua in the same repository.

Next UI implementation boundary: rank-collapsed display slots must map back to actual learned book slots across tab boundaries and book refresh, keeping cast/pickup/cooldown/tooltip identity coherent. Existing name/rank sorting is not itself authoritative rank-family identity; do not collapse distinct spells merely because localized names match. Trace the target mechanism or obtain an explicit supported contract before implementing this projection. GetSpellCooldown additionally still requires the donor slot/bookType form; extending ID/name queries must reuse the existing cooldown authority and preserve initialization availability. OC is currently editing that initialization path; no in-flight source is accepted as delivered and none of its files were edited here.

Native aura timing consumer claim: app ui_script/wrath_queries.rs and wrath_aura_feed.rs, plus UI script/wrath_aura.rs. The inspected in-flight native owner introduces AuraReceipt, per-slot aura_receipts and WrathAuraAnchors::ui_receipt. Prepare UI reads against that contract, preserving timed expiration as unavailable until the anchor matches the current receipt revision. Compare receipt identity in the event memo so an identical refresh can notify. Tooltip time formatting must use the same anchored expiration and existing duration formatter, never query-time renewal or local removal. These consumer changes depend on OC's uncommitted native contract and will remain explicitly uncommitted until that dependency is delivered. No native-owner edits, test changes, builds or execution are included in this claim.

Consumer implementation is now present in those three claimed files, still uncommitted with the native dependency pending. SpellQueries reads the optional native presentation anchor resource and requires its revision to match the slot's current receipt, with a finite/nonnegative GetTime origin. Expiration is that fixed origin plus the wire remaining duration; it stays nil when the timed slot has no matching anchor. Hidden-duration slots retain 0. The aura event memo compares receipt maps as well as availability, generation and wire slots, moving unchanged snapshots rather than cloning them every frame. Tooltips reuse the existing duration formatter and localized strings, clamp only displayed remaining milliseconds at zero and never remove an aura. This consumes the first presentation-receipt interval supplied by OC's current anchor design; it does not establish exact IO-arrival-to-UI latency compensation or server-start reconstruction. The UI clock survives VM reload by the existing clock owner; there is no per-query clock origin. No checks/tests/runtime were run, and no native source is accepted as committed by this working record.

Wrath string-helper claim: benilla-lua/src/lib_string.rs, new lib_string/wrath.rs and vm.rs (crate-private reuse of the existing concatenation operator only). The target SecureTemplates.lua directly captures strsplit for modifier and unit/header parsing; the inherited host deliberately omits post-1.12 helpers and the explicit Wrath constructor currently adds only match/gmatch/reverse. Add strsplit/strtrim/strjoin/strconcat only to that constructor. Behavioral reference: the independent MIT Elune implementation, wowless/wowless elune/liblua/lstrlib.c blob 69c829063337c6d9c4683d38c5d6b574bb9cfe18 (str_split/str_trim/str_join/str_concat). Elune's README explicitly calls its WoW environment an approximation; it is not 12340 binary proof. Preserve delimiter-set and C-string boundaries, split limit/empty fields, binary-safe concatenation through the existing Lua owner and existing stack/allocation bounds. No runtime/library replacement or new test cases are authorized; no complete secure-template or installed-mod acceptance follows from adding these helpers.

String-helper source milestone: four native globals are now installed only by open_51. strsplit uses byte delimiter sets, preserves empty/trailing fields, treats zero as unlimited, and keeps the unsplit remainder under a part limit. Its output is bounded by the existing native frame capacity. strtrim uses the specified byte set or space/CR/LF/tab, without UTF-8 conversion or unsafe pointer arithmetic. strconcat and the empty-separator/zero-or-one-piece join path share Vm::concat (including __concat, numeric formatting and the one-value identity case); multi-piece join uses the reference's C-string piece boundaries and length-delimited separators, with fallible bounded buffer growth. No library/VM replacement, test creation or execution occurred. The UI stdlib comment is updated to distinguish its shared vanilla layer from explicit Wrath installation. The implementation is uncompiled/untested; exact 12340 corner-case compatibility and installed-addon/secure-template acceptance remain open. The pending aura consumer files and OC's native work remain outside this commit.

Wrath CVar boolean query claim: UI script/cvars.rs only, over its existing case-insensitive applied-value table. Target SpellBookFrame.lua and UIParent.lua call GetCVarBool; this missing registration must not be supplied by a second settings store or fabricated defaults. Behavioral reference: wowless/wowless at c09acebd9be7d52e41154dff5276f4d62f3e4970, wowless/modules/cvars.lua, C_CVar.GetCVarBool compares the existing string with '1' and leaves an absent row nil. Adapt that read semantics to the target's global name only in Wrath mode, reusing the existing argument marshaller. This is an explicit surrogate contract, not target-binary proof of all error/unknown-value cases. Preserve host registration, latched/pending values, callbacks, persistence and the vanilla surface. ShowAllSpellRanks registration and rank mapping remain separate open work; an unknown row must not be registered merely to make the spellbook load.

GetCVarBool is now source-connected to that existing applied-value table only in explicit Wrath mode. Known '1' reads true, other known strings false, absent rows nil; argument coercion uses cvar_arg and invalid arguments raise the named usage error. No writes, default rows, callbacks, events, save behavior or latched commits were added or changed. No build/tests/runtime were performed, and no complete target CVar compatibility or spellbook acceptance is claimed.

String-helper allocation repair: source review of Heap::intern_with showed that its fill closure extends an unreserved scratch Vec before Heap::intern applies its fallible allocation path. The new split/trim helper used that path, so it could abort on allocation failure instead of returning Lua's memory error. Replace that use with a bounded/fallible buffer, reused across all split fields, and immediately propagate an interning failure before continuing the loop. No generic heap behavior or other worker file is changed. This repairs our 679c92ec source implementation; allocation-failure execution and all later checks remain unperformed.

Native dependency delivered: OC's 6feb5d4d and read-only consumer handoff (166c90a6, later citation/provenance corrections through 29ff13f9) now commit the receipt map and presentation resource this UI work consumes. The source implementation and accessors were inspected; no earlier OC check/test claim is adopted as Astra execution. The pending consumer changes can now be committed independently. The human's new one-time Lua/mlua/UI check/test grant pins 29ff13f9 and explicitly excludes these pending edits; it does not verify the app integration or permit an extra run after this commit.

Handoff discrepancy retained explicitly: OC's consumer document proposes nil expiration for AFLAG_DURATION-absent slots. Preserve this UI binding's existing 0/0 no-timer convention instead: target UIParent.lua's RefreshBuffs/RefreshDebuffs path subtracts duration from expiration for displayed auras. Nil would break that arithmetic. This is a presentation convention and does not promote a missing wire timer to a known permanent aura in native/WM state. Target source pin inspected through GitHub: wowgaming/3.3.5-interface-files d0339b17b0221db76e6acd2dc2915d224a5b62ca, UIParent.lua aura cooldown setup. Timed slots still refuse a missing/mismatched receipt anchor; exact arrival latency, full snapshot replacement and runtime acceptance remain open.

Explicit second 2026-10-27 grant consumed at 29ff13f9: one check of benilla-lua/mlua/ui passed with warnings; one existing-test invocation exited 101. Completed Lua/mlua suites passed; UI 3322 passed / 2 failed / 1 ignored. The census path case now passes, while the same two retained timezone assertions still fail with the same actual/expected values. Later targets/doctests were not reached. Total 237.7s under 30 minutes, no timeout, no fixes/reruns/new cases. Exact summary/logs: .scratch/project-b/verification/2026-10-27-astra-ui2/. The clean temporary checkout was removed safely. c002a798's aura consumer integration was deliberately excluded from this pin and has no Astra execution evidence. No app/game/graphical/socket/WM/server/DB run occurred; no replacement gate closed.

Existing Wrath-host fixture repair claim: app ui_script/wrath_ui_slice_tests.rs only. Change the existing smoke fixture's constructor/data loader, without creating a test file/case or running a check. Current source selects UiScript::new (vanilla), and shared test_ui::load_ui additionally installs UIPARENT_STAND_INS, including no-op functions and lazy fake frames. That contradicts the fixture header's no-stub claim. Use the explicit Wrath constructor and a direct selected-profile chain provider, require the selected FrameXML.toc interface to be 30300, report profile/data admission, and refuse loader errors/missing includes. Absence can retain the standard data-gated skip, but BENILLA_REQUIRE_DATA=1 must fail missing profile/data admission. Do not change shared vanilla fixtures or manufacture missing APIs/CVars/frames. Future execution requires a fresh grant; the completed Lua/UI pass does not cover this fixture or these source edits.

Fixture correction is source-applied in the existing case only. It explicitly opens WM_ASSET_MANIFEST/WM_ASSET_PROFILE on the resolved data root, checks FrameXML.toc interface 30300 and uses its actual file order through PlayerFrame.xml instead of the inherited hand-built prefix. A fresh chain/provider prevents the shared cached source and UIPARENT_STAND_INS from manufacturing admission. Errors, missing includes and loader warnings refuse acceptance; the diagnostic names target/profile/manifest/data. Use BENILLA_REQUIRE_DATA=1 and retain test output for an acceptance invocation; a standard no-profile/no-data skip remains no evidence. The health-bar existence assertion now names PlayerFrameHealthBar, as the pinned target PlayerFrame.xml does (wowgaming/3.3.5-interface-files d0339b17b0221db76e6acd2dc2915d224a5b62ca). Existing manual health-event, query and CVar readback intent is unchanged. floatingCombatTextCombatDamage is absent from the inspected host registration table and remains an honest potential failure; no made-up registration or alternate easy CVar was substituted. Strict prefix loading may reveal earlier unsupported UI behavior. No new test file/case, check/build/test execution or acceptance result was produced.

OC repair review through 5b4bcdc3 (source-only Astra review): 7f1b73c3 now parses every aura block into an owned staging list before creating/touching an entry, assigning receipt revisions or changing coverage. This resolves the malformed-later-block partial-mutation finding in source without deciding snapshot replacement. 7f9ea050 initially reconstructed cooldown arrival from a wall-clock delta; that left a clock-adjustment defect. 5b4bcdc3 supersedes it: the net loop captures Instant immediately after world.recv, carries Some(received_at) in the in-process SpellBook event, and the existing handler passes it to seed_initial. The 1.12 decoder carries None and retains consumption-time fallback. No wall-clock subtraction remains in this cooldown path. Both reviewed source findings are addressed; no new check/test/run was performed by Astra, and commit-message evidence is not treated as her execution. The corrected Wrath-host fixture's focused-run request remains unanswered. Full-snapshot policy, actual host/runtime behavior and the Northshire G/L grant remain open.

Focused smoke grant subsequently arrived and was consumed exactly once at 404cea29, after the human's requested pre-test OC source review. The app library test binary compiled, and the one selected test admitted Wrath335/interface30300/wm-hd with required data and no fixture stand-ins. It failed at Constants.lua:87 because FillLocalizedClassList is nil. PlayerFrame/manual event/query/CVar assertions were not reached. Total 680.8s (11m21s), including 11m07s build and 7.40s test runtime, below 20 minutes; no timeout, rerun or new case. The clean temporary checkout was removed safely. Exact logs/summary: .scratch/project-b/verification/2026-10-27-astra-wrath-smoke/. This supplies actual app-test compilation evidence for the pinned aura consumers and a concrete Wrath startup blocker, not passing host/gameplay acceptance. Next repair must use the existing class catalog/locale owners rather than a success stub; the complete API/data-admission contract still needs source inspection. No graphical/live/socket/WM/server/DB operation occurred; the full-replacement gate remains open.

Astra class-name binding claim: benilla-ui/src/script/{mod.rs,model.rs,client.rs,target.rs}, new script/class_names.rs and a consumer handoff. OC owns formats/catalog/bootstrap and supplies actual rows; no catalog/bootstrap or app test file is claimed here. Add an immutable UI read projection of native byte class ID, stable file token and separate observed male/female labels, with locale identity and explicit unavailable/clear behavior. Install FillLocalizedClassList only in the shared Wrath surface (glue and game). Validate candidate metadata before replacing the view, release Model borrows before any caller-table writes, preserve unrelated keys and return the same table. Source contract: target Constants.lua at d0339b17b0221db76e6acd2dc2915d224a5b62ca calls the API for both genders; independent Wowless c09acebd9be7d52e41154dff5276f4d62f3e4970 data/impl/FillLocalizedClassList.lua and data/sql/FillLocalizedClassList.sql enumerate stable filenames, choose the requested name column, fill and return the table. This behavioral reference is not exact 12340 ABI proof. No English fallback, copied client assets, success stub or assumed gender fallback is authorized. Also preserve vanilla's eight locale names while adding Wrath's known esMX=7/ruRU=8 mapping from the pinned AzerothCore Common.h/Common.cpp; invalidate the class projection when its locale changes. Source-only, no tests/new cases or runs under the consumed grant.

Astra table-library claim: benilla-lua/src/lib_table.rs only, plus this checkpoint. Source review confirms getn/setn already dispatch length semantics by dialect; do not misreport those as missing. Remaining inherited 5.0 paths differ from official Lua 5.1.5 ltablib.c: insert arity must be exactly 2/3; remove must leave the table unchanged outside 1..length; concat must honor an explicit last=0, check table before range arguments, and identify an invalid element's type/index; obsolete setn still validates its table argument first. Repair only Wrath paths, retain existing vanilla behavior and MAX_SHIFT bounds. Primary baseline: https://www.lua.org/source/5.1/ltablib.c.html (tinsert, tremove, tconcat/addfield, setn). This is the selected Lua 5.1 dialect baseline, not proof of every 12340 customization. No test cases or executions authorized.

Table-library repair delivered in source: insert now rejects every non-2/3 arity in Wrath before mutation; remove returns zero results without writing for positions outside 1..length; concat dispatches to a separate 5.1 implementation that keeps explicit zero/negative end indices, reads arguments in 5.1 order, preserves binary separator bytes using fallible buffer reservation and names the offending element's type/index. setn validates its table before the Wrath obsolete-function error. The donor concat body, vanilla insert/remove branches, existing MAX_SHIFT bound, length owner and table storage are preserved. Reviewed source call chain: new_state_with_dialect selects Wrath51; library functions inspect that dialect; UI stdlib tinsert/tremove aliases reference these functions, so the repairs apply to both namespace and alias calls. No compilation, tests, new test cases or runtime execution was performed; the earlier bounded pass predates these changes. Official Lua 5.1.5 is the explicit behavioral baseline; the exact 12340 customized ABI and post-change behavior remain unverified.

Spellbook review did not justify a guessed implementation: pinned target SpellBookFrame.lua still consumes GetSpellTabInfo returns five/six and maps display slots through GetKnownSlotFromHighestRankSlot. Current ui_spellbook::build_book publishes only flat slots; SkillLineCatalog::rank_successor follows SkillLineAbility.forward_spell_id, documented to cover only server-superseded abilities. That is not a complete learned-rank family relation. Do not collapse equal localized names, make an identity mapping or report rank-aware stock spellbook acceptance. This dependency stays open; no spellbook source or tests were changed in this review.

Astra string-format claim: benilla-lua/src/lib_string.rs only, plus this checkpoint. Audit found the shared formatter still truncates its format at NUL, emits a raw CR in %q, and uses the long-string %s shortcut for an explicit dot with no precision digits. Official Lua 5.1.5 lstrlib.c reads the complete format, escapes CR in addquoted and only shortcuts %s when no dot appears. The independent Elune lstrlib.c at the previously pinned Wowless commit agrees, retains WoW positional formatting and also supports two-digit argument selectors and %F as %f. Implement Wrath-only handling with those explicitly qualified references; preserve vanilla's one-digit selector/C-string/%q behavior and shared CRT formatting owner. Replace the newly added Wrath-only reverse's unchecked collect with fallible reserve. No claim of exact target ABI or all native allocation paths; no builds/tests/new cases authorized.

String-format repair delivered in source: Wrath formats now consume their complete byte length and reserve literal output fallibly; %q escapes CR as backslash-r; an explicit dot suppresses the long-string %s shortcut; positional selectors accept up to two digits and %F uses the existing %f CRT path, matching the independent Elune customization reference. Vanilla retains its C-string format, one-digit selectors, raw CR quoting and donor %s shortcut. The Wrath-only reverse now reserves its output before extending it. Namespace string.format and the UI format alias both reach this same dialect-dispatched function; no second formatter or runtime was added. Official reference: https://www.lua.org/source/5.1/lstrlib.c.html (str_format/addquoted); customization reference: https://github.com/wowless/wowless/blob/c09acebd9be7d52e41154dff5276f4d62f3e4970/elune/liblua/lstrlib.c (scanarg/str_format). These are source-reviewed compatibility repairs, not exact 12340 ABI proof. No build/test/new case or runtime execution occurred. Existing numeric/CRT buffer-allocation and broad language/secure-UI acceptance are not newly proven by this change.

Astra event-admission claim: benilla-ui/src/script/target.rs only, plus this checkpoint. Pinned target wowgaming/3.3.5-interface-files d0339b17b0221db76e6acd2dc2915d224a5b62ca UIParent.lua, PlayerFrame.lua, UnitFrame.lua, CastingBarFrame.lua, BuffFrame.lua, RuneFrame.lua, PetFrame.lua, TargetFrame.lua, PartyMemberFrame.lua, FocusFrame.lua and ComboFrame.lua were fetched read-only. Their literal RegisterEvent subscriptions name 162 distinct events; 59 are absent from the donor reference/events.tsv and 12 of those are already in the Wrath extension. Add the remaining 47 observed names to the in-game Wrath extension. The current frame RegisterEvent silently ignores an unrecognized name (it does not throw); adding names repairs listener admission, not event production, native event IDs, complete addon event coverage or the Wrath unknown-name error ABI. Keep glue and vanilla catalogs unchanged. This source addition is outside the currently running verification pin f70be142 and gets no execution claim.

Astra event-query claim: benilla-ui/src/script/event.rs and object/events_regions.rs only, plus this checkpoint. Actual installed IgnoreMore 1.1.1 (Interface30300), ignoremore.lua:85 calls GetFramesRegisteredForEvent as varargs and invokes each existing OnEvent; SHA256 D4ED811148C9D553E9CB230430893A2E7149A48A4FF2B43511752A630D0A1AA8. Installed Details core/network.lua:606 checks its listener:IsEventRegistered before registering CHAT_MSG_CHANNEL; SHA256 90C9D19FB352FB5381AC1E9406EF4DA04AECFD84029DEE9B596735D0FF515954. Both host functions are absent. Add Wrath-only synchronous queries over existing event_to_frames/frame_events, with canonical case-insensitive lookup, registration order, zero results when no matching listener and a boolean frame answer. Reuse cached frame wrappers, release Model borrow before materialization and skip dead handles; do not emit events, recreate frames or run addons. Independent behavioral reference: wowless/wowless c09acebd9be7d52e41154dff5276f4d62f3e4970 wowless/modules/events.lua, GetFramesRegisteredForEvent[Unpacked] and IsEventRegistered. Modern reference's separate allregs is not a second store to copy; this host's RegisterAllEvents already registers in each existing list. Exact 12340 error/coercion/unit-filter ABI remains unproven. No builds/tests/new cases or runtime grant remains.

Event queries delivered in source: shared Wrath initialization installs GetFramesRegisteredForEvent; Wrath frame method installation adds IsEventRegistered. Queries read the same lists/sets existing registration/unregistration already maintain. The global returns ordered varargs of live cached wrappers (zero when none), not a table or copied frame objects. The frame predicate returns a boolean membership answer after canonical lookup. Listener ID collection finishes and releases Model before any wrapper access; duplicate prevention and RegisterAllEvents behavior stay with the existing registration owner. No addon source was copied/modified/run and no event was fired by these queries. Both this change and 2318d773 are later than the bounded f70be142 check/test snapshot; compilation and behavioral verification remain unexecuted for them. Exact target unknown-event errors, coercion and unit-filter extra returns, addon networking/ignore outcomes and broad mod acceptance remain open.

Astra mouse-over claim: benilla-ui/src/script/region_map.rs, object/layout_methods.rs and region/layout.rs only, plus this checkpoint. Pinned target UIParent.lua defines MouseIsOver(region, topOffset, bottomOffset, leftOffset, rightOffset) solely as region:IsMouseOver(...); no host implementation exists. Actual installed Bagnon, Bartender4, HandyNotes, Questie-335 and Details/LibGraph sources call the wrapper. Add Wrath-only shared Region-map arms over the current layout/cursor owners, covering frame/texture/fontstring receivers, resolved geometry, scale and edge offsets without a second hit-test store. Independent implementation reference: brues-code/ClassicAPI 317539dbd8aa9e9cd7a23441c0e355bb85c7e2ab src/frame/Modern.cpp::Script_IsMouseOver and docs/API.md: corresponding edge addition, cursor divided by region scale, strict interior, false for unresolved geometry/zero scale, no mouse-enable/visibility/occlusion gate. That reference explicitly derives strict-edge behavior from 4.3.4; it is not exact 12340 ABI proof. Record this limitation rather than silently promote it. Preserve vanilla method tables, pointer dispatch and clipping; no builds/tests/new cases or UI/runtime run is authorized.

Mouse-over source delivery: both Wrath Region-map arms now resolve their existing frame/region layout before reading geometry, convert edges with LayoutUnits::edge and cursor with the existing cursor_position projection divided by the same effective scale, add the four local edge offsets and return the strict-interior predicate. No current-hover or mouse-enable/visibility gate is substituted. Missing rect/zero scale returns false; no rect is invented. Offsets use existing numeric coercion, with absent/non-numeric values zero under the independent reference's policy. The shared method dispatcher preserves receiver-kind handling and one callable across frame/texture/fontstring tables. Vanilla method_names and actual pointer capture/hit clipping are unchanged. Source review only: no build, test/new case or UI run; the independent 4.3.4-derived edge/coercion behavior still needs exact 12340 evidence before acceptance. Concurrent formats/chr_classes.rs, formats/dbc.rs and formats/locale.rs dirty work appeared during this slice and was preserved outside Astra's explicit paths.

Astra installed-mod SetShown claim: benilla-ui/src/script/object/frame_state.rs and region.rs only, plus this checkpoint. The selected install's Questie-335/Libs/LibUIDropDownMenu/LibUIDropDownMenu.lua SHA256 EA62513AABD094C46BE023C523F6C611DB1E254ACBA148D9068587176DFB586C contains unguarded expandArrow:SetShown(info.hasArrow) at 1163 and button:SetShown(button.customFrame == nil) at 1315; no SetShown definition exists anywhere in the inspected Questie Lua tree. The host exposes none. This is an owner-installed-mod compatibility extension, not a newly asserted stock 12340 method. Add only in Wrath mode, using the existing frame visibility::set_shown and region set_region_hidden owners, with ordinary Lua truthiness (nil/missing/false hide, numeric zero and empty string show). Reference brues-code/ClassicAPI 317539dbd8aa9e9cd7a23441c0e355bb85c7e2ab src/frame/Modern.cpp::Script_SetShown explicitly delegates to per-kind Show/Hide with lua_toboolean. Retain receiver-kind gates; no second visibility flag, Lua Show/Hide override call, security bypass or synthetic event. Preserve vanilla's method tables and concurrent OC formats work. No new execution/test-case grant exists.

Same-slice exact claim extension: benilla-ui/src/script/button.rs, SetEnabled registration only. The same Questie file calls expandArrow:SetEnabled at 1164 and button:SetEnabled at 2080. Expose the Wrath installed-mod setter through existing set_enabled, preserving Button/CheckButton kind validation, texture/highlight writes and layout relinking; ordinary Lua truthiness as the independent ClassicAPI src/frame/SetEnabled.cpp reference uses. Do not change IsEnabled's inherited numeric return or claim exact 12340 setter presence. The actual subsequent Get/SetMotionScriptsWhileDisabled requirements are not stubs to add; their state/input policy needs a separate owner-level implementation and remains open. Existing secure-frame and enable-script behavior is not newly established by these setter aliases.

Installed-mod visibility/enable setters delivered in source: Wrath frame SetShown calls the same helper as frame Show/Hide; texture/fontstring SetShown writes through the same region-hidden owner as their Show/Hide; Button/CheckButton SetEnabled calls the existing set_enabled state/texture/layout owner. Lua truthiness comes from the existing mlua bool conversion, so numeric zero is truthy. No Lua override, separate visibility/enable flags or simulated OnEnable/OnDisable behavior was introduced. Vanilla gains no method. No compilation/tests/new case or addon/UI execution was performed; the last verification pin f70be142 excludes this work. This does not accept Questie's dropdown: motion-script support and the library's wider modern helper assumptions remain open. The existing secure-frame/protected-operation implementation remains an independent gap; delegating these setters does not close it. OC's concurrent four formats files were preserved outside the explicit commit paths.

Astra disabled-motion claim: widget/kinds/mod.rs, script/button.rs, script/pointer.rs, script/mouseover.rs and loader/widgets.rs under crates/benilla-ui/src, plus this checkpoint. Installed Questie LibUIDropDownMenu at the pinned hash calls Get/SetMotionScriptsWhileDisabled; Modules/WorldMapButton/QuestieWorldMapButtonTemplate.xml sets motionScriptsWhileDisabled=true on a Button template. Implement a per-ButtonState flag (default false, naturally retired with the arena object), Wrath-only getter/setter and XML application at each inherited level only when the attribute is present. Central hover eligibility must admit the flagged disabled button's own OnEnter/OnLeave but retain disabled input/state/visual guards. Skip enabled-path label refresh/hyperlink routing on this script-only branch; retain native class leave side effects and existing hide/capture cleanup. Capture branch choice before Lua callbacks so enabling the button inside the script does not double-enter. Wrath enter eligibility must be checked at the actual notification after the old leave handler, retaining vanilla ordering/guards unchanged. Reference: brues-code/ClassicAPI 317539dbd8aa9e9cd7a23441c0e355bb85c7e2ab src/frame/MotionScriptsWhileDisabled.cpp, explicitly an independent 4.3.4-derived installed-mod extension rather than exact 12340 evidence. The setter uses existing StringToBOOL-style flag parsing, not the ordinary Lua truthiness used by SetShown/SetEnabled. No fake getter, second hover index, engine enable, new cases/tests/build/runtime or OC-owned edit is authorized.

Disabled-motion source delivery: the per-button flag defaults false and persists across enable/disable state changes; destruction retires it with ButtonState. Getter/setter are Wrath-only and use the existing kind gate/flag parser. XML walks existing inherited levels, applying only present nonempty attributes; omitted values preserve a template's setting, explicit false overrides it. The central hover predicate admits a flagged disabled button without changing is_enabled or input guards; hide/removal and pointer-exit paths already call that predicate. Wrath's enter decision is re-read after the old leave and checks that the intended hover still exists; its script-only branch is captured before Lua, skips hyperlink routing and the normal label tail. Leave captures the branch before native class side effects/Lua, keeps class cleanup and skips the normal label tail. No state enable, forced click/drag or synthetic enable script was added. Vanilla's branch selection and event ordering are preserved. No build/test/new case, addon or graphical run was executed; f70be142 verification predates the entire extension. Required future cases: disabled flag off/on; no click/highlight/normal-label effects; enable-inside-enter exactly once; removal/window-exit leave; inherited true/absent/explicit false; invalid receiver and flag parsing; vanilla method absence. These are acceptance inputs, not permission to create/run tests. Exact 12340 behavior is not claimed by the independent extension reference. All concurrent OC metadata/bootstrap paths remain outside this commit.

Additional reference evidence (not local binary verification): brues-code/ClassicAPI at
317539dbd8aa9e9cd7a23441c0e355bb85c7e2ab docs/raw_wotlk_methods.txt reports the recovered 3.3.5
Region IsMouseOver at 0x0049E100, Frame IsEventRegistered at 0x0049F160 and Button
GetMotionScriptsWhileDisabled/SetMotionScriptsWhileDisabled at 0x00978360/0x009783B0.
docs/raw_wotlk_globals.txt reports GetFramesRegisteredForEvent at 0x0081BE70. Thus the prior
installed-mod motivation/independent 4.3.4 implementation reference must not be read as a claim
that these names are absent from stock 3.3.5. Method presence has independently reported target
evidence; exact boundary/coercion/callback ABI is still not established by a registration dump.
SetShown and SetEnabled were not found in that reported method dump; they remain explicitly
installed-mod extensions here, not presumed stock APIs. The dump authors describe recovering
the tables from a ChromieCraft 3.3.5a executable; Astra did not extract or execute that binary.
Pinned reference URLs:
https://github.com/brues-code/ClassicAPI/blob/317539dbd8aa9e9cd7a23441c0e355bb85c7e2ab/docs/raw_wotlk_methods.txt
https://github.com/brues-code/ClassicAPI/blob/317539dbd8aa9e9cd7a23441c0e355bb85c7e2ab/docs/raw_wotlk_globals.txt

Fresh explicit grant: up to eight regression cases in existing UI source files, one cargo check
and one benilla-ui library test invocation within 30 minutes, no reruns or app/runtime runs.
Astra added exactly eight cases in script/target.rs at b52ad4d3. The pinned isolated pass
compiled successfully and ran 3,333 existing/new UI cases: 3,329 passed, three failed, one
ignored. Seven new Wrath cases passed; installed_mod_setters_reuse_visibility_and_button_state
failed at its first texture SetShown call. The other two failures are the retained timezone
expectations. No test was weakened, skipped or rerun. Logs: ../verification/2026-10-27-astra-ui4/.

Concrete failure repair claim: script/region.rs leaf-method publication only. The SetShown
callback exists in the full region table, but the Texture/FontString installers copy only
the fixed donor method lists, omitting this extension. Publish that already-existing callback
to the two Wrath leaf tables explicitly, leaving the title/vanilla lists unchanged. This is a
reproduced method-registration omission, not speculative race/security hardening. The failed
assertion stays intact. No new state or mechanism is needed and no rerun is authorized.

2026-10-27 follow-up ownership: Astra claims only benilla-lua/src/lib_base.rs,
benilla-lua/src/lib.rs and benilla-ui/src/script/newproxy.rs for the landed OC newproxy
contract correction. These files are clean at fcbd2481; OC's dirty chat, region, model,
bootstrap and native-query files are excluded. Lua 5.1 lbaselib.c luaB_newproxy accepts nil
and validates a reused metatable against the native closure's weak-table upvalue. The
landed implementation rejects nil and accepts unmetatabled/foreign userdata instead.
Use the existing weak tables and native-closure API, with no new VM state or security model.
Source-only: all previous check/test grants are consumed. No test cases or runs authorized.

Second narrow claim: benilla-ui/src/script/wrath_security.rs only, clean after 2ff9814e.
issecure currently answers true under every tagged addon/macro call, while the existing
taint::current owner already distinguishes these contexts for protected actions. Wire the
query to that owner without adding context state, policy, taint propagation or a sandbox.
RestrictedFrames.lua:100 gates handle creation on this query and ActionButton.lua gates
show-grid changes on it. This is an ordinary reachable wrong answer, not hypothetical
race/security hardening. Existing taint.rs limitations and securecall remain open.

Continuation claim after 19b65b9d: Astra takes script/tooltip_item/mod.rs only for Wrath
spell hyperlink admission, plus this checkpoint and the existing first-loop inventory.
File is clean; OC's chat, model, layout, region and bootstrap edits remain excluded.
Installed WM_MulticlassSpellbook.lua:33 uses GameTooltip:SetHyperlink("spell:"..id), but
the current handler admits only item/enchant links and silently returns for this path.
Use the existing tooltip_spell::set_spell_by_id renderer/cache/clear owner. No catalog,
mock tooltip, new test case or verification execution is introduced. Installed spellbook
SHA-256: FDD215FF216F05E388BE94D8348CD7BE11026A4B664709A1EAAD62F2AB45381E.

Item-query continuation claim: script/item_stats.rs, new production child item_stats/wrath.rs,
and the already claimed tooltip_item/mod.rs. Current files are clean at b4ba2d25. The installed
WM_SpellmakingAdvanced.lua:518 reads icon at GetItemInfo result 10; Bagnon's
LibItemSearch-1.0.lua:203 reads item level at 4. The shared binding still returns donor 9
values and rejects escaped item links. Add a Wrath adapter using the same item_templates and
item_stat_reads owners, preserving donor registration/tuples. Decode the nine-field 12340
link from the pinned AC HyperlinkTags.cpp::LinkTags::item::StoreTo: permanent enchant, three
gem enchants, reserved word, signed property, suffix factor, render level. Never read gem 1
as a donor property. Keep negative-suffix scaling explicitly open; no data owner is invented.
No builds/tests/new cases or runtime actions are authorized by this source claim.

Follow-through claim: clean script/craft.rs, script/tradeskill/api.rs and script/quest_log.rs,
plus the already claimed item_stats files. Their template-only link producers still emit
four-field donor payloads in Wrath mode. Use one target-aware template-link helper so the
Wrath parser and these producers agree. The app's live-object link producer under ui_items
is a separate OC integration dependency: it also emits donor fields and needs real socket
enchant ids from the item owner. Do not reinterpret arbitrary short Lua links to hide that gap.

Live-container continuation claim: Astra takes only clean benilla-app/src/ui_items/mod.rs and
ui_items/feed.rs for the item-link presentation dependency named in the preceding handoff.
No native protocol/session/field mapper edit. At f7de0bd8, wrath.rs::push_item_fields copies
the first 21 enchant dwords intact, including the permanent and socket slots 2..4; property
and seed are separately mapped to donor accessors. The current container feed has script.target()
and the observed player level. Adapt links at that presentation boundary using those facts;
preserve the donor resolve_slot helper/test assertions and all bag state/event ownership.
This narrows the previous OC handoff's live-bag dependency only; equipment/inspect/trade/loot
producer migration, negative-suffix scaling and the native owner's dropped enchant slots stay open.
Source-only, with no new test cases or execution grant inferred.

Own-equipment continuation claim: clean benilla-app/src/ui_char.rs and the already claimed
benilla-ui/src/script/item_stats.rs. feed_char currently supplies donor object links for all
own equipment, equipped bag items and bank-bag items. Adapt only the completed presentation
snapshots using script.target(), observed level, guid and the existing object-link formatter.
The ammo template has no item object; expose the existing UI target-aware template formatter
through UiScript rather than creating fake object fields. Donor builders/assertions remain
unchanged. Inspect/trade/loot producers and native item authority are outside this claim.

Next concrete addon claim after 89b1a9fe: script/spellbook.rs, script/wrath_spell.rs,
script/host.rs, app/script_calls.rs and app/ui_script/wrath_queries.rs, all currently clean.
Installed WM_SpellmakingV3.lua:296 calls GetSpellCooldown(id) without a book argument; the
current donor binding rejects it through spell_slot_args. Add the Wrath ID/name query form
through the existing lent host and Cooldowns owner. Retain book-slot/pet query behavior by
reusing the original binding body; do not create timers in Lua or infer zero cooldown from
an absent host/world/catalog. Use the same UiClock projection as the spellbook feed.
No new tests, builds, runtime checks or speculative race/security mechanism is authorized.

2026-10-28 continuation claim at 85d74015: clean script/class_names.rs,
script/tooltip_item/render.rs and script/tooltip_item/names.rs. The renderer's CLASS_NAMES
omits class 6, so a Death Knight-only item prints no class requirement and a donor all-class
mask hides the Wrath exclusion. Use the existing locale-bound class-label projection for
the ten playable Wrath IDs (AzerothCore SharedDefines.h CLASSMASK_ALL_PLAYABLE), preserving
donor labels and masks. Missing required observed labels must remain unavailable, not an
English fallback. Also select the established heirloom color in the cached tooltip title;
this does not claim a full Wrath tooltip line-law port. No new state, tests or execution.

Heirloom follow-through claim: clean script/tooltip_item/mod.rs only. Its six InFlight::Name
producers still use the donor palette before the cached renderer runs. Delegate those colors
to the already added target-aware helper after their existing Model borrows end; preserve
the existing name-only white override and cache-wait lifecycle. No new event, state or run.

2026-10-28 race-label continuation claim: clean script/class_names.rs, script/client.rs,
script/tooltip_item/render.rs and app/chr_classes.rs. ChrRaces::localized_labels already
retains all nine Wrath locale columns; the renderer still assumes eight donor races and
omits Blood Elf/Draenei-only restrictions. Seat playable-race labels through the existing
production/from-chain metadata entry points. A typed VM host-data read projection avoids
editing OC's dirty Model/module files; the existing set_locale owner retires it on changes.
No new catalog, protocol owner, English fallback, test case or execution is introduced.

2026-10-28 tooltip identity claim at 2b2abfc1: clean tooltip_item/mod.rs and
tooltip_item/build.rs. Installed WM_MulticlassHunt.lua:100-101 reads GetItem() from its
OnTooltipSetItem callback. The getter currently ignores the rolled instance name and
returns no link for several setters despite available source-view links. Preserve links
from loot/roll/auction/trade and held item views, including cache rebuilds. For an actual
template-only build, reuse the existing target-aware template formatter. Never synthesize
missing instance/gem/property fields. No native producer/format migration, tests or runs.

2026-10-28 tooltip subject claim at 7a73dc9c: clean widget/kinds/mod.rs,
script/tooltip/mod.rs, script/tooltip_spell.rs and script/tooltip_unit.rs. Installed
Questie-335 Modules/Tooltips/Tooltip.lua:389-391 calls GetUnit/GetItem/GetSpell on every
tooltip update; TooltipHandler.lua:18 consumes the unit token from GetUnit. GetUnit and
GetSpell are absent. Retain the spell ID/observed name and unit setter token in the existing
TooltipState, retire them through the existing content-clear owner, and expose Wrath-only
getters. Unit identity must match the retained GUID rather than silently follow a retarget.
The existing unit renderer also lacks the OnTooltipSetUnit edge used by Questie's hook;
publish it after content is ready in Wrath mode. No aura-as-spell assumption, native state,
new registry, test case or execution is authorized. Donor methods/events stay unchanged.

2026-10-28 Bagnon continuation claim at b0a561ea: clean script/container.rs only.
GetContainerItemInfo currently returns five donor values, while installed Bagnon
utility/itemSlotInfo.lua:18 and utility/itemEvents.lua:87/118 read the seventh as the
slot link used for item identity/update messages. Stock 12340 ContainerFrame.lua:684
also reads that seventh value for a refund link. Append Wrath lootability/link through
the existing ItemTemplateView cache and ContainerSlot source links; preserve donor
arity and existing slot/lock/readable/quality semantics. Missing template flags must
remain unknown and use the existing item_stat_reads owner, not a second cache or a
guess from UseFork. No native ownership, new tests or execution is authorized.

2026-10-28 bag-space claim at 06d8886c: clean script/container.rs, script/host.rs,
app/script_calls.rs and app/ui_items/calls.rs. Installed Bagnon itemEvents.lua:179 needs
GetContainerNumFreeSlots and its family mask. Use the already lent ItemSteps owner to
read GUID-seat occupancy and Items::template(..., Ask::Read).bag_family. Do not count
only resolved UI item snapshots or introduce ContainerState fields/new registries.
Unavailable object/template facts remain unavailable; observed unequipped bags are empty.
Reuse current inventory capacities and bank admission, explicitly retaining the upstream
Wrath bank/keyring projection limitations instead of claiming those repaired. Donor API
registration stays unchanged. No new ECS parameter, test case or execution is authorized.

2026-10-28 item-count claim at 7ec2be19: clean UI script/item_stats/wrath.rs and
script/host.rs; app/script_calls.rs, ui_items/calls.rs and ui_items/mod.rs. Questie
TrackerLinePool.lua:559/628/631 requires GetItemCount(id,nil,true); installed Skillet
also uses item links and includeBank. Add the query through the existing ItemSteps host
and shared GUID traversal. Preserve donor count/action/quest callers' whole-item filter;
the new possession query must not exclude broken equipment (AC PlayerStorage.cpp's
GetItemCount counts owned copies without that gate). Use existing template/instance
charge facts for includeUses, with no timer, mutation, new cache or inventory owner.
Expose numeric/link and uniquely cached-name resolution; absent host/template facts
remain unavailable. Exact target charge-tail/name ABI and native bank/keyring capacity
remain explicitly unverified. No tests/new cases, checks or runtime execution authorized.

2026-10-28 random-property loader claim at 76d69b6a: clean formats/src/
item_random_properties.rs only. The existing loader requires the donor 16-column
layout, but the pinned AC DBCStructure.h ItemRandomPropertiesEntry / DBCfmt.h
ItemRandomPropertiesfmt define 24 columns for 12340: ID, internal name, five enchants,
sixteen localized names and a mask. entities.rs logs a load failure and omits the
resource on mismatch. Select this exact known layout by field count and the existing
Wrath locale-column owner, retaining strict parsing and the donor layout. No generic
trailing-field guess, new catalog/owner, negative-suffix scaling, tests or execution.

Dependent enchant claim at 82999308: clean formats/src/item_visuals.rs only.
SpellItemEnchantment loader likewise requires 24 donor fields, while pinned AC
DBCStructure.h/DBCfmt.h define 38 for 12340: charges at 1, names 14..29,
name mask 30, visual 31, flags 32, five requirement/source tail words. AC Item.cpp
uses the field-32 bit 1 for soulbinding. Select the exact known schema and offsets;
retain the donor parser and catalog interface. No render, native ownership, new
catalog, suffix scaling, tests or execution is included.

2026-10-28 negative-suffix catalog claim at b7b11b21: clean formats/src/
item_random_properties.rs and app/items.rs only. Retain the exact 29-field
ItemRandomSuffix table in the existing RandomPropertyCatalog when the loaded
positive-property table selects 12340. Keep positive get()/iter()/fixture behavior;
expose signed-name resolution and seed-scaled enchant amounts using AC's allocation
formula. Route the shared item-name formatter through that observed name. No new
resource/cache, parser fallback, guessed tooltip-token expansion, native field repair,
test case or execution. Negative suffix stat rendering/raw-link Lua queries remain open.

2026-10-28 raw-link suffix-name claim at 445f855f: clean UI script/host.rs,
script/item_stats/wrath.rs and app/script_calls.rs, ui_items/calls.rs. Add one
read-only optional RandomProperties access to existing ItemSteps and expose its signed
suffix-name lookup through the lent host. Wrath GetItemInfo negative-property links
must use that observed localized name and existing ITEM_SUFFIX_TEMPLATE, or remain
unavailable, rather than returning a known unsuffixed name. Keep positive/donor paths
and raw link instance fields unchanged. No new catalog/cache, packet, stat formatter,
tests or execution. New SystemParam admission remains unverified until granted.

2026-10-28 raw item-cooldown query claim at 3e6535c6: clean UI script/host.rs,
script/item_stats/wrath.rs and app/script_calls.rs, ui_script/wrath_queries.rs.
QuestieCompat.lua:201-205 delegates to missing GetItemCooldown(itemID), and its tracker
expects GetTime start/duration/enable. Read cached ItemInfo::use_spell and the existing
Cooldowns::info_item/ui_triple with UiClock through the existing query host. No new
resource/parameter, timer, query packet or change to OC's widget/recovery owners.
Keep item category overrides and held states; missing world/template/spell/clock facts
stay unavailable. Preserve donor registration and avoid any modern C_Container alias.
No new tests/cases, checks, imports or runtime execution are authorized.

2026-10-28 Wrath secondary-stat claim at 3acd2703: clean UI
script/tooltip_item/names.rs and render.rs. ItemTemplateView retains the wire stats,
but the renderer visits only donor IDs 0/1/3..7, dropping target IDs 12..39 and 41..48.
Use AC ItemTemplate.h ItemModType and pinned 12340 GlobalStrings.lua keys for those
values; do not invent type 40 or newer expansion stats. Emit the equip-stat block
through existing localized templates and green on-equip line presentation. Installed
zzzmopitemtooltips/core.lua replaces many numeric formats with %c%d; use the existing
strict formatter to support both forms without mistaking the sign for a numeric value.
Preserve donor/primary/resistance behavior, actual wire amounts and source order.
Exact native block ordering and complete addon/render fidelity remain unverified.
No new stat owner/rating calculations, tests, generators or execution authorized.

2026-10-28 keyring continuation claim at 18096c15: clean app/ui_items/mod.rs,
feed.rs, calls.rs, drain.rs and UI script/container.rs. Current wrath.rs copies all
32 keyring GUIDs (496 -> 648, 64 dwords) and wrath_item_pos translates donor slots
81..112 to server 86..117, matching AC Player.h's fixed 32-seat keyring. The loss is
in UI/item consumers: level-sized <=16 feed, GetContainerNumSlots rejects -2,
wire_pos rejects seats 17..32, and HasKey compares donor family 9 instead of Wrath
mask 0x100. Add target-aware helpers for these paths, raw free/count queries and
direct item actions, retaining existing donor entry points/tests and bank limits.
Stock 12340 GetKeyRingSize itself reads GetContainerNumSlots(-2) and sizes its view;
do not replace that Lua function. Native/protocol files stay untouched. No new test
cases, checks, generator or runtime execution; pending verification is not a grant.

2026-10-28 late-keyring consumer claim at 3104740a: clean app/ui_items/mod.rs;
ui_merchant/mod.rs and calls.rs; ui_trade/mod.rs and calls.rs; ui_action/drain.rs,
feed.rs and recount.rs. Pass the current UI target into merchant destination and
trade placement/re-signal mapping; action lookup/count/recount must use the same
32-seat keyring span. Preserve donor helper entry points and all existing non-keyring
scope/charge/whole-item rules. The trade-open held-item sender is also a production
caller with a current script target; no test helper/case/assertion needs editing.
Native/protocol/recovery
source stays untouched. Other gameplay/reagent helpers without a UI target remain
separate migration work. No tests, checks or runtime operation is authorized.

Trade dispatch follow-through: claim clean app/script_calls.rs as well; TradeSteps::apply
receives no UiScript, so the existing host must pass its current target explicitly.

2026-10-28 clamp regression claim: clean UI src/layout.rs. The authorized UI suite at
be001728 failed assemble_clamp_low_edge: the inset change subtracts the positive
low-edge displacement from the opposite edge, shrinking the frame. Correct both axes
to add that displacement. No test edits or rerun are authorized by the consumed pass.

2026-10-28 Questie item-query claim at 960e78fd: clean UI script/char_stats.rs,
script/item_stats/wrath.rs, script/host.rs and app/script_calls.rs,
ui_script/wrath_queries.rs. Questie TrackerLinePool.lua:529 and TrackerUtils.lua:351
call missing GetInventoryItemID, GetItemSpell and IsEquippableItem. Project existing
inventory slot entry, cached item inventory type and on-use spell catalog name/ID.
Preserve donor bindings, missing-data behavior and existing owners; no new cache,
packet, gameplay authority, tests or execution. Host query and target ABI remain
unverified pending a fresh grant.

2026-10-28 partial catalog admission claim at faf89e39: clean
`crates/benilla-formats/src/item_random_properties.rs` and
`crates/benilla-app/src/entities.rs`. A failure in the separate Wrath suffix DBC
currently discards successfully loaded positive properties. Retain the positive
rows and expose the suffix load error through the existing startup warning; unavailable
negative suffixes remain unavailable. No tests or execution are authorized.

2026-10-28 loot link claim at 348d1074: clean `crates/benilla-app/src/ui_items/mod.rs`
and `crates/benilla-app/src/ui_loot/mod.rs`. The Wrath loot window and item-receipt
chat currently emit vanilla-shaped links that misplace the roll in the Wrath parser.
Use the existing script target and observed fields; retain vanilla output and do not
alter protocol-owned loot data. No builds, tests or new cases are authorized.

2026-10-28 group-loot link claim at 74e4aa29: clean `crates/benilla-app/src/ui_loot_roll.rs`.
Convert the group-loot snapshot and announcement links using the shared target-aware
formatter and the active script target, retaining only values the existing owner holds.
No protocol, tests or runtime execution are in scope.

2026-10-28 trade-link claim at 217913b3: clean `crates/benilla-app/src/ui_trade/mod.rs`.
Carry the active script target through both trade snapshots and item-watch refreshes,
using the existing target-aware link formatter for cached and uncached rows. Existing
trade data, ownership and protocol remain unchanged; no tests or execution authorized.

2026-10-28 inspect-link claim at f406db32: clean `crates/benilla-app/src/ui_inspect.rs`
and `crates/benilla-app/src/ui_tooltip/mod.rs`. Pass the active script target through
the inspect feed and fresh-hover paths to format visible-item links. Only the existing
visible-item fields are available; no descriptor or protocol changes are claimed.
No test cases or execution authorized.

2026-10-28 auction-link claim at 87e879cc: clean `crates/benilla-app/src/ui_auction/mod.rs`
and `crates/benilla-app/src/ui_auction/net.rs`. Pass the active UI target through
the existing list readers for normal refreshes and lane page updates; use the existing
target-aware link formatter with the auction row's property and factor. No wire decoder,
test case or execution changes are authorized.

2026-10-28 quest-panel link claim at 59413efe: clean `crates/benilla-app/src/ui_quest/mod.rs`.
Select the target layout for quest-panel required and reward item links using the
existing script target and template; preserve the vanilla snapshot used by existing
fixtures. Quest state, reward commands, protocol and tests remain outside this claim.

2026-10-28 quest reward chat claim at f4ea0621: clean
`crates/benilla-app/src/ui_quest/lines.rs`. Pass the active UI target to the existing
reward line resolver so its item link uses the shared target-aware formatter. No quest
state, protocol, test case or runtime execution is included.

2026-10-28 merchant-link claim at cb2e1ae7: clean
`crates/benilla-app/src/ui_merchant/mod.rs`. Carry the script target to vendor-list
item link construction, retaining existing vanilla fixtures. Buyback and protocol
behavior are separate; no tests or execution authorized.

2026-10-28 buyback link claim at 9bf22656: clean
`crates/benilla-app/src/ui_merchant/mod.rs` and `crates/benilla-ui/src/script/merchant.rs`.
Installed aux-addon/core/tooltip.lua:295 calls missing GetBuybackItemLink(slot).
Expose the existing streamed buyback object's target-shaped link through the merchant
row and a Wrath-only Lua binding. Existing catalog and object owners supply facts;
vanilla behavior remains unchanged. No runtime, test or protocol work authorized.

2026-10-28 buyback name follow-through at bc61a31f: clean
`crates/benilla-app/src/ui_merchant/mod.rs`. The Wrath buyback link uses the
object's rolled name while the row still uses its base template name. Reuse the
existing object name formatter for the Wrath row; retain vanilla behavior. No
tests or execution authorized.

2026-10-28 Bagnon bag-family claim at 6ae6fb10: clean
`crates/benilla-ui/src/script/item_stats/wrath.rs`, `script/host.rs`,
`crates/benilla-app/src/script_calls.rs` and `ui_items/calls.rs`. Installed Bagnon
bagSlotInfo.lua calls missing GetItemFamily(link) for specialized bag classification.
Expose the existing cached template's 12340 bag-family mask through the current
host seam. Missing templates remain unavailable; no new cache, tests or execution.

2026-10-28 aux durability claim at d7db8c75: clean
`crates/benilla-ui/src/script/container.rs`. Installed aux-addon/util/info.lua calls
GetContainerItemDurability for auctionability. Expose the existing observed bag-slot
durability pair through a Wrath-only binding; no object or protocol changes, test
cases or execution are authorized.

2026-10-28 bare tooltip roll-name claim at aaf29660: clean
`crates/benilla-ui/src/script/tooltip_item/build.rs` and `item_stats/wrath.rs`.
Installed aux passes bare `item:` strings to SetHyperlink; the existing builder uses
the base template name when no display name is embedded. Resolve the observed Wrath
property via the existing item-name owner at build/rebuild time. No guessed scaled
enchant formatting, protocol changes, test cases or execution are authorized.

2026-10-28 cached item-name claim at c8075c08: clean
`crates/benilla-ui/src/script/item_stats/wrath.rs`. The shared Wrath item query
accepts IDs and links while the existing GetItemCount path also accepts uniquely
cached exact names. Reuse that same rule for item queries without inventing an ID
for ambiguous or uncached names. No new cache, tests or execution authorized.

2026-10-28 bounded verification follow-up claim at 3e97bc4d: documentation only:
this checkpoint, `../handoffs/2026-10-28-astra-session-handoff.md`,
`../handoffs/2026-10-27-astra-wrath-item-query-consumer.md`,
`../compatibility/first-loop-behaviors.md`, and
`../verification/2026-10-28-astra-bounded/summary.md`. No source files claimed.
Tracked worktree/index were clean before and after the pinned pass; OC's animation,
protocol/native lanes and all other worktrees remain untouched. Existing untracked
logs and procgen helper preserved. Record the consumed grant and exact results;
no rerun or new test is authorized by this claim.

2026-10-28 group-loot factor claim at d1dc10d6: clean
`crates/benilla-app/src/ui_loot_roll.rs`. The existing `LootStartRoll` carries a
suffix factor that `ActiveRoll` discards. Retain it and place it in Wrath snapshot
links; preserve the vanilla link behavior. The Wrath packet path and tests remain
outside this claim, and no execution is authorized.

2026-10-28 WP3 consumer claim at 7fe6f5e4: clean
`crates/benilla-app/src/ui_loot/mod.rs` and
`crates/benilla-ui/src/script/loot.rs`, plus this checkpoint and the existing item
consumer handoff. OC's shared LootItem now retains the factor. Carry it into LootRow
and the existing Wrath link projection, including delayed-template snapshots. Keep
the donor formatter behavior and coin rows unchanged. Existing literal additions
only; no new tests, execution, protocol/adapter edits or OC animation changes.
Mechanical constructor follow-through also claims the existing LootRow literal in
`crates/benilla-ui/src/script/tests/tooltip_item/mechanics.rs`; no assertion changes.
The same mechanical field propagation claims existing LootRow literals in
`crates/benilla-app/src/ui_script/{loot_tests,game_menu_tests,escape_tests}.rs`.

2026-10-28 delivery-planning claim at 2cefd8a5: this checkpoint and new
`../work-packages/2026-10-28-private-alpha-delivery-plan.md` only. User requested
a detailed design/architecture/reuse plan and confirmed private Windows alpha,
existing local AzerothCore, stock UI first, installed addons staged afterward.
Source/document reads and public primary-source research only; no implementation,
subagents, new runtime verification or changes to existing issue acceptance/ADR status.

2026-10-29 map schema claim at 785fa7a8: `crates/benilla-formats/src/world_map_area.rs`
and this checkpoint. Owner authorized necessary checks, tests and bounded diagnostics.
OC's dirty UiClock extract and alpha checkpoint remain untouched. Establish the
observed 11-column schema failure with an existing-module fixture, preserve the
8-column donor format, and retain unknown-column refusal.
The bounded real-profile run passed WorldMapArea admission and exposed a second
14-column WorldMapContinent mismatch. Also claim
`crates/benilla-formats/src/world_map_continent.rs` for its exact 12340 layout and
focused regression; preserve its existing 13-column donor layout.

2026-10-29 follow-up evidence: synthetic 11-column WorldMapArea failed before the
repair and passes afterward; selected real-profile client then progressed beyond
that error to a 14-column WorldMapContinent mismatch. The 12340 WorldMapContinent
definition in WoWDBDefs identifies a trailing WorldMapID; explicit 14-column
admission now passes its focused fixture while unknown shapes remain refused.
`cargo test --locked --offline -p benilla-formats --lib world_map_` reports five
passed, including two synthetic cases; real-data donor cases may skip without the
vanilla data gate. `cargo check --locked --offline -p benilla-app --lib` passed
with OC's dirty UiClock work present. A full client build passed after the area
change but before the continent change; the newer continent code has not been
built into the client or run with the real profile.

The one 90-second maximum real-profile run with a 12 GiB sampled private-memory
threshold terminated at 14.4 GiB after rapid growth across the sampling interval.
Evidence: `../verification/2026-10-28-oc-alpha/astra-bounded-20261029-005649-312.memory.log`
and corresponding connected-run logs. No process remained. Memory root cause,
map window/API, model identity, freeze and actual route remain unproven. OC's
pre-run binary was preserved in the same untracked evidence folder as
`oc-pre-astra-benilla.exe`; OC's dirty source/checkpoint remain untouched.

2026-10-29 memory diagnosis claim: `crates/benilla-app/src/perf/journal.rs`
for temporary, opt-in image-residency instrumentation, this checkpoint for the
evidence. Owner authorized execution. Preserve OC's dirty files. Remove temporary
instrumentation after diagnosis; retain logs, and do not claim images cause private
memory growth without a measurement that distinguishes them.

The 5 GiB bounded FPS-journal run `astra-profile-20261029-010306-773` stopped at
5.5 GiB private; last journal sample had 1,520 entities, 315 images and 112 M2s.
The `astra-churn-20261029-010357-187` comparison stopped at 5.01 GiB; the optional
asset-churn switch emitted no churn rows in that build. A temporary opt-in journal
probe built into the client measured registered image GPU descriptor sizes:
`astra-imagebytes-20261029-010701-496` recorded 90.5 MiB then 151.2 MiB, with
321 images and 112 M2s at the last journal sample. Private memory later crossed
5 GiB and was stopped at 5.75 GiB. The main-side image data is often moved to the
render world; these sizes do not measure transient decode, driver, M2, Lua or all
process allocations. They rule out only the registered image descriptor total as
an adequate explanation. Temporary source probe removed after capture; binary
still contains it, inert without WOW_MEMORY_ASSET_TRACE. The same run did not
emit either old WorldMapArea or WorldMapContinent DBC mismatch before termination;
the complete map window remains unaccepted. The memory cause, player model,
UI behavior and A0/A1 remain unresolved. No client process remained.
