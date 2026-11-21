# Focus target: FocusUnit, ClearFocus, PLAYER_FOCUS_CHANGED and the focus unit token

Status: ready to merge
Owner: Claude Code / Claude Opus 5.5 (run from the brief written for Gemini)
Branch: gemini/focus-target
Issue: none
Next: integrator reviews and lands; owner checks FocusFrame in game (`/focus`, `/clearfocus`, a despawn)

## Status

Source finished. `benilla-ui` focused tests pass locally; `benilla-app` compiles (`cargo check`)
but its tests were not run locally. Not seen in game.

## Owner: Gemini

The brief named Gemini as owner. Claude Code (Claude Opus 5.5) did this run instead, from the same
brief, so commits carry Claude's trailer rather than `models/gemini`.

## Branch and bundle file

- Branch `gemini/focus-target`, from `origin/main` b520b19aa.
- Bundle `gemini-focus-target.bundle` in the worktree root, uncommitted. It is only a fallback;
  the branch was pushed if the push line in the delivery report says so.

## What changed

- `benilla-ui` `script/wrath_natives.rs`:
  - `FocusUnit([unit])` is now a native; it was missing.
  - `ClearFocus()` replaces the recorded-unsupported no-op, so that
    `record_unsupported_native_once("ClearFocus", …)` is gone.
  - Both go through one `set_focus_target` (`SetFocusTarget`):
    - the same GUID is a no-op;
    - tainted code is refused;
    - otherwise the GUID is stored and `PLAYER_FOCUS_CHANGED` fires with no arguments.
  - `UiScript::focus_guid()` and `UiScript::set_focus_target(guid)` are new; the engine uses the
    second for its clears.
- `benilla-ui` `script/protected.rs`: `forbid_tainted`, the type-0
  `ShowBlockedActionFeedback`.
  - A `RunScript` chunk gets `MACRO_ACTION_FORBIDDEN("FocusUnit()")`.
  - An insecure addon gets `ADDON_ACTION_FORBIDDEN(addon, "FocusUnit()")`.
- `benilla-ui` `script/unit/resolve.rs`:
  - The Wrath focus now has field watchers, which is the reference's `RegisterScript(0x2000)`.
    So the field bridge's `UNIT_HEALTH`, `UNIT_MANA`, `UNIT_AURA` and the rest reach
    `"focus"` through `unit_tokens_of`, which already listed it.
  - The focus is now a `chain_ends` GUID, so `feed_chain_units` pushes its snapshot and
    `UnitHealth("focus")` and the other getters read a real unit.
- `benilla-app` `target/focus.rs` (new):
  - `FocusTarget` sits beside `Selection`. It is the app's copy of the VM's focus, which the
    app resolver (`UnitTokens`, `"focus"` base) now reads.
  - `follow_focus`, run after `follow_selection_object`, adopts the VM's focus. It clears the
    focus:
    - when its object deactivates (despawn or out of range), unless `IsRaidMemberOrPet` holds the
      GUID;
    - when the unit gains `UNIT_FLAG_NOT_SELECTABLE`.
  - `clear_focus_on_leave_world` is the new sweep leg `SweepLeg::Focus`. It runs after the
    selection's clear and before `PLAYER_LEAVING_WORLD`, and clears a focus that is not a raid
    member or pet.
- `benilla-app` `ui_unit.rs`: `WrathUnitTokens.focus` is removed. Nothing ever wrote it.
- `benilla-app` `ui_aura/units.rs`:
  - The resolver push takes `focus` from the VM rather than overwriting it with 0.
  - The focus joins the walk, so `focustarget` hops and its aura list are built.
- `benilla-app` `ui_script/sweep.rs`: the `SweepLeg::Focus` set is added.
- How the focus clears:
  - `/reload` and logout: a new VM starts with no focus, which matches `CGGameUI::Shutdown`
    clearing it.
  - Death: the focus is not cleared, because no `BreakTarget` caller is a death.

## Reference addresses used

Pin e67b6ffe, `D:/Projects/wow/reference/wowdecomp-reference`.

- Natives and the setter:
  - `Script_FocusUnit` `0x522480` and `Script_ClearFocus` `0x5225E0`
    (`WoW/Source/Ui/GameUI_Scripts.cpp`).
  - `CGGameUI::SetFocusTarget` `0x51FF20` (`GameUI.cpp:4012`): same-GUID return, taint refusal,
    `Unregister`/`RegisterScript(0x2000)`, `PLAYER_FOCUS_CHANGED`.
  - `CGGameUI::ShowBlockedActionFeedback` `0x513530`: with type 0 it signals
    `MACRO_ACTION_FORBIDDEN` or `ADDON_ACTION_FORBIDDEN` with `"%s%s"`.
- Token resolution:
  - `Script_GetGUIDFromToken` `0x60ABF0` (`ScriptEvents.cpp:99`): no text gives the target when
    `defaultToTarget` is set; the `focus` base is `m_focusTarget`.
  - `Script_GetTokenIDsFromGUID` `0x60BB70` and `Script_SendUnitSignal` `0x60BF10`: the focus
    is among the tokens a unit signal fans out to.
