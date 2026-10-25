# First graphical acceptance — procedure (execution NOT authorized yet)

Source checkpoint: `docs`-free local checkpoint
`.scratch/project-b/checkpoints/2026-10-25-oc-lead-repair.md`, plus the Wrath cleanup/response
corrections in the current dirty `crates/benilla-app/src/wrath.rs`.

Do not run this until the owner explicitly authorizes the graphical application launch, the lab
auth/world servers and the account/session. It is a preparation record, not executed evidence.

## Preconditions

- Authorized lab account and character; credentials supplied only through protected
  `WOW_USER`/`WOW_PASS` (never argv, logs or this file).
- Authorized `WOW_WRATH_AUTH` and `WOW_WRATH_WORLD`, or the local defaults.
- WotLK profile manifest already used by the route:
  `WM_ASSET_MANIFEST=D:/Projects/project-b-opencode/docs/profiles/base.mpqlist`,
  `WM_ASSET_PROFILE=base` (or the owner's approved `wm-hd`).
- No second client on the same account.
- Record the exact client source hash and profile manifest hash before launching.

## Launch

```powershell
# Values below are only feature/selection switches; credentials come from the protected operator
# environment, not from this file.
$env:WOW_WRATH = "1"
$env:WM_ASSET_MANIFEST = "D:/Projects/project-b-opencode/docs/profiles/base.mpqlist"
$env:WM_ASSET_PROFILE = "base"
# $env:WOW_USER / $env:WOW_PASS / $env:WOW_WRATH_AUTH / $env:WOW_WRATH_WORLD supplied by the operator.
# Optional WM receiver evidence: $env:WM_NATIVE_OBSERVATION_TOKEN.
$env:WOW_UNATTENDED = "1"
$env:WOW_NOSOUND = "1"
cargo run -p benilla
```

Expected log edges:

- no `FATAL`, no WotLK DBC field-count/parser failure;
- `wrath: entered world <map>, player <guid>, <n> entities` with a nonzero entity count;
- a visible world camera, not the glue/login screen;
- no repeated `world server made no response` in a healthy run.

## Required evidence

1. **Frame**: one screenshot or short recording showing the actual player body and at least one
   nearby creature on the selected Azeroth/Elwynn tile, with terrain, model, external skin and
   lighting visible. The run identity (source hash, profile, map, character) accompanies it.
2. **Camera response**: a before/after mouse-look pair or a short clip proving camera yaw/pitch and
   zoom respond; do not substitute a free-fly camera for the player-follow camera.
3. **Correctness checks**: player and creature are not cubes/placeholders; no stale old-scope
   entities; no persistent asset/parser error in the log.
4. **Terminal teardown**: `/logout` or the supported logout path; log shows session end and the
   rendered player/creature scope disappears. Input and pending actions must not survive into the
   next scope.
5. **Server-response policy**: if testable, a simulated silent world-server window of at least 60 s
   produces `world server made no response for 60 s`, a `Disconnected` health frame when WM is
   enabled, and the same scope teardown.

Store the raw log and capture under an owner-approved evidence folder, named with the source hash
and timestamp. Do not paste credentials or account data into the evidence.

## Failure triage order

1. Source/profile mismatch before display.
2. Catalog/asset path failure (Map/CreatureDisplayInfo/CreatureModelData/M2/SKIN/BLP).
3. Player/creature translation or camera stage.
4. Server-response/health/teardown.

Only after the screenshot/log identity is recorded may RENDER or the route be called accepted.
