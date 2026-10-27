# Astra — one authorized check/test pass

Revision: e3c83d90405067ff88516b67e83f8fdc5f8a8ebb (includes OC's adc8a631 aura-availability repair).
Snapshot: detached checkout at F:/AI/tmp/project-b-astra-check-20261027, removed after confirming it clean.
Time: 2026-10-27 09:10:33–09:17:18 Europe/Moscow (06:10:33–06:17:18 UTC). Total 404.7 seconds against a shared 30-minute limit.

## Commands and results

1. `cargo check --locked --offline -p benilla-app`: exit 0, finished in 2m44s, with warnings. This includes compilation of the new Lua/UI code and the vendored parry dependency; it is not a linker, lint or graphical result.
2. `cargo test --locked --offline -p benilla-lua -p benilla-mlua -p benilla-ui`: run once, exit 101. Completed Lua/mlua unit/integration suites passed. The benilla-ui library suite reported **3321 passed, 3 failed, 1 ignored**. Cargo stopped on that failure; doctests/later targets were not reached.

## Failures retained

- `script::census::tests::a_turns_send_made_while_a_handler_is_held_is_counted_under_its_caller`: actual key `turn@crates\\benilla-ui\\src\\script\\census.rs:195`; expected `turn@script/census.rs:195`.
- `script::tests::stdlib::date_and_time_read_the_local_zone`: `date("%H:%M", 0)` returned `00:00`; expected `05:45`.
- `script::tests::stdlib::time_converts_a_date_table`: the `isdst=true` date table returned `999996400`; expected `1000000000`.

All three failure names and assertion values match the retained 2026-10-26 pass at 2b0b533be64bf8ec039855f41496995b53060abe. No claim is made that this establishes every other change free of regressions. No failure was fixed and nothing was rerun under this one-time grant.

## Evidence and limits

Sibling files: run.log, check.stdout.log, check.stderr.log, tests.stdout.log, tests.stderr.log. Source changes made while preparing OC's next prompt were documentation outside the snapshot; no source edits were made to the pinned checkout. Shared target artifacts were reused; offline/locked resolution was required. Capture variables and the WM observation token were removed from the child environment; no credential values were printed.

This pass did not run the app's Wrath UI slice, newly authored tests, a graphical/client/socket/receiver/WM/server process, a DB operation, formatter, linter or deployment. It does not prove new coroutine edge semantics, source-to-native observations, installed-mod behavior, stock Wrath host admission, collision attachment in a running renderer, or playable alpha. The broader full stock-client replacement gate remains open. The grant is consumed.
