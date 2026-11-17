# Agent sync protocol

Status: ready to merge
Owner: Claude Code / Fable 5.2
Branch: docs/agent-sync
Issue: none
Next: the other tools start following docs/agents/sync.md; the owner decides the stranded local work listed below

Date 2026-11-15. Requested by the owner, who was carrying pushes, pulls and handoffs between
OpenCode, Codex, a Google cloud agent and Claude Code by hand.

## What exists now

- `docs/agents/sync.md`: the loop every agent follows. A task branch in the agent's own worktree
  or clone, a task note with a five-line header committed on that branch, a push at every
  stopping point whether or not the work is finished, and one integrator landing `ready to
  merge` branches on `main`.
- `scripts/agent-tasks.sh`: lists each remote branch that holds commits `main` lacks, with the
  status, owner and next step from its task note. `--merged` lists branches already contained in
  `main`. Needs only git.
- `AGENTS.md`, `docs/agents/workflow.md`, `docs/agents/repository.md`,
  `docs/agents/issue-tracker.md`: point at the loop. Claims go in GitHub issues; design tickets
  stay as local markdown. The publication rule now names the integrator instead of a pull request.
- `CLAUDE.md` and `GEMINI.md`: pointers to `AGENTS.md`, so Claude Code and the Gemini CLI load
  the same rules as OpenCode and Codex.
- `.github/ISSUE_TEMPLATE/task.yml`: a claim form.
- 26 handoff notes that existed only on the workstation are now tracked beside the 43 that
  already were. They were checked for credentials, addresses, account names and pasted interface
  code before being added; none were found.

## Checks run

- `scripts/agent-tasks.sh --no-fetch`, `--merged`, `--help` and an unknown argument, in Git Bash
  on the workstation. No Cargo, no client.
- After the push, the script listed this branch with its status, owner and next step.
- Not run: the script in a Linux cloud sandbox.

## Found while setting this up

- All 78 branches on `origin` other than `main` are already contained in `main`. They can be
  deleted; nothing was deleted.
- Local work that is on no remote, each owned by another session and left untouched:
  `fix/wrath-glue-runtime` (5 commits, 14 Nov, never pushed), `D:/Projects/rw-movement`
  (20 modified files, uncommitted, on `fix/wrath-server-movement-a`), `D:/Projects/rw-transfer-abort`
  (10 modified files, uncommitted), `D:/Projects/project-b-client-claude-engine` (1 modified file).
- CI cannot yet serve as the compile check for agents without the workstation: it runs only on
  `wotlk` pushes and pull requests, the `gates` job fails at `cargo fmt --check`, and the
  `windows` job failed in `test` on the last `wotlk` runs.
- The triage labels named in `docs/agents/triage-labels.md` do not exist on GitHub.

## Not done

- No GitHub issue was created and no label was added.
- No remote branch or worktree was deleted.
- The CI triggers and the formatting failure were left as they are.
