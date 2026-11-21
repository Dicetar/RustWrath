You are Astra, working on RustWrath (github.com/Dicetar/RustWrath, PUBLIC repo): a Rust/Bevy WoW
client for 3.3.5a build 12340 against AzerothCore. Long, uninterrupted session. Do not stop to
report progress. Work until both tasks are finished or truly blocked, then write the task notes.

## Setup
1. Fetch `origin`. Read AGENTS.md, CONTEXT.md, docs/agents/workflow.md and docs/agents/sync.md.
2. One branch per task, each from current `origin/main` (at or after d9d35507):
   `astra/sex-stance`, then `astra/all-rigs`.
   - If task 2 needs task 1's code, branch `astra/all-rigs` from `astra/sex-stance` instead, and
     say so in its note.
   - Never push to `main`. Never force-push. Never rebase shared branches.
   - Commit and push your branch, with its task note, at every stopping point.
3. You own `crates/benilla-app/src/souls/retarget.rs`, plus `souls/config.rs` for new switches.
   Claude keeps everything else:
   - `souls/drive.rs`, `souls/input.rs` and `souls/server.rs`;
   - `combat_kit*` and `combat_playback*`;
   - `integrations/ale/**`.
   If your fix needs one of them, write it in the task note instead.

## What landed since your last run
- Your legs (`abc446da`, the skinned-displacement fixture in `81c7a3aa`) and the heavy grip
  (`e48de07c`) are on main, merged through `feat/souls-er-combat`.
- Your branch's red `kit::tests::a_bad_file_is_refused_with_its_line` is fixed on main (89f5dad4).
- eace0985 (Claude) changed `drive.rs` and `input.rs`. Do not undo these:
  - airborne frames zero the root-motion debt, so a jump attack no longer walks the body back
    after landing;
  - while mounted (`unit_mount_display_id() != 0`) there is no swing, roll, guard, sprint,
    crouch or jump attack.
- Warrior rage now comes from each Souls hit, through the ALE script. A server change later will
  make Souls hits run as the real auto-attack. Neither affects animation.

## Task 1: per-sex stance (owner: "Is there a male and female animations/stance? Can we adjust them per character gender?")
- `retarget.rs` already fits each ER clip to every race and sex skeleton, so bone lengths are
  per model.
- The motion itself is one ER rig's. Every race and sex gets the same posture: foot spacing, hip
  and shoulder set, arm hang and idle sway.
- Stock WoW clips do differ by sex. Female models carry narrower stances, different arm carriage
  and their own idle and run cycles.

Add a per-sex style layer:
1. **A per-(race, sex) pose-offset table** with:
   - pelvis tilt and sway;
   - thigh spread (stance width);
   - shoulder drop;
   - upper-arm abduction (elbows in or out);
   - optionally, a spine lean.
   Apply it as an additive rotation on top of the retargeted ER pose, for idle, locomotion and
   guard. Fade it out over attack actives so swings keep their reach, using the same smoothstep
   easing as the other layers.
2. **Seed values from the stock clips.** Measure the mean deltas between the stock male and
   female Stand, Walk and Run poses for each race, classic and HD where both exist. Do not invent
   numbers. Put the table and how you measured it in the note.
3. **Optional sheathed stock gait.** For walk and run with the Souls weapon sheathed, prefer
   the model's stock clip over the ER clip, per sex. Put this behind a `souls.toml` switch, off
   by default.
4. **A toggle.** Add a `souls.toml` feature switch (F10 lists switches), on by default.

## Task 2: every playable rig
`installed_player_limbs` checks only Human female and male, Orc male and Tauren male. Extend it
to every playable race and sex: Human, Orc, Dwarf, NightElf, Scourge, Tauren, Gnome, Troll,
BloodElf and Draenei, both sexes, on the classic and the wm-hd profile where the install has
both.
- Run each check on each rig:
  - the skinned leg displacement (no kink along the thigh or shin);
  - knee and foot facing;
  - the heavy-grip contact (the attachment stays at the hand on every `Heavy*`, `RunHeavy` and
    `JumpHeavyLand` frame);
  - the clavicle rule.
- Fix outliers with rules derived from the rig and the stock clips, not per-model bone lists.
  These rigs are the likely trouble:
  - Draenei and Tauren digitigrade legs and hooves;
  - Gnome and Dwarf proportions;
  - Undead (`Scourge`) open-jaw and ragged-limb helpers;
  - Blood Elf and Night Elf long-limb female rigs.
- Print a per-rig summary table from the fixture and put it in the note.

## Verification (strict)
- sccache is required (see AGENTS.md).
- Run only:
  - `cargo check -p benilla-app`;
  - `cargo fmt` (CI fails on any fmt diff; run it before every push);
  - `cargo clippy -p benilla-app` (fix only what you introduced);
  - `souls::retarget::` by name filter, plus at most 20 new or changed cases per task.
- The real-rig fixture:
  `WOW_DATA='D:\Projects\wow\server\world of warcraft 3.3.5a hd\data' ERAN_FILE='D:\Projects\wow\client\benilla-config\combat\player_anims.bin' cargo test -p benilla-app --lib installed_player_limbs -- --ignored --nocapture`
- CI logs are readable with `gh run view <id> --repo Dicetar/RustWrath --log-failed`. Strip
  the literal colour codes with `sed 's/\^\[\[[0-9;]*m//g'`.
- `player-clippy` is red on main from old debt. It only counts if your diff adds a finding.
- No client launch, no server, no login. The owner builds, launches and judges the motion.

## Done
Both branches are pushed, and CI on each is green, or each failure is explained from its log.
Each note, `.scratch/project-b/handoffs/2026-11-21-astra-<slug>.md`, lists:
- what was wrong, with the numbers that showed it;
- what changed;
- what was tested;
- what to look at in game: which races and sexes, which clips, which `souls.toml` switches, and
  what "right" looks like;
- what is unverified (visible motion in the running client).

Do not merge to `main` yourself unless the owner says so.
