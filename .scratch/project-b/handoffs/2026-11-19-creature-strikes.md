# Creature-authored strike animations

Status: landed
Owner: Astra / OpenAI
Branch: feat/astra-creature-strikes
Issue: none
Next: Owner copies the repository Lua script and checks strike timing in game.

## What changed
STRIKE requests an authored model sequence in this order: Attack1H, Attack2H,
Attack2HL, AttackUnarmed, then the generic attack fallback.
The animation driver scales that clip's first impact key to the windup deadline;
a clip without an impact key uses its end. Timed strikes bypass combat deferral.
Ordinary action requests retain their existing rate and routing.
Removed the server's delayed generic attack emote so it cannot overwrite the clip.
Added pick-order/missing-sequence cases and removed the roadmap's open item.
Preserved main's creature rooting, HD rig and under-nameplate HUD changes.

## CI run id and result
44432259150: Linux app/formats tests pass, including animation selection; inherited UI failures were subsequently fixed on main.
44435863100: Windows workspace check and UI/protocol-wrath tests pass; Linux tests pass.
Untouched player-clippy debt remains red. No local Cargo or Lua/server execution.

## What is not proven in game
Creature model coverage, impact alignment, latency, interruption and the generic
fallback's appearance. The server installation was not changed.
