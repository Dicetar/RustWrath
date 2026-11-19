# Elden Ring clips for relayed Souls players

Status: landed
Owner: Astra / OpenAI
Branch: feat/astra-remote-clips
Issue: none
Next: Owner checks several remote players and generic fallbacks in game.

## What changed
Relayed SWING accepts an action id or legacy name, resolves it in our local kit,
and poses the remote rig from that row's ER source through the shared bake,
rig mapper, sampler and pose solver. Unknown actions retain generic animations;
missing clips, unsupported rigs and overflow also retain the generic fallback.
Each body blends into the clip and back to its underlying stock pose.
Bound: at most 16 cached remote bodies, at most 512 bones each, one sample/pose
solve per body per frame; maps are cached, the bake is shared, and per-frame
ancestor work is bounded by the bone cap (O(B²)). No per-body file loading.
Added id/name/unknown lookup coverage and updated local-only roadmap wording.
Remote locomotion remains stock; no remote movement prediction was added.

## CI run id and result
44432357768: Linux app tests pass, including relay lookup; Windows workspace check passes.
44435862646: Windows workspace check and UI/protocol-wrath tests pass; Linux tests pass.
Only untouched player-clippy debt remains red. No local Cargo/client/server run.

## What is not proven in game
Rig/weapon alignment, crowded-scene frame cost, fades and network latency.
The stated cap is a source bound, not a performance benchmark or visual acceptance.
