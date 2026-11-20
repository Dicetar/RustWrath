# Astra bounded item/UI verification — 2026-10-28

The previously unused grant recorded in the Astra session handoff is **consumed**.
One invocation of each authorized command ran on Windows; no rerun, new test,
source edit during execution, app test, graphical/game/socket/WM/server/DB operation
or installed-addon load was performed.

## Source and budget

- Checkout: `D:/Projects/project-b-client`, branch `wotlk`.
- Before/after HEAD: `3e97bc4d17a76309f47a3e94a3511e6611243741`.
- Tracked worktree and index clean before and after; existing untracked evidence
  and `tools/patch_procgen_dungeon.py` preserved. No other worktree changed by Astra.
- Start: 2026-10-28 14:17:28.963 UTC / 17:17:28.963 Europe/Moscow.
- Shared deadline: 14:47:28.963 UTC / 17:47:28.963 Europe/Moscow (30 minutes).
- Completed: 14:17:50.695 UTC / 17:17:50.695 Europe/Moscow; total 21.8 seconds.
- Runner retained process IDs and enforced the deadline against its owned process tree.

## Commands and results

| Command | Exit | Result |
|---|---|---|
| `cargo check --locked --offline -p benilla-ui -p benilla-app --lib` | 0 | Passed with warnings; Cargo reported 16.55 seconds |
| `cargo test --locked --offline -p benilla-ui --lib` | 101 | 3337 passed, 2 failed, 1 ignored; test execution 2.85 seconds |

Raw evidence in this directory: [run.log](run.log),
[check.stdout.log](check.stdout.log), [check.stderr.log](check.stderr.log),
[tests.stdout.log](tests.stdout.log), [tests.stderr.log](tests.stderr.log).
Raw logs remain untracked owner artifacts; this summary is the tracked evidence index.

`layout::tests::assemble_clamp_low_edge` passed, exercising the existing e8298b54
repair. The only ignored case is `script::tests::dispatch_bench::dispatch_bench`
(explicit release/nocapture benchmark).

The two failures match the previously recorded test names:

- `script::tests::stdlib::date_and_time_read_the_local_zone`, stdlib.rs:591:
  `date("%H:%M", 0)` under `ZoneGuard::set("NPT-05:45")` returned `00:00`,
  expected `05:45`.
- `script::tests::stdlib::time_converts_a_date_table`, stdlib.rs:525:
  forced UTC, 2001-09-09 01:46:40 with `isdst=true` returned `999996400`,
  expected `1000000000`.

Source inspection found Windows uses CRT `tzset`, `localtime_s`, `get_timezone`
and `get_dstbias`; the test guard sets Rust's process `TZ`. This identifies the
platform seam to investigate, not a proven root cause or a repaired failure.
No timezone source or tests were changed. The suite is not green.

## Acceptance limits and next boundary

This establishes compilation of the pinned item/UI/app source and the stated
existing-suite results. It does not establish specific new Wrath item-query ABI,
real DBC admission, installed-addon initialization, rendered/live Northshire,
server link acceptance or full replacement. No new item/UI regression was exposed
by this pass; absence of a failure is not coverage of every recent source change.

OC retains the Questie animation constructor/GetParent repair and protocol/native
dependencies. Current animation.rs still requires CreateAnimation's type and has
no GetParent publication. Astra did not absorb those files. Further execution needs
a new applicable grant; the unused remainder of this budget does not allow a rerun.
