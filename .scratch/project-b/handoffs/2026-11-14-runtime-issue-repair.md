# Handoff: 2026-11-14 runtime issue repair

## Status

The merged nameplate CVar repair remains at `f1a8693154c8a50a8d6d75a6286e3a9ae65d6e0c` on `main`. This handoff records the follow-up runtime investigation and the first repaired UI runtime defect.

## Corrected startup diagnosis

The recorded client session did not establish a permanent startup deadlock. The log reached roster receipt, world entry, and player control:

- `wrath: entered world 0, player 0x1522, 87 entities`
- FrameXML loading began after world entry.
- `ChatFrame.xml` completed at `2026-11-14T12:30:11.287588Z`.
- `FRIENDS.TTF` loaded at `2026-11-14T12:30:11.300911Z`.
- `FloatingChatFrame.xml` completed at `2026-11-14T12:32:44.486073Z`.

The measured gap between the ChatFrame/font lines and FloatingChatFrame completion was `153.185s`. Loading continued afterward through the stock player, party, target, pet, character, quest, merchant, bag, loot, friends, channel, pet bar, and multicast action-bar files. The evidence therefore supports a long synchronous loading stall around `FloatingChatFrame.xml`, not a proven permanent hang. No startup-performance change should be made until a deterministic reproduction or before/after comparison exists.

## First repaired issue: GameTimeFrame error

The launcher report recorded:

`ui_script(FrameXML/Interface\\FrameXML\\GameTime.xml): GameTimeFrame: OnLoad: runtime error: Interface\\FrameXML\\GameTime.lua:141: attempt to index a nil value`

The pinned installed Wrath source shows line 141 is:

`self:GetFontString():SetDrawLayer("BACKGROUND")`

The nil value was the button's implicit `ButtonText` FontString, not the calendar snapshot or `GetGameTime()` value. The RustWrath host already provides `GetGameTime`, `CalendarGetDate`, and `CalendarGetNumPendingInvites`; those bindings are covered by focused tests.

The loader's Wrath button path incorrectly relied on `SetText("")` to create the label. In the Wrath binding, an empty `SetText` returns early when no label exists. Stock XML still creates an unnamed `<ButtonText/>` before the button's OnLoad, so `GetFontString()` must work even when the label has no name or text.

Production fix:

- `crates/benilla-ui/src/loader/widgets.rs` now explicitly creates and adopts the button's FontString for every `<ButtonText>`/`<NormalText>` child before applying its text. This preserves named labels and creates the unnamed implicit label required by Wrath stock UI.

Regression test:

- `crates/benilla-ui/src/loader/tests.rs`
- `unnamed_button_text_is_available_during_onload`

The test loads the button in the Wrath-target VM, calls `this:GetFontString()` during OnLoad, and executes the exact `SetDrawLayer("BACKGROUND")` operation from `GameTime.lua:141`.

## Verification

Focused command:

`cargo test -p benilla-ui unnamed_button_text_is_available_during_onload --lib`

Result after the production fix: 1 passed, 0 failed. The same test was red before the fix in the Wrath-target VM, reproducing the nil-label defect. Existing button-font coverage also passes:

`cargo test -p benilla-ui button_text_reads_its_own_font_and_inherits --lib`

Result: 1 passed, 0 failed. Existing compiler warnings remain unrelated to this repair.

## Follow-up verification

The production change was rechecked against the existing named-label and alignment contracts:

- `cargo test -p benilla-ui a_button_normal_text_is_the_label_names_it_and_fonts_it --lib` — 1 passed, 0 failed.
- `cargo test -p benilla-ui a_button_label_element_does_not_own_its_justify --lib` — 1 passed, 0 failed.

These checks confirm that explicit FontString creation does not break named `NormalText` publication, state-font inheritance, or label alignment. A rebuilt client and owner-driven runtime session are still required before declaring the launcher error cleared in visible gameplay.

## 2026-11-14 continuation inspection

The current worktree still contains only the intentional GameTime loader/test edits plus preserved untracked `.scratch/project-b` evidence. `git diff --check` is clean, and the checkout's `.cargo/config.toml` selects `sccache` as the Rust compiler wrapper. `sccache --show-stats` reports zero compile requests for this iteration; no new Cargo check/test was started.

A live-process check found no `benilla`, `cargo`, `rustc`, or `sccache` process. The previous historical client reservation is therefore not evidence of a current client, but the slot remains owned by Astra in the ownership record and must be coordinated before any launch. No client, login, server, or game-data operation was performed.

`VoiceChatSpeakerTemplate` was traced to the skipped stock `VoiceChat.xml` virtual Button template. Its only observed evidence is the repeated warning while loading PlayerFrame, PartyFrame, and ChannelFrame; voice chat is intentionally disabled and the host already supplies no-op voice verbs plus `IsVoiceChatEnabled() == false`. Because no Northshire-route failure or runtime error is tied to this warning, no speculative template shim was added. The next action is owner/runtime coordination for rebuilding and verifying the current GameTime fix; known-issue repair remains blocked at that acceptance boundary.

