You are Astra, working on RustWrath (github.com/Dicetar/RustWrath, PUBLIC repo): a Rust/Bevy WoW
client for 3.3.5a build 12340 against AzerothCore. Long, uninterrupted session. Do not stop to
report progress. Work until the task is finished or truly blocked, then write the task note.

## Setup
1. Fetch `origin`. Read AGENTS.md, CONTEXT.md, docs/agents/workflow.md and docs/agents/sync.md.
2. Create branch `astra/er-legs` from current `origin/main` (at or after 28fc156d). Never push to
   `main`. Never force-push. Never rebase shared branches. Commit and push your branch at every
   stopping point, with the task note.
3. You own the Souls animation for this task:
   - `crates/benilla-app/src/souls/retarget.rs`: the local pass and `drive_remote_retarget`.
   - The locomotion and upper-body layers inside it.
4. Claude keeps everything else, and stays out of `retarget.rs` until you land:
   - `souls/drive.rs` and `souls/server.rs`;
   - `combat_kit*` and `combat_playback*`;
   - `integrations/ale/**`.
   If your fix needs one of them, write it in the task note instead.

## Background: what landed since your dc50432b
Your model-space pelvis, knees, feet and grip are on main. On top of them:
- **Clavicle:** the clavicle keeps the stock pose. Whether an arm chain has one is read from its
  shape: 4 or more bones, both arms alike.
- **Upper-body layers over the locomotion** (they leave `lower_bones` alone):
  - `Carry`: a cancelled action's upper body;
  - `guard_upper`: a guard held while moving;
  - `ceremony`: the stock draw and stow arms.
  - All three are eased with `smoothstep`.
- **Blend lengths:** `MIN_LOCO_BLEND_SECS` 0.18.
- **Stance banks:** a drawn weapon plays `a{bank:03}_*` locomotion. The kit's `stance [one two]`
  picks the bank; a one-handed dagger or sword is `0 10`.

## The bug: the legs are still broken (owner, Firna, wm-hd Human female, dagger drawn)
The screenshots are in
`D:\Projects\wow\worktrees\live-dx6\.scratch\project-b\prompts\astra-er-legs\`:
1. `1-run-dagger-side.png`, the run: each thigh has a sharp kink low down, near the knee. The shin
   and foot leave the line of the thigh.
2. `2-idle-dagger-side.png`, the idle: the legs look collapsed and bent wrongly at the knee.

## What Claude found (numbers, same folder)
The data is in two files:
- `leg-chains-and-stock-turns.txt` holds each leg's bones, their parts and pivots, plus the stock
  clips' local rotation (angle@axis, about 7 samples) for Run (anim 5), Walk (4) and Stand (0).
- `claude-leg-diagnostics.diff` holds the fixture probes that produced it. It is not applied;
  reuse what helps.

**Root cause: a knee twist helper gets the shin's rotation.**
- On wm-hd HumanFemale, `Leg::build` gives the right leg these bones:

  | Bones | Part | Notes |
  |---|---|---|
  | 6 | Upper | |
  | 25, 26 | Lower | both children of 6, both at the knee pivot [0.087, 0.589, 0.002] |
  | 32, 40, 46, 197 | Foot | |
  | 182 | Lower | child of 26 |

  The left leg is 8, 27, 28, and so on.
- The colocated-sibling rule pulls b25 in as `Lower`, so it gets the shin's model rotation.
- In stock Run:
  - b26, the real shin, bends 20-103 degrees about X (the knee hinge);
  - b25 only twists 3-8 degrees about its own length (Y) and never bends;
  - in Stand, b25 twists about 13 degrees about Y.
- So b25 is a thigh twist (lower-thigh) helper. Folding it with the shin drags the lower-thigh
  vertices round the knee. That is the kink.
- wm-hd HumanMale has the same pair: b16/b17 under b5, and b18/b19 under b6.
- Check the foot helpers the same way (b32/b40 colocated at the ankle, b197).
- Check the classic rigs too. Base HumanFemale's b24 is a child of the shin, so it is probably
  fine.

**Ruled out:**
- *The knee hinge plane.* Claude tried a hinge from the toes and hips while the ER leg is straight,
  instead of the noisy thigh/shin plane. The knee facing in the fixture did not change: idle
  knee_fwd stays within about 0.05 either way. So the visible fault is not the knee plane.
- *Undriven bones.* None: every bone below the pelvis's parent is written.

**The fixture blind spot:**
- `installed_player_limbs` measures only the landmark bones, which are hip, knee, ankle and toe:
  `[6, 26, 40, 46]` for wm-hd HumanFemale.
- It never sees a helper bone that skins the thigh. Its thigh/shin errors stay at 0-5 degrees at
  idle and run while the mesh is visibly broken.

## The goal
On every rig, the helpers deform like the stock clips do:
- a twist helper follows its segment, or splits the twist the way the stock clip does;
- the knee bends at the knee;
- no kink anywhere along the thigh or shin;
- feet flat and pointing ahead.

Rigs to check: Human female and male, Orc male and Tauren male, each on the classic and the HD
(wm-hd) profile.

Prefer a rule derived from the rig and the stock clips over per-model bone lists. For example: a
colocated sibling that no stock locomotion clip bends about the hinge belongs to the parent
segment.

## Also yours: the remaining animation complaints (owner, earlier, after 28fc156d)
Check each once the legs work, and fix what is still wrong.
- **Weak transitions:** clip-to-clip blends and attack-to-run.
- **Moving after an attack:** the upper body should finish over running legs (`Carry`), not
  slide.
- **Draw and sheathe:** the animation, the two-hand idle, and the weapon or two-handed guard while
  moving.
- **Shoulders at idle:** the clavicle fix landed, but the owner has not confirmed it.

## Verification (strict)
- sccache is required (see AGENTS.md).
- Run only:
  - `cargo check -p benilla-app`;
  - `cargo fmt`;
  - `cargo clippy -p benilla-app` (fix only what you introduced);
  - `souls::retarget::` by name filter, plus at most 20 new or changed cases.
- Command for the ignored real-rig fixture:
  `WOW_DATA='D:\Projects\wow\server\world of warcraft 3.3.5a hd\data' ERAN_FILE='D:\Projects\wow\client\benilla-config\combat\player_anims.bin' cargo test -p benilla-app --lib installed_player_limbs -- --ignored --nocapture`
- Extend that fixture so it fails on the kink. For example: every skinned leg bone's model
  rotation against its segment, or vertex positions of the thigh skin.
- No client launch, no server, no login. The owner builds, launches and judges the motion.
- In the task note, write what to look at in-game:
  - which clips and races;
  - which `souls.toml` switches;
  - what "fixed" looks like for each screenshot.

## Done
Your branch is pushed, and CI on it is green or each failure is explained. The task note lists:
- what was wrong, with the numbers that showed it;
- what changed;
- what was tested;
- what is unverified (visible motion in the running client).

Do not merge to `main` yourself unless the owner says so.
