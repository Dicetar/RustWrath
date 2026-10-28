# OC — first-alpha execution, 2026-10-28

Status: bounded source/verification cycle. Base HEAD
`2cefd8a50920b0ca9c697b335561511663b6b940`; the two fixes below are committed at
`40b698facd768220d6e0d1e855d27bbf36fb12d9`. Tracked tree and index clean at start.
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
- First worldview run (base discovery chain): `Azeroth.wdt` failed with
  `truncated WDT chunk header`; every WMO path was reported `Path not found`. Screenshot
  `.scratch/project-b/verification/2026-10-28-oc-alpha/northshire-worldview.png`.
- After `40b698fa`, `cargo test -p benilla-wdt --lib`: 15 passed (including the new
  trailing-padding case). `cargo build -p benilla-worldview` and `cargo build -p benilla`:
  pass. `cargo test -p benilla-protocol-wrath --lib`: 30 passed.
- Profile-correct worldview at Northshire: pass, exit 0, no WDT error and no `Path not found`
  for `nsabbey`, `abbeygate01/02` or `elwynnwidebridge`. Screenshots
  `northshire-profile.png` and `northshire-south.png` show terrain, trees, the abbey roof and
  nearby props. Remaining non-fatal asset errors: `piinetree01.blp` mip level 9 out of
  bounds and `blacksmith_smoke.m2` `not a SKIN profile`; neither is yet proven to block the
  route. The first profile screenshot's near-spawn camera is partly occluded by a WMO wall;
  the south view is usable. This is a renderer/world proxy, not client or UI acceptance.
- Final client binary after `40b698fa`: SHA-256
  `A424BAB2FE8B49C4B439E5189B0FBA3D9407FAAC0808B9ACFD0C0CC5A8A69712`; the launch still
  stops at the missing-identity error.

## Revalidation after the checkpoint

At `2026-10-28T19:27:32+03:00`, HEAD is still
`3a064904fd26ce8743627f0aeafe9a7319e05dd2`; `git diff 40b698fa..HEAD -- crates` is
empty, so the built binary is still relevant. `target/debug/benilla.exe` is unchanged at
SHA-256 `A424BAB2FE8B49C4B439E5189B0FBA3D9407FAAC0808B9ACFD0C0CC5A8A69712`.
Tracked tree and index are clean; only the preserved untracked logs and procgen helper remain.

The lab server is reachable: `authserver` owns `0.0.0.0:3724` and `worldserver` owns
`0.0.0.0:8095` (same processes as the run card: 25176 and 27100). No server/DB command was run.

### Identity handoff (no secret recorded)

The Wrath launcher reads `WOW_USER` and `WOW_PASS` from its process environment. The owner can
supply them in their own PowerShell session, using a masked prompt, and launch the existing
binary from that same session; the environment is inherited and nothing is written to the repo
or this log:

```powershell
$env:WOW_USER = Read-Host 'WOW account'
$sec = Read-Host -AsSecureString 'WOW password'
$env:WOW_PASS = [System.Net.NetworkCredential]::new('', $sec).Password
Remove-Variable sec
# set WOW_WRATH / WOW_WRATH_AUTH / WOW_WRATH_WORLD / WOW_DATA / WM_ASSET_MANIFEST / WM_ASSET_PROFILE
& 'D:\Projects\project-b-client\target\debug\benilla.exe' *> 'D:\Projects\project-b-client\.scratch\project-b\verification\2026-10-28-oc-alpha\connected-run.log'
```

`.probe-identity` is gitignored but the Wrath config does not read it; using that file with a
small shell wrapper is also possible, but no wrapper or new credential path is added here.

## Acceptance ledger

| Step | Result |
|---|---|
| Build current client | pass, `40b698fa` dev profile |
| Login/select character | blocked: no dedicated identity in this environment |
| Enter Northshire | blocked by login |
| Terrain/player/stock UI visibility | terrain, abbey and route props render in the profile-correct worldview; no world-player or stock UI check yet |
| Move/turn, target/interact, quest, combat, loot, turn-in, re-entry | not run; need the live identity and human combat step |

No alpha or full-replacement claim follows.