The authorized focused regression was rerun with `CARGO_INCREMENTAL=0 cargo test -p benilla-ui unnamed_button_text_is_available_during_onload --lib`: 1 passed, 0 failed. The run compiled `benilla-ui` in 3m38s and emitted the existing 14 warnings; no test failure occurred. `sccache --show-stats` still reported zero compile requests after the run, so cache use is not claimed despite the configured wrapper and this should be treated as an environment/tooling discrepancy if another Cargo run is authorized.

The current source-only continuation traced the next evidence-backed GlueXML issue: `CharacterCreate.xml` contains a hidden class-label FontString inheriting `QuestFontNormalHuge`, while the Wrath Glue font set defines the corresponding style as `GlueFontNormalHuge`; neither the pinned Wrath font files nor the glue files define `QuestFontNormalHuge`. The loader correctly reports the missing inherited node, so the repair belongs at the Wrath Glue compatibility boundary rather than in generic inheritance handling.

Production source change, not rebuilt yet:

- `crates/benilla-app/src/ui_script/glue.rs` now installs a Wrath-only `QuestFontNormalHuge` font-object alias immediately before loading `CharacterCreate.xml` in the GlueXML TOC.
- The alias creates `QuestFontNormalHuge` and links it to the existing `GlueFontNormalHuge`, preserving the stock localized font/style object rather than inventing font metrics.
- Glue files are loaded one TOC row at a time so this compatibility alias is sequenced after `GlueFontStyles.xml` and before `CharacterCreate.xml`; the generic loader and the pinned reference files remain unchanged.
- The stock glue-load test now asserts that the alias exists and that the load errors do not contain the missing-font warning.

No Cargo command, rebuild, client, login, server, or game-data operation was performed after these source changes. The new source assertions are therefore unverified until a focused Cargo check/test is authorized; runtime acceptance remains owner-driven and separate.

The next evidence-backed GlueXML warning is `CharacterCreateTooltip2: Unknown frame strata: TOOLTIP2`. The pinned Wrath `CharacterCreate.xml` explicitly uses `frameStrata="TOOLTIP2"`, while the client’s strata table exposes only the topmost `TOOLTIP` bucket. The loader now maps `TOOLTIP2` to `TOOLTIP` only for Wrath XML loading, preserving the strict Lua `SetFrameStrata("TOOLTIP2")` error contract. A focused loader regression `wrath_xml_tooltip2_uses_the_tooltip_stratum` was added, but has not been compiled or run.

The separate `Couldn't find relative frame: CharCreateClassButton6` warning was inspected and not repaired: the stock XML declares buttons in an unusual order and references button 6 before its later declaration, so this is a loader-order/deferred-anchor behavior question rather than evidence for inventing a frame or suppressing a warning. No source change was made for it.

## REA continuation: CharacterCreate warning routing

The project REA/12340 reference workflow was applied to the remaining CharacterCreate warnings. The preserved reference is `D:/Projects/wowdecomp-reference` at pin `e67b6ffef23655dc7da061cb2d36b7963f6ef71a`; inspection remained read-only.

- `Engine/Source/Frame/CLayoutFrame.cpp:1423-1447` confirms the original XML loader resolves `relativeTo` immediately and reports `Couldn't find relative frame` when the target is not registered yet.
- The pinned stock Wrath `CharacterCreate.xml` declares `CharCreateClassButton11` before `CharCreateClassButton6`, and button 11 anchors to button 6. The warning is therefore faithful to the source layout/order and is not evidence for inventing a frame or globally changing anchor semantics.
- The Rust loader already has deferred-anchor behavior for targets within the enclosing subtree; changing the generic drain/order contract to silence this one stock warning would risk changing established XML behavior. No source change was made for `CharCreateClassButton6`.
- `Frame-Sides.blp` exists in the local extracted reference directory but is empty, so the missing-texture warning is an asset-availability issue rather than a proven loader defect. No game/reference asset was copied or modified.

The Wrath-only `TOOLTIP2` to `TOOLTIP` mapping and `wrath_xml_tooltip2_uses_the_tooltip_stratum` regression remain source-only and unverified. No Cargo command, build, client, login, server, or game-data operation was performed.

## Remaining known issues
- `VoiceChatSpeakerTemplate` remains an explicit missing template warning in PlayerFrame, PartyFrame, and ChannelFrame.
- GlueXML missing textures, portrait race-100 fallback, and flight-recorder quest/cast/loot/vendor anomalies remain open.
- Visible gameplay is not established by these focused tests.
- The `FloatingChatFrame.xml` delay remains an observed runtime performance issue, not yet a minimized red-capable test.

## Next priority

Rebuild and run the client through the authorized owner-driven session before claiming the GameTime error is gone at runtime. Then use route relevance and deterministic evidence rules before selecting another repair.
