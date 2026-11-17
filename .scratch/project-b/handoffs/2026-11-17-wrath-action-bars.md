# B5 — Wrath action bars, spellbook and macros

Status: in progress
Owner: OpenCode / DeepSeek V4.1 Flash (took over from Pi / OpenAI)
Branch: fix/wrath-action-bars
Issue: none
Next: finish the temporary/vehicle bar and the Lua RunMacro/secure-button owner, then complete the installed-stock-caller audit; do not land this checkpoint as completed B5.

Started from origin/main `7ef93503` after B4 landed at `b13c163d`; A2 also landed at `58e98b0d`.
Worktree: `D:\Projects\wow\worktrees\fix-wrath-action-bars`. Shared dirty checkout
`D:\Projects\wow\client` is untouched. Server/AC under `D:\Projects\wow\server`, reference
`D:\Projects\wow\reference\wowdecomp-reference` and installed Data are read-only.
Reference HEAD verified at AGENTS pin `e67b6ffef23655dc7da061cb2d36b7963f6ef71a`.
Investigate behavior, do not copy implementation. Start: release-matrix F15 and ui_action/drain.rs.
B4 owns spell; consume its outcomes without rewriting it. No combat_log/combat_text edits.
Data behaviors must remain editable by the live hotpatch/Lua translation route.

Compilation: not run. Tests: not run (focused-case ceiling 20). Client: not run.
Earlier AutoSwarm availability inspections timed out; this continuation detected OpenCode
v2.3.0 and assigned two read-only reviews (macro ownership and bar ownership).
Calls: `call_724d3769f52d46b2b2e6669803ef44d7`, `call_d2c82347145844f6af9332ce9bf426ee`.
Both reached native ACP execution but returned no findings after repeated waits and wrap-up
messages. Inspected handles/logs (empty) and cancelled both after about eleven minutes;
collected final cancelled outcomes, no artifacts. No child had permission to edit, compile,
test or launch. No successful review or callable-model verification is claimed.

## Pushed source checkpoint

Repaired the macro ABI in `crates/benilla-ui/src/script/macros.rs`, preserving donor behavior
for donor VMs. Wrath CreateMacro now takes perCharacter in argument four (and never creates
local-only records); GetMacroInfo returns three values, or zero on a miss. Wrath GetMacroInfo,
EditMacro, PickupMacro and DeleteMacro resolve the same index-or-name identity, with numeric
strings treated as indices. EditMacro ignores the obsolete donor local flag.
This is a shared binding repair, not a FrameXML workaround or a success stub.

Investigated the pinned reference's `WoW/Source/Ui/UIMacros_Scripts.cpp`:
GetMacroFromLua `0x00566DF0`, GetMacroInfo `0x00566EB0`, EditMacro `0x00566FA0`,
PickupMacro `0x005674F0`, CreateMacro `0x00568160`. No implementation was copied.
Added three focused regression cases in `macros/wrath_tests.rs` (not run): fourth-argument
scope; lookup/return arity; edit/pickup/delete identity. Existing donor tests remain intact.
Direct rustfmt and `git diff --check` ran; these are source formatting checks, not compilation.
No changes to spell, combat_log or combat_text. No new data tables or fixed mappings added.

## Owner/query trace (source only; not yet complete 12340 acceptance)

| Stock API / operation | Existing real owner / consumer | Remaining review |
| --- | --- | --- |
| IsUsableAction | ui_action/state.rs usable cache -> ActionState usable/power pair | audit Wrath fields and installed callers |
| IsActionInRange / ActionHasRange | state.rs -> spell range check -> ActionState tri-state | installed range tint, unit argument behavior |
| GetActionCooldown | state.rs -> spell Cooldowns -> UI clock-based triple | consume B4 as-is; verify stock swipe |
| GetActionCount | feed.rs + recount.rs -> inventory-backed ActionSlot count | macro item bindings / spell charges |
| IsCurrentAction / IsAutoRepeatAction | action.rs ActionState checks live CastView | full B4/A2 producer trace and stock consumers |
| GetBonusBarOffset | feed.rs bonus_bar_offset -> form DBC bonus_bar | incomplete: temporary/vehicle and preset-form bar |
| Spellbook drag / relog | cursor/bar.rs PlaceAction -> ActionBarSteps::set -> Wrath SetActionButton encoder -> AC | verify selected stock drag caller and persistence |
| Spec replacement | Wrath parser (144 words) -> translate_gameplay -> spell/net.rs action_buttons -> ActionSlots dirty | keep B4 file untouched; state-1 replaces, state-2 pending preserves live bar |
| Macro /cast | ui_macro record/body -> run_macro EXECUTE_CHAT_LINE -> stock chat handler -> cast host | stock Wrath runner/security and conditional parsing audit still open |

