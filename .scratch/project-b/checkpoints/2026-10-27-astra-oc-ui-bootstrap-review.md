# Astra review — OC's Wrath UI bootstrap

Reviewed committed range bbbe94ca..fcbd2481 and the retained run33/run35 logs. Dirty OC work
was read for context and preserved. This was source/log inspection, not a fresh execution.
No check, test, new test case, extractor, game, socket or server operation was performed.

## Findings, ordered by consequence

1. **P1 — Cooldown is admitted without the method the shipped UI uses.**
   `script/object.rs:476` maps the Wrath Cooldown type to a generic Frame. There is no
   SetCooldown implementation in the UI crate. The pinned 3.3.5 `Cooldown.lua:3` calls
   self:SetCooldown(start, duration) for every active timer. The host's replacement
   CooldownFrame_SetTimer in `script/wrath_natives.rs` only writes table fields and changes
   visibility; it does not repair the stock helper when FrameXML defines it. An idle smoke
   cannot close this: the active action/spell/item cooldown path remains incomplete, in
   addition to the already disclosed absent sweep renderer. This is source-demonstrated;
   the active-timer runtime failure was not executed by Astra. OC owns the current native
   queries/object bootstrap work; no overlapping patch was made.

2. **P1 — Fixed query/action answers must remain incomplete coverage.**
   `wrath_natives.rs` reports no tracking entries regardless of the player's known spells,
   a permanently zero CalendarGetDate, and silently consumes VehicleExit. These lack a
   production state/action owner. Unsupported host functionality is not evidence that the
   server/player has no state. GameTime.lua reads the day back into the visible clock, so
   zero dates are not confined to an unused settings panel. BNGetMaxPlayersInConversation's
   fixed 10 also has no target reference. These can advance load-time reachability but must
   not count as faithful implementations or playable acceptance.

3. **P1 — issecure was unconditional, independent of the existing caller context.**
   `wrath_security.rs` returned true in tagged addon/macro calls as well as stock calls.
   RestrictedFrames.lua:100 uses it to guard frame-handle creation; ActionButton.lua uses it
   for show-grid state. Repaired narrowly in 2c13c575 by reading taint::current. No new taint
   propagation, permission system or securecall behavior was introduced. The existing
   stack-scan limitations in taint.rs remain: this does not establish Wrath secure execution.

4. **P2 — newproxy did not implement the claimed Lua 5.1 contract.**
   6a2ac724 rejected nil, accepted userdata with no metatable, and copied unrelated userdata
   metatables without checking origin. 2c0736ed installs the native with the existing closure
   and weak-table mechanisms: nil/false allocate without a metatable, true creates and records
   one, and reuse requires a recorded metatable. Protected __metatable values are not queried
   through Lua's public getmetatable. This is ordinary reference-defined API behavior, not
   an additional sandbox-hardening project. Exact 12340 binary ABI is not established by the
   independent Lua 5.1 baseline. The correction is source-only and uncompiled.

5. **P2 — Animation objects are a partial facade, not an animation engine.**
   `script/animation.rs` changes only a Lua playing boolean; no time progression,
   interpolation, completion or callback firing exists. Loader nesting also treats every
   non-Scripts child as an animation object, rather than decoding parameter children by
   their own schema. OC discloses the timeline gap, which is good; neither XML load success
   nor IsPlaying alone proves visible animation behavior. No speculative replacement was
   added in this review.

6. **P2 — The smoke warning allowance is wider than its comment.**
   `wrath_ui_slice_tests.rs:79` permits every warning with the GlobalStrings key-miss suffix,
   while its rationale names OPTIONS and BATTLEGROUND_LORETEXT_1. A newly missing localization
   key would also pass. Keep only evidenced exceptions or record a deliberate broader gate;
   do not treat this as zero-warning coverage. The fixture is OC's dirty work and was not edited.

RegisterStaticConstants is also a no-op whose rationale relies on an exported API stub and
current consumers, not a recovered native mechanism. Its exact behavior remains unverified.
securecall is still a direct Lua call wrapper; no error-isolation or context-restoration
acceptance is established here. Expanding that mechanism needs a reference and separate scope.

## What is substantiated

- The class-label fixture now uses the shared catalog projection rather than fabricated labels.
- The selected-profile/BENILLA_REQUIRE_DATA banner is present in retained run35.log.
- That log reaches FloatingChatFrame.xml and then FAILS: missing SetClampRectInsets,
  a missing numWoWOnline result, a dropdown nil button, and a ChatFrame2 assertion.
- Thus the Constants.lua blocker moved; the full PlayerFrame smoke has not passed.
- Earlier logs/checkpoint describe intermediate failures. No exact source pin appears in
  run35's visible admission banner; do not substitute its outcome for a current-tip result.
