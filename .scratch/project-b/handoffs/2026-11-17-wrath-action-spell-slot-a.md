# Wrath action-bar spell fallback and cooldown owner

Status: landed
Owner: OpenCode / DeepSeek V4.1 Flash (took over the branch)
Branch: fix/wrath-action-spell-slot-a
Issue: none
Next: owner batch build and the four checks below; source review is complete and no Cargo, test or client run was made.

Merged `origin/main` `7114694f` (2026-11-17) into the branch without conflicts. The branch had
no task note; this one is the claim and the review surface. Worktree used:
`C:\Users\Qraag\.local\share\opencode\worktree\399553\rw-action-spell-a` (the clean
existing worktree; the duplicate under `D:\Projects\wow\worktrees` was removed after it was
created detached).

## What was wrong

`GetSpellCooldown(id, bookType)` accepts 12340's action-bar spell ID form: a spell with no
displayed spellbook slot is passed as its raw ID plus a book type. The donor seam resolved that
fallback as a `(spell, pet)` pair, so a `"pet"` book could read a pet cooldown for a player-bar
ID. It also required the host's `Spell.dbc` catalog to know the spell before asking for its
cooldown, while the reference's `Script_GetSpellCooldown`
(`SpellBookFrame_Scripts.cpp:504-534` -> `Spell_C_GetSpellCooldown`) forwards the ID with no DBC
lookup. A cold/unknown spell therefore answered the no-cooldown triple instead of the host's
cold answer, and a numeric-string ID was not treated as numeric.

## What changed

- `spellbook::action_bar_spell_reference` now answers `Option<u32>`, rejects the `"pet"`
  book (the fallback is always the player book), and accepts a numeric string through
  `coerce_number`.
- `spellbook::book_cooldown` forwards the fallback ID to the host's cooldown owner directly,
  with `pet = false`; the `Spell.dbc` pre-gate is removed.
- `wrath_spell.rs` (the shared `GetSpellInfo`/`GetSpellLink`/`GetSpellTexture` reference
  resolver) adapts to the `u32` result with the player book.
- The branch's three focused cases cover the reference resolution, the host's cold/known
  cooldown answer and the action-bar ID fallback; they are written but **not run**.

## What was checked

- 12340 `SpellBookFrame_Scripts.cpp` `Script_GetSpellCooldown` and its
  `Spell_C_GetSpellCooldown` call; the action-bar raw-ID fallback belongs to the player book.
- The branch applies cleanly to main `7114694f`; only `spellbook.rs` and `wrath_spell.rs`
  differ from main. `git diff --check` passes.
- No Cargo, test execution, client launch or CI run was made. Compile and tests are **not run**.

## Named limits

- The host cooldown owner is B4's, unchanged; this branch only fixes the fallback shape and the
  catalog pre-gate.
- The donor 1.12 path keeps its existing book-slot resolution; the `"pet"` rejection is the
  Wrath action-bar fallback only.

## Owner check list

1. Put a spell on the bar, relog, and hover/click it: the cooldown starts and matches the
   spellbook's cooldown for the same spell.
2. Cast it twice: the second press is refused while the cooldown is live; no cold/no-cooldown
   answer.
3. A macro or pet spell on the bar must not read the pet book's cooldown for a player spell.
4. A spell with no displayed book slot still formats `GetSpellLink`/`GetSpellTexture` and its
   cooldown from the raw action-bar ID.