AC research: `Azerothcore_WoTLK_Rebuild/source/azerothcore/src/server/game/Handlers/MiscHandler.cpp`
HandleSetActionButtonOpcode reads button u8 + packed u32 and validates macro/spell/item kinds.
`Entities/Player/Player.cpp` SendActionButtons sends state plus 144 words except state 2;
IsActionButtonDataValid validates the slot, kind and learned spell. Relog/spec source path exists;
no server, database or live receiver was exercised.

## Ordinary paging checkpoint — continuation

Merged main `78166894` without conflicts. Part 1 is pushed at `beb86564`, still a
handoff-only delta against main, with the numbered owner run card and source review complete.

`script/action.rs` now keeps the ordinary Wrath page in its native model, rather than treating
writable `CURRENT_ACTIONBAR_PAGE` as authority. Invalid/nonfinite/missing inputs are refused
without state changes; numeric strings and fractional numbers use Lua numeric coercion and
truncation. Re-selecting the same page is a no-op; the change event sees the committed page.
The page-count policy is read from stock Lua `NUM_ACTIONBAR_PAGES` at each write, not fixed
in Rust; installed-caller confirmation is still pending the read-only review. Donor behavior
is unchanged. Three focused source cases added (six total B5 additions), not run.

Reference: `ActionBarFrame_Scripts.cpp` ChangeActionBarPage `0x005A7F60` and GetActionBarPage
`0x005A7FD0`. Also investigated `GameUI.cpp` CanPerformAction `0x005191C0`: Wrath action 14
checks taint and protected-action permission, not the donor gate's numeric tag 14. That gate
and the temporary-page flags are NOT implemented by this ordinary-page repair; do not claim
full Wrath paging/security acceptance. No reference implementation was copied.

Direct rustfmt ran; `git diff --check` passed. Model formatting unrelated to the new field was
kept as main has it. Compilation: not run. Tests: not run. Client: not run.
`gh run list --branch fix/wrath-action-bars -L 1` returned no runs before and after the
`4012c29d` push. CI: not run yet. Child reviews did not complete; installed page-policy
confirmation and the owner gaps below remain open. This is a pushed WIP checkpoint,
not the requested completed B5 delivery.

## Macro capacity checkpoint — takeover (2026-11-17)

Owner changed to OpenCode / DeepSeek V4.1 Flash; the branch was clean and in sync with
`origin/fix/wrath-action-bars`. The stock MacroUI showed the concrete defect: 3.3.5's
`MAX_ACCOUNT_MACROS = 36` and `MAX_CHARACTER_MACROS = 18` read account 1..=36 and character
37..=54, while the donor table stopped at 18 account entries and put character at 19..=36, so the
character tab was empty and the 19th..36th account entries read character macros.

- `MacroLimits` now carries the per-scope caps and the Lua index stride: donor 18/18 over 18,
  12340 36/18 over 36 (the reference's `m_sortedMacros[72]`, `UIMacros.cpp:35`, and
  `s_maxMacros[2] = {36,18}`, `:62`).
- `MacroState` stores the limits; `from_scopes_with_limits` parses 12340 account files with 36
  and character with 18; `id_at`, `index_of_id` and `create_macro` use the scoped caps and the
  36/18 stride. Donor behavior is unchanged through `from_scopes`.
- The stock FrameXML globals are the live policy: `UiScript::macro_limits` reads
  `MAX_ACCOUNT_MACROS`/`MAX_CHARACTER_MACROS` (target defaults when absent), `set_macros`
  adopts them, and every macro verb refreshes them, so a live edit changes the visible table and
  the refusal cap without a Rust table.
