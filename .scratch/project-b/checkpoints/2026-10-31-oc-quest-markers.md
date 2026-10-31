# OC - quest marker status selection

Claim before source editing: `crates/benilla-app/src/quest_markers/mod.rs`. Baseline
`d1b54f50` (Astra's v264 particle package); the owner's dirty `AGENTS.md`, `CONTEXT.md`,
`docs/CONTRIBUTING.md`, `docs/METHOD.md`, `docs/agents/12340-reference.md` and handoffs are
preserved. No server, database, WM, install or combat state was changed.

Reference: `D:/Projects/wowdecomp-reference`, pin
`e67b6ffef23655dc7da061cb2d36b7963f6ef71a`. `WoW/Source/Object/ObjectClient/Object_C.cpp`:
`s_interactIcons` at `0x00ADAC98` (12 file slots) and `s_questStatusIcons` at `0x00A34F5C`
(`{0,6,1,2,3,5,10,11,4,9,9}`); `CGObject_C::UpdateInteractIcon` OFFSET `0x00745DA0` selects
`s_questStatusIcons[m_questStatus]`, then `s_interactIcons[icon]`.

Cause: the server sends the 0-10 `QuestGiverStatus` (`QuestDef.h:110-126`: 1 UNAVAILABLE,
2 LOW_LEVEL_AVAILABLE, 3 LOW_LEVEL_REWARD_REP, 4 LOW_LEVEL_AVAILABLE_REP, 5 INCOMPLETE,
6 REWARD_REP, 7 AVAILABLE_REP, 8 AVAILABLE, 9 REWARD2, 10 REWARD). `marker_model` instead used a
0-7 dialog-status table (`0x80c454 = {0,3,0,2,7,1,6,6}`). Status 3 drew the grey `?`
(`TalkToMeQuestion_Grey`, Grad1A white-to-black) instead of the light-blue `?`; status 5 drew
the gold `!` (`TalkToMe`, YellowOrange64) instead of the grey `?`; status 8 drew no marker
instead of the gold `!`; statuses 9/10 drew no marker instead of the gold `?`. The gold texture
was not lost in the renderer; the wrong file was selected for the status.

Repair: `marker_model` now applies the reference's exact `s_questStatusIcons` through
`s_interactIcons`, with a focused regression
`quest_markers::tests::the_marker_model_is_the_reference_status_table` covering statuses 0-11.

Verification: `cargo test -p benilla-app --lib the_marker_model_is_the_reference_status_table`
passes (1 passed). `cargo build -p benilla` passes; binary SHA-256
`A7821AF0B50D266F1918D169075E1D4D936F769522A29A25D2D176031D5ACBB7` from HEAD `d1b54f50` plus this
patch. Bounded run `connected-run-20261031-132404-565` (screenshot
`oc-run-20261031-132404-069.png`, profile wm-hd/shadow-data, 45-second probe bound) logs the
new selection: `TalkToMeQuestion_LTBlue.m2` over `0xf130004ddf002b8d` (status 3) and
`TalkToMeQuestion_Grey.m2` over `0xf130004ea2002891` (status 5). The old binary logged only
`TalkToMeQuestion_Grey.m2` for the camp's status-3 NPCs. The run logged in as the identity's
current character (Jeckle, Tanaris), not the camp character; the marker log is the rendered
signal for the status-3 file change, and the status-5 grey marker remains the model's Grad1A
white-to-black art. The gold `!`/`?` (statuses 8/9/10) are now selected correctly and use
YellowOrange64.

Remaining: a rendered same-NPC capture with an available (status 8) or reward (9/10) quest is
still needed to see the gold marker in frame; the unit test covers the selection. The
status-5 grey `?` art itself is unchanged. The glue character-create textures, tent camera,
map dropdowns, debuglocals noise and Share state texture remain open as recorded.
