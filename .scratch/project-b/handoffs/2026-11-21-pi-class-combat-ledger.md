# Class combat ledger

Status: ready to land
Owner: Pi
Branch: pi/class-combat-ledger
Issue: 18
Next: Merge latest `origin/main`, push `HEAD:main`, delete the remote topic branch, and record the landing result.

What changed: Added the class-specific combat compatibility ledger to issue 18 for warrior, rogue,
mage, priest, warlock, hunter, druid, paladin, and death knight. Each row names the current repo
owner/file lines, 12340 reference symbols and addresses, source-backed shared seams, and explicit
unknowns for class-specific formulas, server outcomes, and live behavior. The issue-18 combat-log
rows were also corrected to reflect landed feed and retention history work (`f902a884c`,
`7e4628ca1`, `519095eb3`, `9b8ba646c`, `cd95c37d9`, `bdf7391cd`, `681426a2b`, `8a78e95ef`).

Reference addresses: `Spell_C_IsUsableAction 0x0080B5D0`, `Spell_C_GetPowerCost 0x008012F0`,
`Spell_C_GetPowerDisplayMod 0x007FDE00`, `CGUnit_C::GetPredictedPower 0x0071C2E0`,
`CGUnit_C::GetShapeshiftForm 0x0071AF70`, `SpellRec::UsableInShapeshift 0x007FE850`,
`CGGameUI::GetComboPoints 0x00513920`, `Script_GetComboPoints 0x00611670`, and rune anchors
`CGRuneInfo::ConvertRune 0x005EE020`, `ResyncRunes 0x005EE440`, `AddRunePower 0x005EE520`,
`HasRunesForSpell 0x005EE5B0`, `ComputeRuneSpellCooldown 0x005EE7E0`.

CI run id and result: No CI run; this is a Markdown-only change and the current workflow does not
trigger CI for documentation-only commits. Permitted static check `git diff --check` passed.

What is not proven in game: No local Cargo, client, server, login or in-game run was authorized.
Generic resource/UI seams do not prove class spell coefficients, talents, glyphs, procs, server
acceptance, combat-log correctness per class, persistence, or rendered stock-frame behavior.
