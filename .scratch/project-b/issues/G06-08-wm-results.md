# G06-08 — WM action results

Status: proposed, unassigned. Derived from the G06 source inventory on 2026-11-03.
Blocked by: approval of this channel's capacity and overload contract; explicit test assignment
before execution. No implementation or stress result is claimed by creating this ticket.

Owner seam: `wrath.rs::WrathWmActions::answer; publisher.rs::pump_actions` (app paths relative to crates/benilla-app/src).

Scope: this channel's producer, consumer, lifecycle and caller refusal/retention behavior.
Preserve existing version boundaries and other owners' dirty work. Do not convert a render
thread send to a blocking operation or silently drop gameplay/ownership messages.

Acceptance: Disconnect or stall the result consumer; terminal evidence stays bounded with explicit acknowledgement/expiry, and retries cannot invent or duplicate server outcomes.

Evidence required: source/diff, chosen capacity assumptions, authorized stopped-consumer
input/output and memory/queue counters. No full workspace suite or live account action is
authorized by this proposal. See ../handoffs/2026-11-03-g06-queue-inventory.md.
