# Run card — rendered connected Northshire loop (requested execution)

Date: 2026-10-27. Owner: OpenCode (client/gameplay integration lead).
Source identity: `wotlk` at the commit that adds this file; `cargo check -p benilla-app --lib`
and `cargo check -p benilla` were run at `d8250c5c` (a8a51af9 + Astra's cast UI commits) and
passed. This file is the single bounded execution request for the graphical/live (G/L) gates;
it grants nothing by itself.

## State already verified read-only

- Lab server processes are up (observed 2026-10-27): `authserver` pid 25176, `worldserver` pid
  27100, `mysqld` pids 39852/41944, started 2026-10-24. No server/db command was run by the
  client campaign.
- Client profile: `WM_ASSET_PROFILE=wm-hd`, manifest SHA-256
  `439bdc21d09f5509073cd88cc341adb2eb05854b425f4d9dec1d2bcee5083d59`.
- Base manifest SHA-256 `00c7357f2a5599b05ea7b97e4b3529198deb17a458b47199f6567259a1bde1cf`.
- Terrain collider panic repaired at `a8a51af9`/`1f61f21a`; the exact panic input builds under
  the vendored parry3d (`third_party/parry3d/BENILLA.md`). The graphical half still needs this
  run.

## Requested authorization

One client process, one human player, one account, one pinned character; maximum 60 minutes
wall clock; local loopback only; no deployment, no DB edit, no server restart, no account
creation. WM bridge/receiver disabled for this pass (`WM_NATIVE_OBSERVATION_TOKEN` unset) so the
first result is baseline playability, not WM.

## Pinned scenario

- Character: the eligible Alliance human mage, level 2–5, with Fireball 133 known. Record the
  actual name/GUID/level/class shown at the select screen; do not substitute another character
  if this one is ineligible — stop and report.
- Quest 6 “Bounty on Garrick Padfoot”; Deputy Willem entry 823; Garrick Padfoot entry 103;
  Garrick's Head item 182; reward choice 0 = Tapered Pants item 6076.
- Explicit-accept control (only if quest 6 auto-accepts): quest 5261 from the same Willem.

## Launch (PowerShell, from `D:/Projects/project-b-client`)

Secret-bearing variables are supplied by the owner; they are never written to the repo.

```powershell
$env:WOW_WRATH = "1"
$env:WOW_USER  = "<owner-supplied account>"
$env:WOW_PASS  = "<owner-supplied password>"
$env:WOW_WRATH_AUTH  = "127.0.0.1:3724"
$env:WOW_WRATH_WORLD = "127.0.0.1:8095"
$env:WOW_DATA = "D:\WOW\world of warcraft 3.3.5a hd\data"
$env:WM_ASSET_MANIFEST = "D:\Projects\project-b-client\docs\profiles\wm-hd.mpqlist"
$env:WM_ASSET_PROFILE  = "wm-hd"
Remove-Item Env:WM_NATIVE_OBSERVATION_TOKEN -ErrorAction SilentlyContinue
cargo run -p benilla --release
```

## Script

1. Select screen appears. Record the roster; pick the pinned mage.
2. Enter world. Confirm terrain, sky, own character model and nearby NPCs render; record draw
   distance and framerate only as observation, not acceptance.
3. Camera: orbit, zoom, look up/down. Input: walk forward/back, strafe, turn, jump.
4. Confirm the character stays on the ground (no fall-through) on the abbey path and the bridge
   toward Garrick. Save a screenshot at the abbey gate and at the bridge.
5. Target Deputy Willem, open gossip, open quest 6. If it auto-accepts, record that; otherwise
   accept explicitly. Record the quest-log state.
6. Walk to Garrick Padfoot (west/south along the road). Target it; start attack; cast Fireball
   133 repeatedly until it dies. Record HP movement, cast bar behaviour, misses/refusals.
7. Loot the corpse; take Garrick's Head (182). Confirm the bag slot and quest counter.
8. Return to Willem; complete quest 6; request reward; choose Tapered Pants (6076). Confirm the
   reward item exists in the bags and the quest is gone.
9. Logout through the client; return to character selection; enter the same character again.
   Confirm inventory/quest state persisted and no stale cast bar/aura/target remains.
10. Quit the client.

## Evidence to keep (folder supplied by the owner)

- Client stdout/stderr with `RUST_BACKTRACE=1` set.
- `D:\WOW\WM_BridgeLab\run\Server.log` and `Errors.log` excerpt for the session window.
- Screenshots: select screen, world entry, abbey gate, bridge, combat, reward, re-entry.
- Exact source commit, profile hashes above, character name/GUID, wall-clock window.

## Stop conditions (stop and report; do not repair live)

- Any panic (especially `spawn_streamed_tiles` / parry BVH): keep the log and screenshot; the
  W1 repair is then disproven at runtime and must be re-diagnosed from that evidence.
- Missing/refused credentials, empty roster, or the pinned character ineligible.
- Any server-side error or a requested DB/admin intervention.

## What this run does not establish

WM observation/action delivery, experimental combat, or full replacement acceptance; those have
their own gates. A pass here is an internal playtest milestone only.
