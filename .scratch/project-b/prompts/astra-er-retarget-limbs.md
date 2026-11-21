You are Astra, working on RustWrath (github.com/Dicetar/RustWrath, PUBLIC repo): a Rust/Bevy WoW
client for 3.3.5a build 12340 against AzerothCore. Long, uninterrupted session. Do not stop to
report progress. Work until the task is finished or truly blocked, then write the task note.

## Setup
1. Fetch `origin`. Read AGENTS.md, CONTEXT.md, docs/agents/workflow.md and docs/agents/sync.md.
2. Create branch `astra/er-retarget-limbs` from current `origin/main` (at or after 982477a6).
   Never push to `main`. Never force-push. Never rebase shared branches. Commit and push your
   branch at every stopping point, with the task note.
3. You own `crates/benilla-app/src/souls/retarget.rs` for this task: the local player pass and the
   relayed-player pass (`drive_remote_retarget`, which you wrote). Claude stays out of it until you
   land. Do NOT touch the rest of `souls/**`, `combat_kit*`, `combat_playback*` or
   `integrations/ale/**`. If your fix needs one of them, write that in the task note instead.

## The bug: Elden Ring clips on the WoW rig give broken legs (and a dubious grip)
With Souls mode on (`F10`; `benilla-config/combat/souls.toml` has `retarget = true` and
`retarget_locomotion = true`), the player's body plays the ER clips from `player_anims.bin`.
The arms and torso read fine. The legs do not. The owner's screenshots (Human female, Firna,
one-handed axe) are in:
`D:\Projects\wow\worktrees\live-dx6\.scratch\project-b\prompts\astra-er-retarget-limbs\`

1. `1-idle-axe-front.webp`, the ER idle/guard stance:
   - The legs splay wide, knees in.
   - The feet point straight down, like a ballerina on tiptoe.
   - The axe hangs low by the thigh, head down.
2. `2-run-axe-back.png`, the run:
   - The legs are bowed.
   - The feet are twisted inward/backward.
   - The weapon trails low behind the hand.
3. `3-low-pose-split.png`, a low pose (crouch or roll):
   - The pelvis drops to the ground.
   - Both legs stretch straight out sideways, a full split.

The goal: a WoW body that stands, walks, runs, rolls and crouches like the ER clip. That means
knees bending forward, feet flat and pointing ahead, no splits, no tiptoe, and a weapon held the
way the ER hand holds it. It must work on both rigs: the classic M2 player models and the HD ones.
Check at least Human female, Human male, Orc male and Tauren male (Tauren has a different
hip/leg build).

## How the pass works now (read the module doc and `RigMap::build` / `RigMap::pose`)
- **The bake:**
  - ERAN gives model-space joint positions per frame at 30 fps, plus the head's axes.
  - Root motion is removed.
  - `er_to_model` is a half turn about up.
  - The local bake is `D:\Projects\wow\client\benilla-config\combat\player_anims.bin`. It is
    52 MB, 1,479 clips. Never commit it.
  - The reference that produced it, read-only, is
    `D:\Projects\elden-ring-combat-rewrite-reference`.
- **Legs:**
  - Each leg chain, thigh to toe, is anchored to ER `Thigh/Calf/Foot/Toe0` by pivot height
    (`want = points[0].y * rest.y / hip_y`).
  - Each bone is posed with `Drive::Aim`: the shortest arc that turns its bind direction onto the
    ER segment direction.
  - Nothing controls the bone's twist around its own axis. There is no knee pole vector and no
    foot frame.
  - The toe "follows its foot".
- **Pelvis:**
  - Its translation is the ER pelvis's offset from its rest height, times `scale`.
  - There is no leg IK or foot planting, so a lowered pelvis with legs aimed at the ER directions
    is not constrained to keep the feet on the ground.
- **Hands:**
  - `Drive::Hand` builds the wrist frame from ER hand→middle knuckle (up) and hand→thumb base
    (side), against the M2 hand/thumb bind pivots (`HandKeyBones.thumbs`).
  - That roll also turns the held weapon. In screenshot 1 the axe points down, which may be the
    roll or the side sign.
- **Arms:** clavicle detection by length, with elbow anchoring by the elbow attachment.
- **Spine:** `Drive::Torso` frames by arc length.

## Hypotheses to check first. Prove each with numbers, not with eyes alone.
- **Missing twist:**
  - The shortest-arc aim leaves thigh and calf roll free.
  - The knee plane should be set from the ER hip-knee-ankle plane, or from the knee's forward
    direction.
  - The foot needs a whole frame: ankle→toe forward plus an up/side, the same way
    `Drive::Hand` frames the wrist. Right now the foot only aims at the toe, and if the M2 toe
    pivot sits below or behind the ankle the foot points down.
- **Anchor mismatch:**
  - Anchoring knee and ankle by height assumes the model's origin is at the feet and that the
    bind leg hangs straight.
  - Check the actual M2 and HD leg pivots for the races above.
  - The split in screenshot 3 smells like a chain anchored to the wrong ER joint, a left/right or
    axis sign, or HD hip-root handling (`shared` ancestors), not just missing twist.
- **Proportions:**
  - ER and WoW leg/torso proportions differ.
  - Consider two-bone IK for the legs: target the ER ankle position scaled to the model's leg
    length, measured from the hip, with the pole from the ER knee. That keeps the feet on the
    ground when the pelvis drops.
  - Keep it bounded and cheap. The relayed pass allows 16 bodies × 512 bones.
- **Grip:**
  - Confirm the hand frame's side sign per hand.
  - Confirm the M2 hand attachment and weapon orientation convention. A weapon should point along
    the ER weapon (`R_Weapon`/`L_Weapon` joints are in the bake) when the hand is posed.
  - If the ER weapon joint gives a better frame for the wrist than the thumb does, use it.

## Tooling you are encouraged to build
- A pure (non-Bevy) debug dump: take one rig (parents and binds from a real model through the
  existing asset path) and one clip frame, and print each driven bone's world direction next to
  the ER segment direction it should match. Also print the foot height relative to the ground.
- A test fixture from a real classic M2 rig and an HD rig. Test the invariants below.

## Invariants to test (at most 20 new cases in total)
On the idle, run and crouch frames:
- every driven segment's world direction is within a few degrees of its ER segment;
- the knee is in front of the hip-ankle line;
- the foot's forward points roughly along the body's forward;
- neither foot is far below or far above the ground;
- left and right are not swapped.

Also keep the 4 existing `souls::retarget::tests` passing.

## Verification (strict)
- sccache is required (see AGENTS.md).
- Run only:
  - `cargo check -p benilla-app`;
  - `cargo fmt`;
  - `cargo clippy -p benilla-app` (fix only what you introduced);
  - your new tests and `souls::retarget::` by name filter.
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
