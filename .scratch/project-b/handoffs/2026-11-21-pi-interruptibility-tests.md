# Cast interruptibility tests

Status: landed on `main` at `4c084a036`
Owner: Pi
Branch: pi/interruptibility-tests
Issue: spellcast interruptibility
Next: No further source work; gameplay verification remains owner-run and unauthorized in this task.

What changed: `refresh_cast_interruptibility` now matches the pinned 12340 predicate for prevention type, the `AttributesExG & 0x2000` interruptible gate, ordinary interrupt/silence masks, non-player special masks (`AttributesExG & 0x800/0x1000`), cast-school immunity, cast-wide interrupt/silence immunity, and aura immunity effects. `Spell.dbc` Wrath rows now retain `AttributesExG`. `SMSG_SPELL_START` now decodes the `CAST_FLAG_IMMUNITIES` school/immunity words and carries them through the native active-cast model. Focused tests cover prevention types, spell interrupt/silence sources, special non-player sources, school and cast-wide immunity, aura immunity, and one-time versus unchanged `InterruptibilityChanged` emission. Implementation commit: `b6f65edb7`.

Reference addresses: `CGUnit_C::ProcessCastNotInterruptible`, `Unit_C.cpp:11745` / `0x007262E0`; `CGSpellBook::UpdateSpellList`, `SpellBookFrame.cpp:780-810`; `PendingSpellCast::OnSpellStartPacket`, `Spell_C.cpp:1000-1075` / `0x00806700`; `CGUnit_C::SetCastImmunities`, `Unit_C.cpp:6112` / `0x00715AB0`; `CGUnit_C::SetCastSchoolImmunities`, `Unit_C.cpp:6107` / `0x00715AA0`.

CI run id and result: GitHub Actions CI run `44929979240` (`https://github.com/Dicetar/RustWrath/actions/runs/44929979240`) was started for the landed merge and was `in_progress` at handoff finalization; final completion is not yet recorded here.

What is not proven in game: No local Cargo, client, server, login or in-game run was run or is authorized. CI and source tests do not establish live gameplay behavior, packet acceptance, rendered cast-bar behavior, or in-game interruptibility.
