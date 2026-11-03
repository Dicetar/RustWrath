# RustWrath repository

The owner selected https://github.com/Dicetar/RustWrath as the canonical project repository
on 2026-10-31. All agents, including Astra and PC/OC, use this repository for future work.

- Existing checkout: `D:/Projects/project-b-client`. The directory name does not need to
  change; existing launchers, asset profiles and local state keep working from this path.
- `origin`: `https://github.com/Dicetar/RustWrath.git`, the owner's project.
- `upstream`: `https://github.com/samwhosung/benilla.git`, the original donor. Do not push
  Project B changes there.
- `wotlk`: the existing shared development branch. Track `origin/wotlk` after migration.
- `main`: the intended published baseline. The migration preserved the destination's initial
  README commit and existing donor/fork history. Publication uses focused branches
  and pull requests to `main`; do not force-push shared branches.

Before committing, inspect HEAD, the index and dirty files. Stage explicit owned paths only.
Do not stage another agent's unfinished changes or use `git add .`. Fetch before publishing;
coordinate shared-branch updates and preserve concurrent commits. Do not pull, rebase or
switch branches underneath a dirty shared checkout.

Testing an existing local binary needs no Git operation. A commit does not compile source
or activate a hotpatch, and a push does not update the local executable. Use the
[launch guide](../launching.md) and [coordination workflow](workflow.md).

Keep the native Cargo workspace and React/Vite web preview distinct during reviews and
staging. A web build cannot validate the native client. Inspect untracked root files such
as FETCH_HEAD before staging; a loose command artifact is not Git metadata.

Published history does not prove that a dirty working tree builds or that playable alpha
is accepted. Integrate another agent's work only after reviewing its diffs and evidence.
Current reservations belong in [active-ownership.md](active-ownership.md); completed
implementation and runtime results belong in the owning handoff.

Never upload `.probe-identity`, account credentials, `benilla-config`, `target`, extracted game
assets, MPQ archives, local run dumps, or the preserved decompilation checkout. Local verification
directories are ignored; previously tracked Markdown summaries remain part of history.
The user's install and decompilation remain separate read-only runtime/research inputs.
