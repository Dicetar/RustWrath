# Session failure recorder

Status: in progress
Owner: Pi
Branch: pi/session-failure-recorder
Issue: none
Next: Read branch CI gates/windows/player-tests and repair failures.

Adds session-thread bounded inbound/outbound rings (256/64, 512-byte prefixes), post-world-auth only, full error-chain report and ten-file retention. All direct session sends/reads and multiclass sends use the recorder. Explicit reducer, socket read/send, reply-backlog, multiclass receive/dispatch and response/logout timeout paths.

Verification: source inspection and rustfmt only; CI pending. Three focused tests added. No local Cargo/client/server run. Own target directory reserved as D:/Projects/wow/worktrees/pi-session-failure-recorder/target; no target created.

12340: instrumentation only; no wire layouts/behavior changed. Authentication boundary follows existing AUTH_OK handling in wrath.rs. No reference implementation copied.

Unproven in game: actual disconnect diagnosis, file write permissions at the player's install, packet-path overhead under live load. Reports contain gameplay packet payload prefixes; keep local, never upload them to the public repository. Account/session-key authentication packets are excluded.
