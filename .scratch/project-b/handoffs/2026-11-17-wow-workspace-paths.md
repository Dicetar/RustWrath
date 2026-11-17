# Workstation paths moved under D:/Projects/wow

Status: pushed on the task branch, not landed; not compiled, not run
Owner: Claude Code (Fable 5.2)
Branch: docs/wow-workspace-paths
Issue: none (owner request 2026-11-17: move all WoW work into `D:/Projects/wow`)
Next: the integrator lands this on `main`. The owner's next batch build confirms the two Rust
files still compile and shows whether the new checkout path costs one full rebuild.

Date 2026-11-17. Worktree `D:/Projects/wow/worktrees/docs-wow-workspace-paths`, cut from
`origin/main` `ae7eef7d`. No Cargo check, test or build was run and the client was not started.

## What moved on the workstation

Same-volume renames, nothing copied. Every old path below except the loose legacy worktrees
is now a junction to the new folder, so old spellings in dated notes and scripts still resolve.

| Old path | New path |
|---|---|
| `D:/Projects/project-b-client` | `D:/Projects/wow/client` |
| `D:/Projects/rw-worktrees` | `D:/Projects/wow/worktrees` |
| loose `D:/Projects/rw-*`, `project-b-*`, `rustwrath-*` worktrees | `D:/Projects/wow/worktrees-legacy/<name>` |
| `D:/WOW` | `D:/Projects/wow/server` |
| `D:/Projects/wowdecomp-reference` | `D:/Projects/wow/reference/wowdecomp-reference` |
| `D:/Projects/elden-ring-combat-rewrite-reference` | `D:/Projects/wow/reference/elden-ring-combat-rewrite-reference` |
| `D:/Projects/er-unpack-tools` | `D:/Projects/wow/reference/er-unpack-tools` |
| `D:/Projects/RustWrath-backups` | `D:/Projects/wow/backups` |

Git worktree links of this repository (130 worktrees, including the OpenCode ones on `C:` and
`F:`) were repaired and point at `D:/Projects/wow/client/.git`. The backup scheduled task,
`WOW_DATA` and the Start Menu shortcut use the new paths.

## What this branch changes

- 55 path literals in 32 live files rewritten to the new layout: `README.md`, `AGENTS.md`,
  `CONTEXT.md`, the guides under `docs/`, `src/App.tsx`, and the defaults in
  `tools/backup-project.ps1` (`$Repository`, `$Destination`) and `tools/launch-client.ps1`
  (`$DataPath`, `$MulticlassGluePath`).
- Rust: the default AddOns folder literal in `crates/benilla-app/src/ui_script/addons.rs`, and
  doc comments in `crates/benilla-protocol-wrath/src/session.rs`, `gameplay/social.rs` and
  `gameplay/wrath_systems.rs`. No logic changed.
- `docs/agents/repository.md` records the move, the layout and the junction rule.

## Left as written

- Dated records: everything under `.scratch/`, `docs/opencode/`, `public/assets/aistudio/`, and
  the files with a date in their name under `docs/architecture/`. Their old paths resolve
  through the junctions.
- `docs/baseline/mod-multiclass-drift.txt` (recorded diff output with `/d/WOW/...` paths).

## Not verified

- Compilation of the two touched crates.
- Whether Cargo or sccache treat `D:/Projects/wow/client` as a new location and rebuild once.
- A server incremental build. Its CMake cache holds `D:/WOW/...` paths, which the junction
  keeps valid; keep using that spelling for server builds.
