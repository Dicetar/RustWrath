# G06-07 — WM cancellation IDs

Status: proposed, unassigned. Derived from the G06 source inventory on 2026-11-03.
Blocked by: approval of this channel's capacity and overload contract; explicit test assignment
before execution. No implementation or stress result is claimed by creating this ticket.

Owner seam: `wrath.rs::wrath_wm_admit` (app paths relative to crates/benilla-app/src).

Scope: this channel's producer, consumer, lifecycle and caller refusal/retention behavior.
Preserve existing version boundaries and other owners' dirty work. Do not convert a render
thread send to a blocking operation or silently drop gameplay/ownership messages.

Acceptance: Burst cancels before matching requests with a stopped consumer; bounded retained IDs and expiry must not turn an acknowledged cancellation into an admitted action.

Evidence required: source/diff, chosen capacity assumptions, authorized stopped-consumer
input/output and memory/queue counters. No full workspace suite or live account action is
authorized by this proposal. See ../handoffs/2026-11-03-g06-queue-inventory.md.
