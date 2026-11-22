# Session failure recorder

Status: ready to merge
Owner: Pi
Branch: pi/session-failure-recorder
Issue: none
Next: Coordinator review and merge; owner checks the next real disconnect's local report.

Adds session-thread bounded inbound/outbound rings (256/64, 512-byte prefixes), post-world-auth only, full error-chain report and ten-file retention. All direct session sends/reads and multiclass sends use the recorder. Explicit reducer, socket read/send, reply-backlog, multiclass receive/dispatch and response/logout timeout paths.

Verification: CI 45002983208 (source 20450e2a9): gates and windows green. All three recorder cases passed in Linux tests, Windows tests, and player-tests (verified in run logs). player-tests actually ran: 6854 + 591 passed, zero failures. Covers ring bounds/prefix truncation, authentication exclusion, a real malformed LOGIN_VERIFY_WORLD reducer error's report/full cause chain, and ten-file retention. Earlier CI 45002894052 was superseded/cancelled, not a source failure. Source inspection, rustfmt and git diff --check passed. No local Cargo/client/server run. Own CARGO_TARGET_DIR designated D:/Projects/wow/worktrees/pi-session-failure-recorder/target; no Cargo invocation or target directory created, so no target remains to delete. Final fetch: zero origin/main commits missing from this branch.

12340: instrumentation only; no wire layouts/behavior changed. Authentication boundary follows existing AUTH_OK handling in wrath.rs. No reference implementation copied.

Unproven in game: actual disconnect diagnosis, file write permissions at the player's install, packet-path overhead under live load. Reports contain gameplay packet payload prefixes; keep local, never upload them to the public repository. Account/session-key authentication packets are excluded.
