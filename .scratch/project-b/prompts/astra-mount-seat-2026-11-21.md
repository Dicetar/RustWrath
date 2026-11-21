You are Astra, working on RustWrath (github.com/Dicetar/RustWrath, PUBLIC repo): a Rust/Bevy WoW
client for 3.3.5a build 12340 against AzerothCore. Long, uninterrupted session. Do not stop to
report progress. Work until the task is finished or truly blocked, then update the note.

## The bug (owner report, 2026-11-21)
Firna (HD Human female) on a mount sits **too high**, and **the pose is wrong too**. The mount
seen is the Dreadsteed (`Creature\FelHorse\FelHorseEpic.m2`, stock, in `patch.mpq`,
CreatureModelData 2475, mountHeight 1.877). Spells 23161 and 5784 have state kit 0, so the
rider's anim is 91 Mount. Check the other mounts Firna owns as well, not just this one.

## What Claude established (source and assets only, no runtime)
- **12340 reference** (`D:/Projects/wow/reference/wowdecomp-reference`,
  `WoW/Source/Object/ObjectClient/Unit_C.cpp`):
  - `CreateUnitMount` (0x73D5D0, around line 21459):
    - sets `m_mountScale = creatureModelScale`;
    - attaches the rider to the mount with `AttachToParent(mount, 0)`, which is attachment id 0;
    - plays `SetKeyBoneSequence(worldModel, -1, m_mountAnimBehaviorId)`, which is 91 unless the
      mount aura's SpellVisual state kit names another anim (line ~16696);
    - sets the rider's `m_worldTransform` to `scale(1/m_mountScale)`.
  - `GetTrueScale` multiplies by `m_mountScale` while mounted.
  - Anim selection (around 4542-4560 and 4636-4711) forces the base slot to the mount anim.
    When the torso and the base both hold it, the client calls
    `UnsetKeyBoneSequence(torsoKeyBone)` (lines 4696, 4707-4711). **Our driver keeps the
    key-bone overlay running while mounted**: `creature_anim/driver.rs:~676` says "The key-bone
    overlay rides it untouched" (1.12 donor addresses). That is one candidate for the wrong
    pose.
  - `GetStandBox` uses `CreatureModelData.m_mountHeight` only for the collision box, not for
    the seat.
- **Our code:**
  - `crates/benilla-app/src/entities/mount.rs`:
    - the mount is a `NetEntity` child of the host;
    - the seat anchor sits under the mount's attachment-0 joint (`RigPose::anchor_for`,
      `benilla-world/src/rig_anim/pose.rs:154`), with a pivot-relative offset
      (`benilla-assets/src/model.rs:357-369`, `wow_to_bevy(pos) - pivot`);
    - the counter-scale is `1/mount_scale` (line 86);
    - `reseat_rig` moves `RigPose.joints_root`.
  - Mounted gait candidates are `[MOUNT(91), STAND]` (`creature_anim/driver/mode.rs:~371`), and a
    state-kit pin overrides them.
  - The Souls ER retarget (`souls/retarget.rs`, `drive_retarget`) runs only when the stock base
    `stands_in_for` (0, 4, 5, 11-13, 25-29, 37-40, 119, 120, 143, 187). 91 is excluded, so a
    mounted rider should keep the stock clip. Confirm this at runtime and in code. Also check
    that the souls drive, the stance layer, `sex_stance`, `sheathed_stock_gait`, eace0985's
    mounted guards and any grip or IK pass all leave a mounted rider alone.
- **Assets:**
  - HD `Character\Human\Female\HumanFemale.m2` comes from `patch-a.mpq` (v264, 242 sequences).
    - Mount 91 has four variations (sequence indices 135, 136, 137 with flags 0xa0, and 210 with
      flags 0xa1), all embedded, with z bounds -0.83 to 0.96.
    - Stand 0 has z bounds -0.05 to 1.93.
    - So if the rider renders Stand (or any upright clip) at the seat, she sits about one metre
      too high in a standing pose, which matches both symptoms.
  - FelHorseEpic has **two id-0 attachments**: bone 51 at (0.301, 0, 1.877) and bone 52 at
    (0.213, 0, 1.866). Check which one we take and which one 12340 takes (the first match in the
    attachment lookup).
