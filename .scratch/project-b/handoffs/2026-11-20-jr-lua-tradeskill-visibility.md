# Lua tradeskill linking and UI visibility

Status: ready to merge
Owner: Claude Opus 5.5
Branch: jr/lua-tradeskill-visibility
Issue: none
Next: Read CI for the pushed branch, then land after green

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
- CI: not run yet; push this commit and read the branch run.

Residual risk: no local compile or live FrameXML acceptance was performed. The owner should wire the
future network trade-list producer to `set_trade_skill_link` when that packet path is implemented.
