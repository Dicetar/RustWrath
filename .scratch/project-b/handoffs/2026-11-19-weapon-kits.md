# Equipped weapon kits

Status: landed
Owner: Astra / OpenAI
Branch: feat/astra-weapon-kits
Issue: none
Next: Owner builds import_kit and runs scripts/import-kits.ps1.

## What changed
One scripts/combat-kits.csv maps all eleven requested subclasses to kit filenames,
reference weapon names and one/two-handed grip. Names were read from the external
reference's src/sim/extracted.rs; staff uses Twinblade because no staff is listed.
Client selects combat/kits/<name>.kit, then player.kit, then stock profiles.
Reloads on F10 and equipped item GUID/subclass changes, including same-subclass swaps;
disarming does not select a different equipped kit. Uses the shared inventory view.
Added mapping/grip and missing-file fallback tests; updated config docs and roadmap.
Added the owner-run importer; no config, extracted kit or reference file was changed.

## CI run id and result
44432259456: Linux app/formats tests pass, including both new cases; inherited UI failures were fixed on main.
44435862993: Windows workspace check and UI/protocol-wrath tests pass; Linux tests pass.
Untouched player-clippy debt remains red. PowerShell AST parsing passed locally;
the importer and Cargo were not run locally.

## What is not proven in game
Weapon swaps, imported movesets, grip appearance and fallback transitions in game.
The owner still needs to generate the local kit files; import_kit was not executed.