- The current OC dirty chat/region/bootstrap patches may change those failures and need their
  own authorized pinned evidence. Astra did not overwrite or execute them.

## Follow-through and coordination

Astra committed 2c0736ed (newproxy) and 2c13c575 (issecure query), with exact-file claims before
editing and explicit-path commits. Both need later authorized verification. Previous SetShown
leaf repair 67511a2a likewise has no dedicated post-fix regression rerun in Astra's evidence.
All prior grants are consumed. No speculative race/security mechanism was added.

OC should finish the current chat bootstrap repairs, then address the real Cooldown native
and retain unsupported-feature distinctions. The full stock-client replacement gate and
Northshire rendered/live gate remain open. Procgen remains outside the priority path.

## Primary references read

- Lua 5.1.5 base library, luaB_newproxy and base_open:
  https://www.lua.org/source/5.1/lbaselib.c.html
- Target FrameXML pinned at d0339b17b0221db76e6acd2dc2915d224a5b62ca:
  https://github.com/wowgaming/3.3.5-interface-files/blob/d0339b17b0221db76e6acd2dc2915d224a5b62ca/Cooldown.lua
  https://github.com/wowgaming/3.3.5-interface-files/blob/d0339b17b0221db76e6acd2dc2915d224a5b62ca/RestrictedFrames.lua
  https://github.com/wowgaming/3.3.5-interface-files/blob/d0339b17b0221db76e6acd2dc2915d224a5b62ca/ActionButton.lua
  https://github.com/wowgaming/3.3.5-interface-files/blob/d0339b17b0221db76e6acd2dc2915d224a5b62ca/GameTime.lua

Read external reference files only; no install code was copied into the repository.

## 2026-10-28 committed smoke supersession

Reviewed OC commits b1eef5a9, cdd4a7ca and 04563c9e against current source and the
retained run59.log. The earlier run35 FAIL above is superseded for this fixture:
run59 contains the Wrath335/interface30300/wm-hd manifest/data banner and one passing
stock_player_frame_host_smoke_test (0 failures, 0 ignored, 6601 filtered). The human
also reports a committed-tree repeat passing in 0.48 s. The retained run59 itself has
no Git revision or manifest hash in its admission banner; the committed-tree repeat
is owner-reported rather than independently pinned evidence. No check was re-run here.

The source genuinely uses with_target(Wrath335), required data admission, the real
profile chain, localized catalogs, TOC-order loading through PlayerFrame.xml, no shared
test-loader stand-ins, loader error/missing-file checks and recorded Lua-error checks.
Its assertions prove frame existence, a manually fired UNIT_HEALTH reaching a custom
registered observer, a positive seeded UnitHealth, an absent target and same-VM CVar
readback. They do not assert the rendered health-bar value or session-produced events.

VoiceChat.xml is now excluded by the shared production-loader decision as well as the
fixture, with one missing VoiceChatSpeakerTemplate warning explicitly allowed. The human
handoff confirms the permanent voice-disabled product decision; do not call this an
unmodified complete stock TOC load. The prefix spans the reported 75 TOC positions with
the voice entry excluded. Installed addons, later TOC entries and their runtime calls,
native cooldowns, animation timelines, persistence/reload and graphical/live acceptance
are outside this result. The broad test comment that any API gap fails must be read
with these boundaries and the native defaults below, not as a fidelity guarantee.

Actionable remaining fidelity findings, present in committed source:
- cdd4a7ca's GetDungeonDifficulty/GetRaidDifficulty always return 1; GetOptOutOfLoot
  always returns false; UnitThreatSituation always returns nil; GetInstanceInfo always
  describes an open world; UnitInBattleground returns no values. These are unconditional
  bootstrap answers, not observations of server/session facts. They can show incorrect
  UI when the character's real context differs. Keep their semantic coverage open until
  existing owners supply the actual facts; a successful PlayerFrame load does not close it.
- Cooldown remains a Frame and the fallback global only stores Lua fields/show state.
  Once stock Cooldown.lua overrides that global, an active timer calls missing SetCooldown.
  The seeded smoke does not establish an active cooldown or visible sweep. A real widget,
  lifecycle and rendering path is needed; do not add a no-op native to satisfy a call.
- Animation play state, empty tracking, zero calendar data and missing chat geometry
  persistence remain explicitly incomplete in OC's checkpoint. The Steam voice-provider
  attribution in reference_ui.rs is unsupported by this source review; the actual product
  decision is simply that this host supplies no voice service.

Astra's subsequent 8e919cec bag-space query was committed after this smoke and has no
build/test evidence. The current worktree is clean after explicit-path source commits.
No test cases, builds/tests, generators, game/socket/WM/server/DB or graphical runs were
performed by Astra in this continuation. Full stock-client replacement stays the gate.
