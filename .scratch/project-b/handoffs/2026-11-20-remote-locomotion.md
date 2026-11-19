# Remote Souls locomotion

Status: landed
Owner: Astra / OpenAI
Branch: feat/astra-night2-remote-loco
Issue: none
Next: Owner checks remote gait transitions in game.

## What changed
MODE keepalives and sprint changes announce nearby Souls peers. The shared remote pose cache picks idle, walk, run or sprint from streamed movement. Actions keep priority; stock casts, mounts and airborne poses are not replaced. The 16-body, 512-bone-per-body cap still applies.

## CI run id and result
44470837229: Windows workspace check and Linux scoped tests passed.

## What is not proven in game
Lease expiry, gait transitions, visual pose quality and busy-scene cost. No client or server launched.
