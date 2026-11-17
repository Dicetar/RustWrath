# Gemini: four Wrath API defects

Implement exactly these four fixes in D:/Projects/project-b-client, branch wotlk. Read AGENTS.md, docs/METHOD.md, docs/agents/12340-reference.md, docs/agents/active-ownership.md first. HEAD at preparation: 8807107f. Recheck status/diffs before edits and preserve all existing changes. Reference D:/Projects/wowdecomp-reference, pinned e67b6ffef23655dc7da061cb2d36b7963f6ef71a: inspect behavior, never copy source. This is a prepared prompt, not evidence that Gemini has run.

## Coordination

OpenCode owns app cvars/table.rs, UI script/cvars.rs and CVar tests, fixing missing Sound_EnableMusic/textureFilteringMode, wrong defaults, and version gating. Do not touch those files. Your ownership: UI script/glue_select.rs, app ui_script/glue_mgr.rs, UI script/tooltip_item/ and wrath_natives.rs, and directly associated existing focused tests. Ask Astra before expanding scope. Parent owns all Cargo and client runs: no builds/tests/client launch until a shared slot is explicitly assigned. No commits/pushes, new agents, destructive git, server/DB/install/config/credential edits. Human enters credentials and drives combat. Do not repeat OC's completed cursor implementation; repair its remaining contract defect.

Latest successful binary SHA256 D3A611C044A4D601D07F92649EAFBC3CFE27968C845A3FCBD53146C7AAFCFBC6. Latest log benilla-config/Logs/Launcher/20261101-234026-676/stderr.log proves realm-split server reply, but not correct Lua event arguments or visible workflow completion. Keep source fixes, executed tests, and runtime demonstrations separate.

## G-1 [medium, source confirmed]: SetRealmSplitState narrows through i32

Location crates/benilla-ui/src/script/glue_select.rs around475: number_arg(...) as i64 as u32. number_arg already coerces to i32; casting afterward cannot recover the value. Reference WoW/Source/Glue/GlueScriptEvents_Scripts.cpp Script_SetRealmSplitState, OFFSET0x004DE250, converts Lua numeric input through int64 then uint32 and sends only if final value <=2. Use the existing float_arg/numeric-string coercion convention, then explicit intended truncation/conversion and state gating. Inspect binding_abi.rs and reference edge behavior; do not blindly assume Rust saturating casts equal native conversion for nonfinite/out-of-range values. Preserve usage errors and Wrath-only registration. Query API must still send u32::MAX.

Acceptance cases to propose: states1/2; accepted numeric strings; invalid string usage error; rejected ordinary3; fractional truncation; a representable large value such as4294967297 that wraps to1 after int64->u32 and exposes the current i32 narrowing defect. Verify request queue contents without network/login automation. Keep cases bounded.

## G-2 [medium, source confirmed]: realm-split event loses signed projection

Location crates/benilla-app/src/ui_script/glue_mgr.rs around2038: SERVER_SPLIT_NOTICE emits f64::from(msg.choice) and f64::from(msg.state). Reference Glue/CGlueMgr.cpp::OnRealmSplitMsg (0x004D8200) reads two u32 fields then signals "%d%d%s". Engine/Source/FrameScript/FrameScript.cpp:1554 consumes %d as int32_t (distinct from unsigned conversion). Consequently wire0xFFFFFFFF must reach Lua as -1, not4294967295. Latest log line35 demonstrates the real server returns this first-field sentinel.

Keep parser and transport fields u32; reinterpret at the Lua event boundary as i32 before converting to f64. Check both numeric fields against reference. First field is realm-id echo, currently named choice: explain the naming issue, but do not spread a rename across network/protocol owners without agreement. Preserve field order/date/event timing and Wrath-only event registration. Acceptance: boundary projection gives(-1,0,"01/01/01") for the observed reply; positive values remain unchanged. Actual server receipt already works: do not replace it with local synthetic success.

## G-3 [high functionality gap, source confirmed]: comparison tooltip is an unconditional nil stub

Location crates/benilla-ui/src/script/tooltip_item/mod.rs:527-533, SetHyperlinkCompareItem ignores every argument and always returns nil. Thus a valid hyperlink can never display equipped comparison content. Consult the exact registered method and CGTooltip comparison helpers in the pinned reference; the prior read-only audit identified wrapper0x00601D70 and comparison routines0x00631B60/0x00631850. Resolve symbol/address correspondence before implementation rather than trusting addresses alone.

Implement through existing tooltip item machinery and tooltip_item/compare.rs. Validate the hyperlink argument with reference-compatible usage behavior; find the current player, parse/resolve item, derive compatible equipment slots, select worn item using offset and dual-wield rules, render equipped-item tooltip and return1 on success, nil for genuine misses. Optional offset defaults0 and comes from trunc(arg3)-1 when supplied; inspect optional shift/merchant scaling branches before choosing behavior. The existing merchant/auction comparison helper has a class filter that this native method does not: do not accidentally import that filter. Honor the equipped header/latch, reset stale tooltip state appropriately, and use real item data. Do not create blank successful tooltips or silently claim unsupported branches implemented.

Acceptance: valid link+compatible worn item yields populated comparison and1; missing player/item/slot yields nil; wrong argument type errors as reference; offset selects correct ring/weapon counterpart. OC adds alwaysCompareItems default0 separately. Prefer extending existing tooltip tests, not a new end-to-end harness. Live chat-link comparison remains a separate acceptance step.

## G-4 [medium, source confirmed]: GetCursorInfo item arms return nil hyperlink

Location crates/benilla-ui/src/script/wrath_natives.rs::cursor_info, lines29-35 and67-73. CursorPayload::Item emits nil when cached link absent; CursorPayload::Action always emits nil. Reference Ui/GameUI_Scripts.cpp::Script_GetCursorInfo (around2829) resolves item data and returns an item hyperlink string; failed resolution returns zero values, not an apparently valid item triple containing nil.

Use the existing item-template/model lookup and item_stats::item_link helper (verify signature and quality/name encoding), preserving a valid existing full instance link when available. Resolve action item IDs through authoritative model data. If required item metadata is unavailable, match the reference's zero-return behavior rather than fabricate a blank link. Check reference distinction between resolved instance and template link; preserve enchant/random-property information when present. Keep money/spell/merchant/macro/pet branches unchanged unless a directly demonstrated regression requires repair.

Acceptance: resolved item/action item gives("item",id,string hyperlink); unresolved data follows reference zero returns; existing six wrath_natives tests remain meaningful (update any expectation that enshrines nil). Use a few focused cases. Runtime bag drag/drop must be checked separately without deleting an owner's item.

## Out of scope and completion

Kobold Camp Cleanup row/Accept behavior is observed but not root-caused. Do not relax packet parsers, clear response latches, or add broad API stubs on speculation. Logout guard, roster switching, creation/cancel and owner-driven Firna route still need visible acceptance, but are not extra fixes in this assignment.

Return a concise report: files changed, each defect's causal repair and reference anchor, unimplemented branches, exact proposed targeted checks, and any coordination requests. Source changes are not runtime success. Wait for Astra to coordinate the single integration build.
