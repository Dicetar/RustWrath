# Quest command delivery contract (G04/G05)

2026-11-03. Narrow implementation contract for the Wrath UI acceptance action, not approval
of ADR-0001 or the proposed diagnostics platform. Source reviewed; compile and runtime
acceptance are pending. Existing server packets and donor-mode commands are unchanged.

## Owners and boundaries

`QuestVerbs::accept` retains the existing guard, command channel, acted latch and teardown.
In Wrath mode it sends `ClientCommand::QuestgiverAcceptTracked`, carrying a
`CommandReceipt` with NPC, quest, session epoch, world generation and a private response
channel. The same `NetCommands` FIFO carries earlier and later actions; the UI does not
bypass it to insert directly into the outbox. `wrath_tap_commands` translates the command
into the ordinary accept packet and passes the receipt into `WrathOutbound`.
`run_session_with_cancel` owns the actual write; `QuestGiver::poll_accepts` consumes results
through the existing quest feed and error-message sink.

| Evidence | Producer | Meaning |
|---|---|---|
| ChannelAdmitted log | UI, after successful `send` | Local channel owns the command, not necessarily the outbox |
| Channel send error | UI | Disconnected receiver; leave panel and acted latch unchanged |
| `OutboxAdmitted` | Tap, under queue lock | Packet inserted in the bounded outbox |
| `Rejected(OutboxFull)` | Tap | No packet inserted; no later send of this action |
| `Rejected(ScopeChanged)` | Tap or writer | Obsolete epoch/world or no in-world writer; no send |
| `Rejected(Cancelled)` | Writer | Session stop requested before this packet's write; no send |
| `Rejected(Unavailable)` | Tap | Outbox mutex unavailable; no send |
| `Rejected(UnsupportedMode)` | Donor writer defensive branch | Tracked Wrath-only request rejected |
| `Written` | Wrath writer | Header and payload `write_all` returned successfully |
| `WriteFailed` | Wrath writer | Write failed, possibly after sending some bytes; server outcome unknown |
| Receipt receiver disconnected | Quest feed | Owner dropped the command/queue without a terminal receipt; write unconfirmed |
| Receipt still pending after 10 seconds | Quest feed | One stalled-delivery warning, not cancellation or proof of server failure |

`Written` is never a quest success notification. Only received server-backed quest state
and its journal projection can establish acceptance; that remains Q02. Delivery logs identify
the quest and epoch/world. Each response receiver belongs to one submission,
so it cannot consume a later submission's receipt. No local trace ID is added to WoW packets.

## Capacity, ordering and lifecycle

- At most 16 outstanding tracked UI accepts; a duplicate NPC/quest is refused while its
  receipt is outstanding. One bounded two-entry response channel per action fits admission
  plus one terminal result. Producers use `try_send`, never blocking the network thread.
- The existing command channel remains unbounded (G06 follow-up). This change bounds its
  tracked UI-accept contribution, not all command producers or all process memory.
- Outbox capacity remains 256. Movement retains its existing replacement policy and cannot
  evict a tracked acceptance. Earlier nonmovement actions remain ordered ahead of it.
- Both tap and writer check epoch/world. Writer also checks cancellation and `Phase::InWorld`. Queue clearing,
  worker errors and abandoned batches drop receipt senders; consumers distinguish missing
  confirmation from a successful write. No retries occur automatically.
- The 10-second warning is a UI observation default, not a measured latency budget. The
  receipt and duplicate guard stay alive afterward: freeing them would allow repeated
  submissions to accumulate behind a stalled consumer.
- Receipt state lives in the host's QuestGiver resource, outside the Lua VM. A new panel
  does not replace outstanding receipts. Existing session clearing does not erase them;
  results describe their original command and never mutate a newly opened panel.
- Quest row selection has one separate pending receipt. A confirmed local refusal clears
  the acted latch only if the original NPC and panel generation remain current. Uncertain
  delivery does not reset the latch or automatically resend the action.

## UI recovery policy

Immediate local refusal leaves the panel open and does not set `acted`. Successful channel
admission retains the normal immediate latch/close/tutorial behavior; it does not wait for
the server. A later outbox refusal displays a message directing the player to reopen the
questgiver. Reopening resets the ordinary latch. No asynchronous receipt reopens or closes
an unrelated panel. Write failure, lost receipt or a stall directs the player to inspect
the journal/connection, avoiding an automatic duplicate request with an uncertain outcome.
Native messages use the existing unkeyed error sink; localization is not added in this slice.

## Reference and review

Preserved reference pin `e67b6ffef23655dc7da061cb2d36b7963f6ef71a`:
`WoW/Source/UI/QuestFrame.cpp`, `CGQuestInfo::AcceptQuest`, OFFSET `0x0058CD20`:
offer/player/response guard, command, then response latch and panel close. Used as behavioral
evidence, not copied code. `benilla-protocol-wrath/src/world.rs::send` was inspected to
establish that Written follows both socket `write_all` calls.

Source review covers sender ownership, the two-result channel bound, FIFO insertion,
movement-only eviction, partial-write uncertainty and version/scope checks. Independent
review and focused execution remain outstanding. Planned cases: disconnected channel,
full outbox, valid write, partial write error, world change before tap/write, queue drop,
stalled consumer, and adjacent movement ordering. No tests were added or run.