- Clears:
  - `CGGameUI::BreakTarget` `0x524350` (`GameUI.cpp:5301`) clears the focus.
  - `BreakTarget` callers:
    - `CGUnit_C::Disable` `0x734FD0`, unless `IsRaidMemberOrPet` (or a duel);
    - `CGUnit_C::OnFlagChanged` `0x73C330`, on flag `0x2000000`;
    - `SMSG_BREAK_TARGET` in `CGGameUI::MessageHandler` `0x526530`.
  - `CGGameUI::LeaveWorld` `0x528C30` (`GameUI.cpp:8219`) clears a focus that is not a raid
    member or pet.
  - `CGGameUI::Shutdown` `0x528F00` (`GameUI.cpp:7496`) clears it unconditionally. It is
    reached from logout and from `/reload` (`CGGameUI::Idle` `0x52AF40`).
  - `PetChangeHandler` `0x6D1970` clears a focus on a dismissed pet.

## Unverified facts

- `Script_GetGUIDFromToken` with a known token that names nobody stores the sentinel
  `0xFFFFFFFFFFFFFFFE` and returns true, so the reference focuses that sentinel. We clear the
  focus instead. Both name no object, and `UnitExists("focus")` is nil either way. The one
  difference: after this, the reference's next `ClearFocus` is a change and ours is not.
  Not checked against the binary.
- Name tokens: `FocusUnit("Bob")` reaches `Script_GetGUIDFromString` in the reference. Here it
  sets nothing, the same as an unknown token. Not implemented.
- Stock FrameXML: the brief says `SecureTemplates.lua`'s `type="focus"` action calls
  `FocusUnit(unit)`, and `ChatFrame.lua`'s `/focus` and `/clearfocus` call
  `FocusUnit`/`ClearFocus`. Both are taken from it and from general knowledge. No local
  FrameXML copy was read, and the test uses a synthetic `OnClick` of that shape.
- `UpdateScriptRegistration` `0x720960` recomputes the flags without `0x2000` and overwrites
  them, so a focused unit's watchers could drop on a party or target change. That looks like a
  reconstruction gap. We keep the focus watched while it is the focus.
- The blocked-action latch: `forbid_tainted` shares the existing gate's `refusing` latch; the
  reference has its own static `showing`.

## Local checks run, and what was not run

Toolchain 1.98.1, sccache wrapper. First pass: `CARGO_TARGET_DIR=D:\Projects\project-b-client\target`;
the follow-up: this worktree's own `target/` (see the CI line below).

- Run:
  - `cargo check -p benilla-ui -p benilla-app`: passed.
  - `cargo test -p benilla-ui --lib wrath_natives::focus_tests`: 7 passed.
  - `cargo test -p benilla-ui --lib -- script::unit::resolve script::unit::chain_tests`: 28
    passed. These are existing tests of the two functions this branch changes, run as a
    regression check beyond the brief's "your tests" filter.
  - `cargo fmt`: clean.
  - `cargo clippy -p benilla-ui -p benilla-app --no-default-features -- -D warnings`: passed.
- Not run:
  - The `benilla-app` tests, locally: one changed
    (`ui_unit::tests::wrath_target_path_resolves_focus_vehicle_and_boss_state`) and one is new
    (`target::focus::tests::the_focus_breaks_where_break_target_runs`, the despawn rule). The test
    binary was not built.
  - Clippy on test targets.
- CI run 44928950573 on 3029f93cc failed. Nearly every `benilla-app` test panicked with Bevy
  B0002: `follow_focus` and `clear_focus_on_leave_world` held `ResMut<FocusTarget>` beside
  `HostedVm`, whose call host reads `Res<FocusTarget>` through `UnitTokens`. The follow-up
  commit moves both systems to `VmWith<FocusWorld>`, the disjoint pair `follow_selection_object`
  uses. CI 44932954995 on 634332de8: compiles; tests pass (Windows and Linux gates).
- The first local checks shared `CARGO_TARGET_DIR` with the other task's worktree, and cargo then
  reported this worktree's edits as fresh: a deliberate `compile_error!` probe passed. The
  follow-up was re-checked with this worktree's own `target/`, where the probe failed as it
  should, and passed `check`, `clippy -D warnings`, `fmt --check` and the 35 `benilla-ui`
  tests above. A B0002 conflict is a runtime schedule check, so only the `benilla-app` tests (CI)
  can prove the fix.

## What is not proven in game

- FocusFrame showing and tracking a unit; `/focus` and `/clearfocus` through the stock chat
  path; a secure `type="focus"` button from stock `SecureTemplates.lua`.
- The despawn, unselectable and leave-world clears with real objects.
- Cast bars, auras and `UNIT_TARGET` for `focus` through the real feeds.
- Known one-frame lag: the app's `FocusTarget` copy is refreshed in `UiFeed`. A call applied in
  the same chunk that set the focus (`/focus` then an app-resolved `TargetUnit("focus")` in one
  macro) reads the old focus. The VM's own readers, and `UseAction(slot, "focus")`, which
  resolves in the VM, are not affected.
- No server help is needed: focus is client-only in 12340.
