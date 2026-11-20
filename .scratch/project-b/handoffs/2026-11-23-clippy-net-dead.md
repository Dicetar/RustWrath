# Clippy: remove the dead net sink and typed quest-push command

Status: ready to merge
Owner: OpenCode DeepSeek overseer
Branch: oc/clippy-net-dead
Issue: none
Next: land after green CI

## What changed

- `NetEventSink` (a `Sender<SessionEvent>` resource) had no reader and no sender use; the
  resource, its insertion and the now-unused `NetHandles::events_out` field/clone are removed.
- `ClientCommand::QuestPushResultForQuest` had no constructor anywhere in the workspace (only a
  match arm in the Wrath tap and a no-op in the 1.12 writer); the variant and both arms are
  removed. The existing `QuestPushResult` variant and its refusal remain unchanged.
- No behavior change: both were unreachable dead API.

## Checks

No local Cargo/build/test/client/server; CI is the compile authority.

## Not proven

Only the lint removal and the absence of callers by workspace grep; no runtime change.
