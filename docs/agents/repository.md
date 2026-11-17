# RustWrath repository

The owner selected https://github.com/Dicetar/RustWrath as the canonical project repository
on 2026-10-31. All agents, including Astra and PC/OC, use this repository for future work.

- Existing checkout: `D:/Projects/wow/client`. It moved there on 2026-11-17 from
  `D:/Projects/project-b-client`, which remains as a junction to the same folder, so launchers,
  asset profiles, local state and dated handoffs that name the old path keep working.
- Workstation layout under `D:/Projects/wow`: `client` (this checkout), `worktrees` (task
  worktrees, formerly `D:/Projects/rw-worktrees`), `worktrees-legacy` (older loose worktrees),
  `server` (AzerothCore lab, game install and wm-project, formerly `D:/WOW`), `reference`
  (preserved decompilation and Elden Ring references) and `backups`. `D:/WOW` is also a
  junction; the server build tree was configured under that spelling. Remove a junction only
  with `cmd /c rmdir`, never with a recursive delete.
- `origin`: `https://github.com/Dicetar/RustWrath.git`, the owner's project.
- `upstream`: `https://github.com/samwhosung/benilla.git`, the original donor. Do not push
  Project B changes there.
- `wotlk`: the existing shared development branch. Track `origin/wotlk` after migration.
- `main`: the intended published baseline. The migration preserved the destination's initial
  README commit and existing donor/fork history. Publication uses focused task branches
  that the integrator lands on `main`, by fast-forward or merge, or by pull request when the
  owner asks for one ([sync](sync.md)); do not force-push shared branches.
- The repository is public. Branches, task notes and issues are world-readable.

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