- **Tools** (run from `D:/WOW/world of warcraft 3.3.5a hd/Data`):
  - `D:\Projects\wow\client\target\debug\examples\mpqcat.exe <archive> '<path>'`;
  - `dump_attach.exe <file.m2>`;
  - Python helpers in `F:\AI\tmp\`: `m2seq.py` (sequences and bounds), `mount_kits.py`,
    `state_kits.py`, `cmd_names.py`.
  - Server DBCs are in `D:/Projects/wow/server/Azerothcore_WoTLK_Repack/data/dbc`.

## Hypotheses, most likely first
1. The rider is not rendering 91. Something plays Stand or another upright clip instead: the
   fallback in `[MOUNT, STAND]`, a variation or sub-anim lookup on the HD model, the souls or ER
   path, or a one-shot or emote that is never displaced.
2. The torso key-bone overlay keeps an upright upper body over 91, where Wrath unsets it.
3. The HD rig and the seat anchor interact badly: `joints_root`, the pivot offset, or a
   double-applied scale (`GetTrueScale` times `1/mountScale` times our own scale).
4. The wrong id-0 attachment (bone 51 or 52) is taken.

## Task
1. Find the cause with evidence, not reasoning alone. Write a focused test or fixture against
   the real assets (`WOW_DATA` as in your earlier fixtures) that does the following:
   - mounts HD HumanFemale on FelHorseEpic through our real seat and driver code;
   - reports which sequence each slot (base and torso overlay) plays;
   - reports the rider's world-space pelvis height above the mount's attachment point, against
     what 12340's transform chain gives.
   If you need runtime evidence, add a temporary trace behind an env switch. The owner can run it
   during their next launch, so say exactly what to run and what to read.
2. Fix the shared cause, so that every rider on every mount sits in the saddle in the Mount pose.
   Then do the following:
   - match Wrath's torso unset while mounted, if hypothesis 2 is part of it;
   - keep 1.12-target behaviour where it differs;
   - do not undo eace0985's mounted guards.
3. Add a regression test that would have caught this: a mounted rider's base is 91 (or the
   state-kit anim), the torso overlay is clear, and the seat height is within a stated tolerance
   for HD HumanFemale on FelHorseEpic and one more race and mount pair.

## Setup
- Fetch `origin`. Branch `astra/mount-seat` from `origin/main` (now 1f674fd72 or later). Merge,
  never rebase. Never force-push or push to `main`.
- Use your own target dir every time, for example
  `--target-dir D:/Projects/wow/worktrees/<yours>/target`, with sccache and the toolchain from
  `rust-toolchain.toml`. If sccache drops its connection (10054), run `sccache --stop-server`,
  then retry once.
- You own `entities/mount.rs`, the mount paths in `creature_anim/driver*`, and
  `souls/retarget.rs`. Claude keeps `souls/drive.rs`, `input.rs`, `server.rs`, `combat_kit*`,
  `combat_playback*` and `integrations/ale/**`.

## Verification (strict)
- Run only:
  - `cargo check -p benilla-app`;
  - `cargo fmt` before every push;
  - `cargo clippy -p benilla-app`, fixing only what you introduced;
  - your new and changed tests by name filter, at most 20 cases in total, plus the real-asset
    fixture with `--ignored`.
- Read CI logs with `gh run view <id> --repo Dicetar/RustWrath --log-failed`, stripping colour
  codes.
- No client launch, no server, no login. The owner builds, launches and judges the seat.

## Done
- Push `astra/mount-seat` with a note at `.scratch/project-b/handoffs/2026-11-21-astra-mount-seat.md`
  containing:
  - the cause, shown with numbers (sequence ids, slot states, heights);
  - the fix, and the 12340 lines it follows;
  - the test and fixture output;
  - the CI run id and result, read from the log;
  - what the owner should look at in game (Firna on the Dreadsteed and one other mount: seated
    in the saddle, legs astride, upper body following the mount clip, moving and standing
    still);
  - what is unverified.
- Do not merge to `main` yourself. The branch is held for the owner's judgement.
