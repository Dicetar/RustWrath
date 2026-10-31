# RustWrath repository

The owner selected https://github.com/Dicetar/RustWrath as the canonical project repository
on 2026-10-31. All agents, including Astra and PC/OC, use this repository for future work.

- Existing checkout: `D:/Projects/project-b-client`. The directory name does not need to
  change; existing launchers, asset profiles and local state keep working from this path.
- `origin`: `https://github.com/Dicetar/RustWrath.git`, the owner's project.
- `upstream`: `https://github.com/samwhosung/benilla.git`, the original donor. Do not push
  Project B changes there.
- `wotlk`: the existing shared development branch. Track `origin/wotlk` after migration.
- `main`: the published project baseline. The migration preserves the destination's initial
  README commit and the existing donor/fork history. Future changes use focused branches
  and pull requests to `main`; do not force-push shared branches.

Before committing, inspect HEAD, the index and dirty files. Stage explicit owned paths only.
Do not stage another agent's unfinished changes or use `git add .`. Fetch before publishing;
coordinate shared-branch updates and preserve concurrent commits. Do not pull, rebase or
switch branches underneath a dirty shared checkout.

The migration publishes committed source history, not a claim that the dirty working tree
builds or that playable alpha is accepted. Unfinished combat, graphics and hotpatch work stays
in this checkout until its owner commits it. Current work ownership remains in
`docs/agents/active-ownership.md` when present.

Never upload `.probe-identity`, account credentials, `benilla-config`, `target`, extracted game
assets, MPQ archives, local run dumps, or the preserved decompilation checkout. Local verification
directories are ignored; previously tracked Markdown summaries remain part of history.
The user's install and decompilation remain separate read-only runtime/research inputs.
