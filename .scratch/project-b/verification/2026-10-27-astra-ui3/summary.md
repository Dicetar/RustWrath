# Authorized class-list and Lua compatibility verification

Explicit human grant: one bounded cargo check plus existing benilla-lua, benilla-mlua and
benilla-ui tests, maximum 20 minutes total. No new test cases, reruns, app/game/graphical/socket/
WM/server/DB execution, installs or deployment. Grant consumed exactly once.

Pinned source: f70be14286e9c7e088820ad4c67d9b0b7493f9e1 in a clean detached temporary checkout
at F:/AI/tmp/project-b-astra-check-20261027-ui3. This includes the real-data class-list read
projection, Wrath locale mapping, table-library boundary repairs and string-format repairs.
The later event-admission addition 2318d773 is excluded and remains source-only.

Started 2026-10-27 17:04:16.120 Europe/Moscow (14:04:16.120 UTC).
Finished 17:08:48.326 Europe/Moscow (14:08:48.326 UTC).
Total 272.2 seconds, approximately 4m32s, below 20 minutes; no timeout. The process used a
shared 19-minute execution deadline to leave setup headroom within the grant.

Commands, each exactly once, with --locked --offline, the existing shared target directory,
Cargo offline mode and Rustup update checks disabled. Credential/capture environment entries
were removed without printing their values. Cargo processes were launched hidden.

1. cargo check --locked --offline -p benilla-lua -p benilla-mlua -p benilla-ui:
   exit 0, 13.78 seconds reported by Cargo. Two existing UI atomic-method deprecation warnings;
   no formatter, linter or fix command ran.
2. cargo test --locked --offline -p benilla-lua -p benilla-mlua -p benilla-ui:
   exit 101. Completed Lua/mlua unit/integration suites passed. UI library: 3,322 passed,
   two failed, one ignored, 2.76 seconds for that suite. Test-profile compilation took 3m26s.
   Cargo stopped at the UI library failure; later targets/doctests were not reached.

The two failures match the previously retained timezone/date failures:

- script::tests::stdlib::date_and_time_read_the_local_zone:
  date("%H:%M", 0) returned "00:00", expected "05:45".
- script::tests::stdlib::time_converts_a_date_table:
  isdst=true produced "999996400", expected "1000000000".

No assertions, test cases or test files were added or changed for this pass. No rerun followed.
Raw evidence beside this summary: run.log, check.stdout.log, check.stderr.log, tests.stdout.log,
tests.stderr.log. The orchestration session completed; no verification process remains live.
The temporary checkout's exact resolved absolute path, clean status and pinned revision were
checked before git worktree remove, without force. Its absence was confirmed afterward.
The shared checkout and unrelated untracked owner files were preserved.

Evidence scope: compilation and results of existing suites at the pin. The inherited suites
do not establish direct semantic coverage of the new Wrath bindings/branches, new allocation
failure paths or real catalog publication. FillLocalizedClassList still needs OC's catalog
bootstrap before Constants.lua; the failed real-data app smoke was not rerun. Neither this
pass nor the later listener-list addition proves stock UI startup, installed-mod compatibility,
producer/payload coverage, rendered gameplay, production WM acceptance or full stock-client
replacement. All those acceptance gates remain open.
