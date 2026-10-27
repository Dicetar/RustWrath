# Eight authorized Wrath regression cases and one verification pass

Explicit human grant: add up to eight focused cases in existing UI source files, one cargo
check and one benilla-ui library test invocation on a pinned snapshot, maximum 30 minutes
including case authoring. No reruns, app/game/graphical/socket/WM/server/DB operations,
installs or deployment. The human also explicitly required buy-in before complexity for
speculative race/security edges. This pass tests committed behavior and adds no such hardening.

Exactly eight cases were added to script/target.rs in b52ad4d3. No new test file was created.
The initial post-grant clock read was 2026-10-27 20:21:50 Europe/Moscow (17:21:50 UTC).
Commands started 20:24:29.394 and finished 20:28:09.235: 219.8 seconds, about 3m40s of
execution, approximately 6m19s from the initial clock read including case authoring/setup.
The later source fix and cleanup were also completed before the 30-minute deadline. A fixed
17:50:00 UTC execution deadline left headroom within that grant; it was not reached.

Pin: b52ad4d3f3c133e2de05b1b8321f2cb804a806e1, clean detached checkout
F:/AI/tmp/project-b-astra-check-20261027-ui4. Concurrent dirty owner work in app/Lua/UI files
was excluded; no source under that checkout changed during the pass.

Commands, each exactly once, locked/offline with existing shared target directory and Rustup
update checks disabled. Credential/capture environment entries were removed without printing
their values; Cargo helpers launched hidden.

1. cargo check --locked --offline -p benilla-ui: exit 0, 14.18s reported by Cargo.
2. cargo test --locked --offline -p benilla-ui --lib: exit 101; 3,329 passed, three failed,
   one ignored, zero filtered. Compilation 3m13s, suite runtime 5.80s. Two existing UI atomic
   deprecation warnings; no formatter/linter/fix command or other test invocation ran.

## New case results at the pin

All eight live under script::target::wrath_regressions:

| Case | Result |
|---|---|
| observed_class_labels_preserve_tables_and_reject_invalid_replacement | PASS |
| wrath_table_boundaries_and_binary_formats_follow_the_selected_dialect | PASS |
| event_queries_keep_wrapper_identity_order_and_unregister_membership | PASS |
| mouse_over_uses_scaled_region_geometry_without_mouse_enablement | PASS |
| installed_mod_setters_reuse_visibility_and_button_state | FAIL |
| disabled_motion_delivers_tooltip_scripts_without_enabling_clicks_or_visuals | PASS |
| motion_xml_keeps_inherited_true_and_honors_explicit_false | PASS |
| leave_can_admit_next_disabled_button_and_enter_can_enable_it_once | PASS |

The failed case reaches its first texture SetShown call and errors that the method is nil.
The full region table contained the callback, but leaf construction copied only fixed donor
method lists, so Texture and FontString never received it. Commit 67511a2a explicitly publishes
that same existing callback to both Wrath leaf tables, leaving title/vanilla tables alone.
The original failing test is unchanged. This is a reproduced registration omission, not
speculative hardening. The repair is SOURCE-ONLY: no check or test was rerun after it.

The other two failures are the same retained timezone expectations:
- date_and_time_read_the_local_zone: date("%H:%M", 0) gives 00:00, expected 05:45.
- time_converts_a_date_table: isdst=true gives 999996400, expected 1000000000.

Raw evidence: run.log, check.stdout.log, check.stderr.log, tests.stdout.log, tests.stderr.log.
The orchestration session completed; no verification process remains live. The temporary
checkout's exact resolved absolute path, pinned revision and clean status were checked before
git worktree remove without force; absence was confirmed afterward. Shared checkout and owner
WIP were preserved. Grant consumed; no ninth case or rerun was added.

These results establish compilation and the named CPU-side regression outcomes at the pin.
They do not establish the unexecuted SetShown repair, every flag/coercion/boundary case,
non-English DBC data, real catalog/profile bootstrap, exact 12340 ABI, installed-addon workflows,
secure-frame authority, graphical/live gameplay, WM consumption or full stock-client
replacement. The overall campaign is incomplete. Latest goal accounting during this pass
reported usageLimited and 13,886 active seconds (about 3h51), not six active hours; no completed
six-hour campaign or release acceptance is claimed.
