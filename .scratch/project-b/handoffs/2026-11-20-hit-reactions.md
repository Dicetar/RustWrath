# Creature hit reactions

Status: landed
Owner: Astra / OpenAI
Branch: feat/astra-night2-hit-react
Issue: none
Next: Owner verifies in game.

## What changed
Added traceable wound flinches (450 ms cooldown) and interrupts (1800 ms cooldown) at a third of maximum stance. A stance break takes precedence. RESOLVE interrupted clears the pending strike without dealing damage.

## CI run id and result
44470651224: Windows workspace check and Linux scoped tests passed.

## What is not proven in game
ALE did not run; wound emote layering and repeated interruptions remain unverified.
