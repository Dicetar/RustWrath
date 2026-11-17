# Keeping agents in step

Several tools work on RustWrath at once: OpenCode, Codex, Claude Code, Gemini, and cloud agents
that never see this workstation. They share one thing: `origin`
(https://github.com/Dicetar/RustWrath). Work that is not on `origin` does not exist for the
other agents, so the owner should never have to carry it between them by hand.

`origin` is a public repository. A branch, a task note and an issue are all world-readable.

## The loop

1. **Look.** `git fetch --prune origin`, then `scripts/agent-tasks.sh` to see every unmerged
   branch, its status and its next step. Read the note of anything near your task.
2. **Claim.** One task, one owner, one branch. Open or comment on a GitHub issue naming the
   agent and the branch; the commands are in the [issue tracker](issue-tracker.md). A sandbox
   without `gh` skips the issue; the pushed branch and its note are the claim.
3. **Branch.** Start from `origin/main` in your own worktree or clone:
   `git worktree add -b <type>/<slug> <path> origin/main`. `D:/Projects/wow/client` is the
   integration checkout; do not edit, build or switch branches in it.
4. **Write the task note first.** `.scratch/project-b/handoffs/<date>-<slug>.md`, committed on
   the branch, starting with the header below. Push it with the first commit.
5. **Check in at every stopping point.** When a step is done, when the session or quota is about
   to end, and before waiting on the owner: update the note, commit your own paths, push the
   branch. Unfinished work is welcome on a task branch; see the rules below.
6. **Finish.** Set `Status: ready to merge`, push, and say so on the issue. The integrator lands
   it on `main`.

## The task note

The first lines of the note are read by `scripts/agent-tasks.sh`. Keep them current; they are
the only status another agent will see.

```
# <title>

Status: in progress | ready to merge | blocked | parked
Owner: <tool and model, for example "OpenCode / DeepSeek">
Branch: <type>/<slug>
Issue: <number, or none>
Next: <the next concrete step, in one line>
```

Below the header, keep what the [workflow](workflow.md) already asks of a handoff: what exists
now, which checks were actually run and their result, what is not done or not proven, and the
next step in enough detail that a different agent can take it without asking. One note per task;
update it in place rather than adding a new file per session.

## Unfinished work

- A task branch may hold commits that do not compile or pass. Start their subject with `wip:` and
  state in the note what is broken or unrun. `main` only receives work whose source is finished
  and reviewed; see [landing](#landing-on-main) for what that does not prove.
- Stage your own paths by name. No `git add .`, no generated or game files; see the
  [repository rules](repository.md).
- Do not rewrite a pushed branch. No force-push, no rebase of commits another agent may have
  fetched. Bring in `main` with a merge when you need it.
- A cloud agent has no game data, no client and no local Cargo slot. It works on source and
  focused tests that need no install, and reports data-gated checks as unrun.

## Taking over a branch

Fetch, read the note, check the issue for a newer claim, then comment that you are taking it and
change `Owner:` in the note in your first commit. Continue on the same branch. If the previous
owner may still be running, ask through the issue or AutoSwarm instead of pushing alongside it.

`Status: parked` means the owner has stopped and anyone may take the branch. `blocked` names what
it waits for in `Next:`.

## Landing on main

The owner builds and launches the client by hand, with `Launch RustWrath.cmd` in the integration
checkout, after a batch of changes has landed. That script builds what is on `main` in that
checkout and nothing else. Agents do not run Cargo or start the client unless the owner asks for
it in that session.

So a branch lands on a source review, not on a build. One integrator at a time lands branches,
when the owner asks for a batch. For each `ready to merge` branch it reads the diff and the note,
checks the change against its reference and that what it calls exists, merges or fast-forwards
into `main`, pushes, and deletes the remote branch and its worktree. It leaves a branch alone
when its worktree holds another session's uncommitted work, and lands only what is pushed. The
owner may ask for a pull request instead.

Say in the merge commit and in the note what was not compiled or run. `main` can therefore fail
to build until the owner's next batch. A build or test failure from that batch is the next task:
fix it on a new branch, with the error text in the note.

After landing, the integrator fast-forwards the integration checkout. Other agents start their
next task from the new `origin/main`.

`scripts/agent-tasks.sh --merged` lists remote branches already contained in `main`; they can be
deleted.

## What stays on the workstation

Cargo and client slots, PIDs and run reservations are local facts and stay in
[active ownership](active-ownership.md). Build and test logs, diagnostics, game files, extracted
interface files, `benilla-config`, credentials, account and character-login details and server
addresses are never committed and never pasted into an issue. A note describes a local result in
words and names the log file; it does not include it.
