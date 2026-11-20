# Spirit-healer POI follow-through

Status: ready for CI
Owner: ClaudeJr / Claude Opus 5.5
Branch: jr/spirit-healer-followthrough
Issue: none
Next: Read CI for the pushed commit, then land after validation.

## Verification

- `SMSG_DEATH_RELEASE_LOC` is handled in `crates/benilla-app/src/death/net.rs`.
  `death_release_loc` stores `ReleaseLocation` only when the active player is not a ghost;
  the ghost path clears it, matching the 12340 `SetDeathReleaseLocation(-1, zero)` behavior.
- The existing focused test `a_release_location_is_kept_only_while_not_a_ghost` covers the
  non-ghost snapshot, ghost transition, and resurrection `-1` clear.
- `minimap::feed_minimap_arrows` reads the stored release snapshot, filters it to the current map,
  and rebuilds `MinimapSpiritHealer` only when the position/map identity changes.
- `emit_minimap` consumes `(Res<MinimapCorpse>, Res<MinimapSpiritHealer>)` as a grouped system
  parameter, and `blips::BlipInputs` groups the static blip inputs so the Bevy 16-parameter limit
  is respected.
- The existing `minimap::blips` tests cover spirit-healer landmark/rim-arrow selection.
- Branch was fast-forwarded to current `origin/main`; no source diff was required.
- Local Cargo, client launch and server launch: not run per ticket rules.
- In-game verification: not proven.
