# Spirit-healer POI follow-through

Status: landed
Owner: ClaudeJr / Claude Opus 5.5
Branch: jr/spirit-healer-followthrough
Issue: none
Next: Owner in-game check: die, confirm the spirit-healer blip, release and follow it.

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
- CI: run 44843666172 on 48ed1118f (branch merged with origin/main f82c180f3): windows and gates both success.
- Local Cargo, client launch and server launch: not run per ticket rules.
- In-game verification: not proven.
