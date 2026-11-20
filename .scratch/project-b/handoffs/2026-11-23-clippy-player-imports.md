# Player-clippy debt: crates/benilla-app/src/player.rs

Status: ready to merge
Owner: Flint 40B Free
Branch: oc/clippy-player-imports
Issue: none
Next: land after green CI

## What changed
- `crates/benilla-app/src/player.rs`: the `pub(crate) use controller::{...}` re-export block is now
  `#[cfg(feature = "hotpatch")]`.

  The finding listed the block's names as unused imports (`MODULE_OWNED_FLAGS`, `MODULE_SPEED_SLACK`,
  `MODULE_STEP_FLOOR`, `MODULE_STEP_SLACK`, `MovementBaseline`, `MovementCorrection`,
  `MovementIntentRejection`, `MovementIntentVerdict`, `MovementProcessorOutcome`,
  `MovementProcessorRefusal`, `MovementProcessorState`, `MovementProcessorTick`, `MovementProcessor`,
  `MovementProposal`). The player build the player-clippy gate lints is
  `cargo clippy -p benilla -p benilla-app --no-default-features`, the shipped library without the
  `dev`/`hotpatch` features, so the bridge module that was the block's only consumer (`mod hotpatch`,
  itself `#[cfg(feature = "hotpatch")]`) is compiled out and the re-exports were unused there.

  The gate keeps the feature-gated re-export, so the hotpatch bridge's `use crate::player::{...}`
  still resolves when `hotpatch` is on; for the shipped player build the names are no longer imported.

## Checks not run

- No Cargo, no build, no tests, no client/server (per task and owner policy). CI is the compile and
  clippy authority.

## What is not proven

- A local clippy run; the green player-clippy gate is what proves the findings are gone. The
  feature-gated hotpatch build still carrying the re-export was verified by source reading of the
  only consumer, not by a compile. No client or server run, no live gameplay.
