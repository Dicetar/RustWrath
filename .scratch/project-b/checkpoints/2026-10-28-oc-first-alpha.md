# OC — first-alpha execution, 2026-10-28

Status: bounded source/verification cycle in progress. Base HEAD
`2cefd8a50920b0ca9c697b335561511663b6b940`; tracked tree and index clean at start.
No server/DB/install/WM operation.

## Claims

- `crates/benilla-world/src/worldview.rs` — honor the same `WM_ASSET_MANIFEST` /
  `WM_ASSET_PROFILE` profile source the client uses, so the render check reads the selected
  Warcraft install profile instead of the 1.12 discovery chain.
- `crates/benilla-wdt/src/lib.rs` — accept a trailing partial chunk header (the selected
  `Azeroth.wdt` ends with an 8-byte zero-length `MWMO` and 3 padding bytes), with one focused
  existing-module regression case.
- this checkpoint.

No Astra-claimed UI file is edited.

## Observed evidence

- `cargo build -p benilla` at `2cefd8a5`: pass, 7m31s, dev binary
  `target/debug/benilla.exe`.
- Wrath launch with the run-card environment but no `WOW_USER`/`WOW_PASS`:
  `wrath: FATAL - WOW_WRATH needs WOW_USER: environment variable not found`, exit 1.
  No `.probe-identity` exists at the repo root and no dedicated credential variables are present.
  This is the live-route blocker.
- Stock Wrath UI smoke with real `wm-hd` data: pass (1 test).
- `benilla-protocol-wrath` lib tests: 30 passed.
- `benilla-protocol` loot tests: 16 passed.
- Worldview at Northshire with the run-card data variables: rendered terrain, exit 0, but
  `Azeroth.wdt` failed with `truncated WDT chunk header`; every WMO path was reported
  `Path not found` because the worldview used the 1.12 discovery chain, not the profile.
  Screenshot `.scratch/project-b/verification/2026-10-28-oc-alpha/northshire-worldview.png`
  shows the right side occluded by a WMO/fallback surface and no abbey props.

## Acceptance ledger

| Step | Result |
|---|---|
| Build current client | pass, `2cefd8a5` dev profile |
| Login/select character | blocked: no dedicated identity in this environment |
| Enter Northshire | blocked by login |
| Terrain/player/stock UI visibility | terrain only in worldview; profile-correct render pending the fix below |
| Move/turn, target/interact, quest, combat, loot, turn-in, re-entry | not run; need the live identity and human combat step |

No alpha or full-replacement claim follows.
