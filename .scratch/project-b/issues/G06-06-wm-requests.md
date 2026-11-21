# G06-06 — WM action requests

Status: proposed, unassigned. Derived from the G06 source inventory on 2026-11-03.
Blocked by: approval of this channel's capacity and overload contract; explicit test assignment
before execution. No implementation or stress result is claimed by creating this ticket.

Owner seam: `wrath.rs::wrath_wm_admit; benilla-wm-native/src/publisher.rs::pump_actions` (app paths relative to crates/benilla-app/src).

Scope: this channel's producer, consumer, lifecycle and caller refusal/retention behavior.
Preserve existing version boundaries and other owners' dirty work. Do not convert a render
thread send to a blocking operation or silently drop gameplay/ownership messages.

Acceptance: Stop app admission under a request burst; bound queued bytes/count and per-frame drain, return explicit refusal through the broker, and preserve the existing 16 in-flight cap.

Evidence required: source/diff, chosen capacity assumptions, authorized stopped-consumer
input/output and memory/queue counters. No full workspace suite or live account action is
authorized by this proposal. See ../handoffs/2026-11-03-g06-queue-inventory.md.
