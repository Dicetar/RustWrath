# Authorized committed Lua/UI verification

Explicit human grant: one bounded cargo check plus existing benilla-lua, benilla-mlua and
benilla-ui tests, maximum 30 minutes total. No new test cases, application check/run,
game/graphical/socket/WM/server/DB execution, installs or deployment. Grant consumed.

Pinned source: `29ff13f982256a813c07227296bb9823f99cbee3` in a clean temporary detached
checkout at `F:/AI/tmp/project-b-astra-check-20261027-ui2`. This includes the committed string
helpers, their allocation repair, GetCVarBool and census path fix. It excludes the then-pending
three aura UI consumer files, subsequently committed separately as `c002a798`.

Started 2026-10-27 12:41:00.331 Europe/Moscow (09:41:00.331 UTC).
Finished 12:44:57.988 Europe/Moscow (09:44:57.988 UTC).
Total 237.7 seconds, approximately 3m58s, below the shared 30-minute deadline; no timeout.

Commands, each exactly once, with `--locked --offline`, shared existing target directory,
Cargo offline mode, Rustup update checks disabled and credential/capture environment entries removed without
printing their values:

1. `cargo check --locked --offline -p benilla-lua -p benilla-mlua -p benilla-ui`:
   exit 0. Two existing UI atomic-method deprecation warnings; no linter/fix command run.
2. `cargo test --locked --offline -p benilla-lua -p benilla-mlua -p benilla-ui`:
   exit 101. Completed Lua/mlua unit/integration suites passed. UI library: 3,322 passed,
   2 failed, 1 ignored. Cargo stopped at the UI library failure; later targets/doctests were
   not reached. No rerun or additional check followed.

Failures retained in `tests.stdout.log`:

- `script::tests::stdlib::date_and_time_read_the_local_zone`:
  `date("%H:%M", 0)` returned `00:00`, expected `05:45`.
- `script::tests::stdlib::time_converts_a_date_table`:
  the retained `isdst=true` table returned `999996400`, expected `1000000000`.

These match the two timezone failures and actual/expected values retained in the previous
2026-10-26 and 2026-10-27 passes. The census caller-path case now passes:
`script::census::tests::a_turns_send_made_while_a_handler_is_held_is_counted_under_its_caller`.
The earlier UI result was 3,321 passed / 3 failed / 1 ignored; no assertion was changed here.

Logs beside this summary: `run.log`, `check.stdout.log`, `check.stderr.log`,
`tests.stdout.log`, `tests.stderr.log`. Preserve failed evidence. The temporary checkout was
confirmed clean and removed without force after an exact resolved-path check. Shared source
and unrelated untracked owner files were preserved; the verification session is terminal.

Scope of evidence: compilation of these packages and completed existing-suite results at the
pin. No new helper-specific cases were created, and passing inherited suites does not prove
complete Wrath API semantics, installed-mod admission, allocation-failure behavior, the later
aura integration, the application, native/WM consumption, rendered gameplay or full stock-client
replacement. Those gates remain open.