- `ui_macro/store::parse_with_cap` keeps 36 account records on Wrath while `parse` still stops
  at the donor's 18; the loader sizes both files from the same policy.
- The secure `[vehicleui]` state now reads the real 3.3.5 `UnitHasVehicleUI("player")` instead
  of the absent `HasVehicleActionBar` global (there is no such 12340 global; the stock UI uses
  `UnitHasVehicleUI`).

Audit of the other requested areas, no code change needed:

- Cooldowns: `GetActionCooldown` in `script/action.rs` consumes B4's `Cooldowns` triple in the
  stock `ActionButton.lua:347` shape; `IsUsableAction`, `IsActionInRange`, `ActionHasRange`
  and `GetActionCount` have their owners in `ui_action/state.rs` and `range_tests`.
- Spellbook: `PickupSpell`/`PlaceAction` and the Wrath `(id, bookType)` spell references are
  already implemented in `script/spellbook.rs`/`cursor/bar.rs`; no open caller was found in the
  stock 3.3.5 SpellBookFrame.
- Macro mutation security: 12340's `CreateMacro`/`EditMacro`/`DeleteMacro` gate on
  `CanPerformAction(12)` and `PickupMacro` on 14; those tags are past the 0x494a50 table and
  pass every context (`protected.rs`'s `ARMS.get(...).unwrap_or(Arm::Pass)`). The action bar's
  macro press reaches the app's real `run_macro` through `UseAction`; the donor Lua
  `RunMacro`/`RunMacroText` globals are still absent for secure/addon buttons, named below.
- Five focused source cases added (four in `macros/wrath_tests.rs`, one in
  `ui_macro/store.rs`); total B5 additions now 11. Direct rustfmt ran on the touched files;
  `git diff --check` passed. Compilation, tests and CI are not run.

## Known gaps — not ready to merge

1. ~~MacroState and file parser still use donor 18-per-scope limits/indexing.~~ **Closed** by the
   `MacroLimits` checkpoint above: 12340's 36+18 caps and 36-entry stride, with the stock globals
   as the live policy. The donor's 18+18 path is unchanged.
2. Wrath mutation security **reviewed**: `CreateMacro`/`EditMacro`/`DeleteMacro` use
   `CanPerformAction(12)` and `PickupMacro` uses 14, tags past the 0x494a50 table that pass every
   context; no extra gate is missing. The action bar's macro press reaches the app's real
   `run_macro` through `UseAction`. The Lua `RunMacro`/`RunMacroText` globals (stock
   `SecureTemplates.lua:373/378`) and the remaining macro-frame query setters are still absent;
   they are an addon/secure-button surface, not the default bar path.
3. The secure `[vehicleui]` key now reads the real `UnitHasVehicleUI("player")`; there is no
   12340 `HasVehicleActionBar` global. `IsPossessBarVisible` is still constant false and the
   vehicle temporary action spell table, visibility, slot contents and execution are unbuilt.
4. Reference `ActionBarFrame.cpp` UpdateBonusBar `0x005A83C0` selects offset 5 for temporary
   flags or form preset spells; feed.rs only reads the form bonus_bar column. GetActionBarPage
   `0x005A7FD0` also forces page 1 while temporary flags are active. This is not yet implemented.
5. Ordinary page validation/native state is repaired as above. Temporary override/restoration
   and the Wrath protected-action gate remain unimplemented.
6. Installed stock FrameXML call-site enumeration, positive/empty owner audit and hotpatch policy
   integration are unfinished. No extracted FrameXML/GlueXML was written or committed.

Do not mark ready from the macro tests alone. The temporary/vehicle bar, the secure Lua
`RunMacro` surface and the installed-stock-caller audit remain open; the total focused-case
budget is 20 (11 used).

## Owner check list — after the next batch build

1. drag two spells to the bar;
2. relog;
3. check the cooldown swipe and the out-of-range tint;
4. make and run a /cast macro;
5. open the macro frame: the account tab lists 36 slots, the character tab starts at 37, the 37th
   account macro is refused;
6. switch spec, if available.
