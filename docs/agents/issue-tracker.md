# Issue tracker

## Claims: GitHub issues

Who is working on what is recorded in GitHub issues on
https://github.com/Dicetar/RustWrath/issues, because every tool and cloud agent can read and
write them. One issue per task. The issue names the owner and the branch; the detail lives in the
task note on that branch ([sync](sync.md)).

```
gh issue create --title "<task>" --body "Owner: <tool and model>. Branch: <type>/<slug>."
gh issue comment <n> --body "Taking this over. Owner: <tool and model>."
gh issue close <n> --comment "Landed on main as <commit>."
```

Issues are public. Keep them to the task, owner, branch and outcome.

## Tickets: local markdown

Design tickets are one file per ticket under `.scratch/<feature>/issues/<NN>-<slug>.md`.
Current feature: `project-b`. Numbered in dependency order; "Blocked by" cites numbers/titles.
Working the frontier = any ticket whose blockers are all done. No parent issues. A GitHub issue
that claims a ticket cites its file.

## Wayfinding operations

Not in use for this repo. Project B was charted through planning rounds whose adopted
outcomes live in [owner decisions](../baseline/owner-decisions.md) and the ADRs.
