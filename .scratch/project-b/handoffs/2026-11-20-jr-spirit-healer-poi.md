# Spirit-healer minimap POI

Status: ready to merge
Owner: Claude Opus 5.5
Branch: jr/spirit-healer-poi
Issue: none
Next: Read CI for the pushed branch, then land after green

Reference: Wrath 3.3.5a build 12340 decompilation at e67b6ffef23655dc7da061cb2d36b7963f6ef71a.
`CGGameUI::SetDeathReleaseLocation` forwards a valid active-map release position to
`MinimapSetSpiritHealerPOI`; the reference stores it in dynamic POI slot 3 with icon 8, flags 2,
importance -1, and the `SPIRIT_HEALER_RELEASE_RED` label. Project B already receives
`SMSG_DEATH_RELEASE_LOC` into `DeathNet::release_location` and clears it for the ghost/invalid leg.

Implemented:
- Added a map-scoped `MinimapSpiritHealer` resource derived from `DeathNet::release_location`.
- Added the `SPIRIT_HEALER_RELEASE_RED` landmark with the reference position, icon 8, flags 2,
  importance -1, and static arrow slot 3.
- Added spirit-healer participation to in-range POI icons and out-of-range rim arrows; static
  records remain visible past the ordinary 694.444-yard rank cutoff and indoors.
- Mismatched maps and `map_id == -1` produce no marker through the active-map filter.
- Added a focused selector test for icon 8, slot 3, indoor arrow behavior, and in-range atlas cell.

Verification:
- `git diff --check`: passed.
- Initial CI run `44652774628`: failed rustfmt and Linux/Windows compilation; source follow-up snapshots the release location before entering the VM callback and rustfmt-formats the changed Rust files. Corrective CI run pending.
- Local Cargo/client/server: not run, per owner instruction.

Residual risk: no live client gameplay acceptance was performed. The marker is sourced from the
existing network state and uses the existing minimap landmark renderer; owner should visually check
the icon/arrow after the next client batch.
