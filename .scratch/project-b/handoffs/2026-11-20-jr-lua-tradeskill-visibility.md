# Lua tradeskill linking and UI visibility

Status: landed
Owner: Claude Opus 5.5
Branch: jr/lua-tradeskill-visibility
Issue: none
Next: none; landed in main

Reference: Wrath 3.3.5a build 12340 decompilation at e67b6ffef23655dc7da061cb2d36b7963f6ef71a.

Implemented:
- Added `IsTradeSkillLinked`: returns `(1, player_name)` only when host-supplied linked-list metadata
  exists, otherwise the reference-compatible `(nil, nil)` pair.
- Added `GetTradeSkillListLink`: returns the host-supplied serialized trade-list link or nil.
- Added `UiScript::set_trade_skill_link`, the explicit producer hook for a future remote trade-list
  packet to provide `(player name, serialized link)` metadata.
- Added a focused UI test covering both unsupported/no-metadata behavior and populated metadata.

Unsupported branches named:
- Current Project B app trade-skill producers only build local recipe snapshots; no network trade-list
  producer currently supplies the remote player GUID/name or serialized list link. Until one calls
  `set_trade_skill_link`, the two new APIs correctly return nil rather than fabricated data.
- `SetUIVisibility` already stores the reference `CGGameUI::m_visible` equivalent in `Model.ui_visible`
  and exposes it through `UiScript::ui_visible()`. The world renderer has no consumer of this flag,
  so world-side Alt-Z hiding of unit highlights/interact icons remains unsupported and is explicitly
  not claimed by this ticket.

Verification:
- `git diff --check`: passed before handoff creation.
- Local Cargo/client/server: not run, per owner instruction.
- Initial CI run `44652834737`: Linux rustfmt failed; Linux live-code-check passed; Windows check passed while test-build was still running. Formatting correction was pushed as `5a63bd1e`.
- Landed through `d8d4ee20`; `89f5dad4` removed the later nil stubs that overwrote the new host-backed natives. The original focused test read nil as `i64`; the corrected test uses an optional integer shape. Without host metadata, GetTradeSkillListLink remains explicitly unsupported because the own-list `|Htrade:` bitmap is not encoded.

Residual risk: no local compile or live FrameXML acceptance was performed. The owner should wire the
future network trade-list producer to `set_trade_skill_link` when that packet path is implemented.
