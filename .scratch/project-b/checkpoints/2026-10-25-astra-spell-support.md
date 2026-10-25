# Astra support: WotLK spell-range and form catalogs

Owner authorized Astra to help implement Project B alongside OpenCode. OpenCode remains lead; this narrow support claim supersedes the earlier sole-writer rule only for the files listed here.

Status: SOURCE IMPLEMENTED; UNCOMPILED / UNTESTED. The owner subsequently authorized immediate scoped commits; this delivery is recorded by the commit containing this checkpoint and the two source files. No tests, builds, imports, probes or runtime checks were run or newly authorized for Astra by this assignment.

## File ownership for this slice

- `crates/benilla-formats/src/spells/ranges.rs`
- `crates/benilla-formats/src/spells/forms.rs`
- This checkpoint file.

Both source files were clean before Astra's edits. OpenCode's current gameplay/network/application changes are preserved and outside this task. Only the two source files and this checkpoint belong to Astra's delivery commit. No new agents or external session messages are part of this work.

## Outcome

The existing production loaders must recognize the explicit 1.12 and 3.3.5 layouts rather than treating changed columns as trailing fields. Preserve the vanilla path and errors for other layouts. Keep WotLK friendly and hostile spell ranges distinct without changing existing hostile/default callers. Adapt the form metadata's shifted flags, creature type and attack icon columns.

Sources: local AzerothCore `DBCStructure.h` (`SpellRangeEntry`, `SpellShapeshiftFormEntry`) and `DBCfmt.h` (`SpellRangefmt`, `SpellShapeshiftFormEntryfmt`); `SpellInfo.cpp::GetMinRange/GetMaxRange` selects the friendly/hostile pair. Existing vanilla loader schemas remain the source for the 1.12 layout.

This does not claim the main `Spell.dbc` catalog, spell-action validation or graphical gameplay is WotLK-complete. In particular, the main spell loader still has a 173-column vanilla schema; OpenCode should preserve that as a separate compatibility dependency.

## Delivered and integration limits

- `load_spell_ranges`: explicit 22-column vanilla / 40-column WotLK selection. WotLK hostile min/max/flags use columns 1/3/5; friendly min/max use 2/4. Other layouts fail with the file and expected field counts. The existing DBC parser still validates magic, record size and body bounds.
- `SpellRangeCatalog::get` keeps existing callers' behavior and selects the hostile pair for WotLK. `get_for_target(index, friendly)` exposes both pairs; vanilla and fixture-constructed catalogs use their one range for both. Existing callers need no signature change. OpenCode must select the appropriate pair in friendly-target validation/tooltips; that application policy is outside this two-file claim.
- `load_shapeshift_forms`: explicit 14-column vanilla / 35-column WotLK selection. WotLK flags/type/icon use 19/20/21. The bonus bar and localized name still begin at 1/2. No broad trailing-column acceptance or generic schema changes were added.
- Existing production callers in `ui_action::load_spells` already invoke these loader functions; no parallel loader/startup path was introduced. That caller first needs a working main spell catalog, so its full WotLK execution remains unproven.

Source review compared the mappings with `DBCStructure.h:1828`, `DBCStructure.h:1852`, the matching `DBCfmt.h` formats, and `SpellInfo.cpp:2714` onward. Reviewed the two-file Git diff and searched catalog construction sites; the only direct catalog constructors are in `ranges.rs`, and its fixture constructor was updated. This is source inspection, not executed validation.

When separately authorized, useful acceptance cases are: vanilla catalog behavior; a WotLK row whose friendly and hostile ranges differ; WotLK flags loaded from column 5 rather than a range float; a form with nonzero flags/type/icon after sixteen locale slots; malformed/unsupported row layout refusal. No new test files were created.

Astra's code edits are complete for this slice. OpenCode may now consume these two files and continue its integration work. No application/network/protocol/WM files were modified by Astra.
