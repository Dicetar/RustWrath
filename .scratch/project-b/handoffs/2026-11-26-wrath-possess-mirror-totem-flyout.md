# Handoff: Wrath 3.3.5a Totem Flyout Discovery, Possession Action Bar, Mirror Timers & Quest Links (2026-11-26)

## Task & Scope
Closed the next four stock-replacement slices from Issue 18 (`18-stock-replacement-inventory.md`):
1. **Slice 1 — Shaman Multi-Cast Totem Flyout Discovery (`GetMultiCastTotemSpells` `0x006EA480`)**:
   - Extracted `RequiredTotemCategoryID[2]` (`Spell.dbc` columns `222..=223`) in `crates/benilla-formats/src/spells/layout.rs`.
   - Joined `DBFilesClient\TotemCategory.dbc` (with standard 12340 totem category fallback masks for Fire `4 => 0x01`, Earth `2 => 0x02`, Water `5 => 0x04`, Air `3 => 0x08`, and Master Totem `162/163/164/190 => 0x0F`) into `SpellDisplay::required_totem_categories` and `SpellDisplay::totem_slot_mask` (`crates/benilla-formats/src/spells/display.rs`, `crates/benilla-formats/src/spells/mod.rs`).
   - Projected known totem spell IDs per 1-based element slot (`1..=4`) in `crates/benilla-app/src/ui_spellbook.rs` via `UiScript::set_wrath_totem_flyout_spells`, and wired `GetMultiCastTotemSpells(slot)` in `crates/benilla-ui/src/script/wrath_natives.rs`.
2. **Slice 2 — Possession Action Bar (`GetPossessInfo` `0x005DD4E0`, `IsPossessBarVisible` `0x005A8820`, `UPDATE_POSSESS_BAR`)**:
   - Updated `GetPossessInfo(index)` in `crates/benilla-ui/src/script/wrath_natives.rs` to return `(texture, name, enabled)` from `model.pet_bar.slots[index - 1]` when `model.vehicle_guid.is_none() && model.death.possess_bar` is active.
   - Emitted `UPDATE_POSSESS_BAR` alongside `UPDATE_BONUS_ACTIONBAR` on possession state edges in `crates/benilla-app/src/death/mod.rs` and on possessed action bar updates in `crates/benilla-app/src/ui_pet/bar.rs`.
3. **Slice 3 — Breath, Fatigue & Feign Death Mirror Timers (`0x1D9`–`0x1DB`, `GetMirrorTimerInfo`, `GetMirrorTimerProgress`)**:
   - Wired `SMSG_START_MIRROR_TIMER` (`0x1D9`), `SMSG_PAUSE_MIRROR_TIMER` (`0x1DA`), and `SMSG_STOP_MIRROR_TIMER` (`0x1DB`) in `crates/benilla-protocol-wrath/src/gameplay.rs` and translated them to `SessionEvent::MirrorTimerStart`, `MirrorTimerPause`, and `MirrorTimerStop` in `crates/benilla-app/src/wrath.rs`.
   - Added `WrathMirrorTimerSlot` (`progress_ms(now)`) and `wrath_mirror_timers: [Option<WrathMirrorTimerSlot>; 3]` to `Model` (`crates/benilla-ui/src/script/model.rs`), synchronized from `crates/benilla-app/src/ui_mirror.rs`.
   - Replaced the constant `GetMirrorTimerInfo` stub with live slot projection and registered `GetMirrorTimerProgress(timer)` (`0x00516720`) in `crates/benilla-ui/src/script/wrath_natives.rs`.
4. **Slice 4 — Quest Hyperlinks (`GetQuestLink` `0x005E7C10`) & Issue 18 Ledger Sync**:
   - Replaced the `GetQuestLink` nil stub in `crates/benilla-ui/src/script/wrath_natives.rs` with 3.3.5a quest hyperlink formatting (`|cffffff00|Hquest:<questID>:<level>|h[<title>]|h|r`) supporting both 1-based quest-log display indices and direct quest IDs.
   - Updated `.scratch/project-b/issues/18-stock-replacement-inventory.md` (Table F, Possession bar, Multi-cast/totem bar, GameObject objective names `oc/quest-go-name`, and Quest log watch helpers).

## Changed Paths
- `crates/benilla-formats/src/spells/layout.rs`
- `crates/benilla-formats/src/spells/display.rs`
- `crates/benilla-formats/src/spells/mod.rs`
- `crates/benilla-app/src/ui_spellbook.rs`
- `crates/benilla-app/src/death/mod.rs`
- `crates/benilla-app/src/ui_pet/bar.rs`
- `crates/benilla-protocol-wrath/src/gameplay.rs`
- `crates/benilla-app/src/wrath.rs`
- `crates/benilla-app/src/ui_mirror.rs`
- `crates/benilla-ui/src/script/model.rs`
- `crates/benilla-ui/src/script/mod.rs`
- `crates/benilla-ui/src/script/wrath_natives.rs`
- `.scratch/project-b/issues/18-stock-replacement-inventory.md`
- `.scratch/project-b/handoffs/2026-11-26-wrath-possess-mirror-totem-flyout.md`

## Verification & Evidence Status
- **Source & Focused Unit Tests Authored**:
  - `benilla_ui::script::wrath_natives::tests::wrath_multicast_totem_flyout_and_possess_and_mirror_and_quest_link`
- **Cargo compilation & test execution**: Not run in this cloud container (`cargo`, `rustc`, `sccache`, `gh`, and `.git` are not installed in this environment). Per `docs/agents/sync.md`, compilation and tests are reported as not run.
