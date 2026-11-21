You are Astra, working on RustWrath (github.com/Dicetar/RustWrath, PUBLIC repo): a Rust/Bevy WoW
client for 3.3.5a build 12340 against AzerothCore. Long, uninterrupted session. Do not stop to
report progress. Work until every task is finished or truly blocked, then update the notes.

## Review of your last run (Claude)
`astra/sex-stance` (d3f99bde) and `astra/all-rigs` (b0d82fd5) are pushed with notes.
- **CI:** both fail only `player-clippy`, which is old workspace debt that main has too
  (`benilla-wm-native/observation.rs` and others). Neither diff adds a finding. The
  `chunks_exact` warning at `retarget.rs:316` predates you (5d3ce460).
- **all-rigs: good, not finished.**
  - The fixture covers 40 rigs, and skin and grip drift are zero.
  - Shin deviation is 126-143 degrees on Gnome (both sexes, both profiles) and Tauren male,
    and 71-95 degrees on Tauren female and Draenei female. The note calls this "a diagnostic,
    not a pass threshold" and leaves it unexplained. A shin 140 degrees off its target is a
    knee folded backwards, so it must be explained or fixed.
  - The note's line "The original HumanFemale grip regression remains" is unclear. Is a grip
    regression still present, or does the fixture still hold its regression check? Fix the
    line.
- **sex-stance: wrong runtime, not landable as is.**
  - `stock_stance` averages the rig's *own* stock local rotations: pelvis X/Z, thigh Z,
    clavicle Z and upper-arm Z over Stand, Walk and Run.
  - `apply_stance` then *adds that whole mean* on top of the retargeted ER pose, for both
    sexes, on by default.
  - The ER pose already has its own arm hang, hip set and stance width. Adding the stock
    pose's full offset from bind on top counts the posture twice. If the stock Stand hangs
    the arm 40 degrees from bind, every character's arms go 40 degrees further.
  - Your fixture prints only the female-minus-male delta, so nothing measured what the runtime
    applies.
  - The local build failed because the target dir was shared with another worktree's newer
    rustc (E0514 on `flate2`), and the name-filtered run used the other worktree's binary.

## Setup
- Fetch `origin`. Continue on your two branches. Merge `origin/main` into each (merge, not
  rebase). Never force-push or push to `main`.
- **Build with your own target dir**, every time, for example `--target-dir D:/Projects/wow/worktrees/<yours>/target`,
  with sccache (AGENTS.md) and the toolchain from `rust-toolchain.toml`. If sccache drops its
  connection (10054), run `sccache --stop-server`, then retry once.
- Ownership is unchanged. You own `souls/retarget.rs` and the new switches in `souls/config.rs`.
  Claude keeps `souls/drive.rs`, `input.rs`, `server.rs`, `combat_kit*`, `combat_playback*` and
  `integrations/ale/**`. Do not undo eace0985's airborne-debt and mounted guards.

## Task 1: make the stance layer a correction toward stock, and measure it
1. The offset must move the retargeted ER posture toward the rig's stock posture, not add the
   stock posture again. Per rig:
   - `stock = mean stock pose` (what `stock_stance` measures now);
   - `er = mean retargeted ER pose`, the same five angles over the ER idle, walk and run clips
     on this same rig;
   - `offset = stock - er`.
   - Measure `er` from what the retarget actually writes, after the gait layer and before the
     stance layer. Cache it per (race, sex, model), as now.
2. Clamp each angle to a stated limit, about 12 degrees for the pelvis and 15 for the limbs,
   and log any clamp once per rig. A clamp that fires on a common rig means the measure is
   wrong, not the rig.
3. Check the Euler decomposition. `to_euler(XYZ)` on a bone whose stock rotation is large
   mixes axes. Measure in the bone's hinge or abduction axes from the rig map where they exist,
   or show that the XYZ terms stay under about 20 degrees on every rig.
4. Make the fixture print what runtime applies: `stock`, `er` and `offset` per rig, for all
   40 rigs. Put the table in the note. The female-minus-male delta can stay as a column.
5. Add a test: a synthetic rig whose ER pose already equals its stock posture gets a zero
   offset, and one with a known gap gets exactly that gap.
6. Keep `sex_stance` on by default *only* if every rig's offset is within the clamps without
   firing them. Otherwise default it off and say why.

## Task 2: explain and fix the leg outliers
For Gnome, Tauren and Draenei female, and Dwarf if it shows the same pattern:
- Find where the 70-143 degree shin error comes from: the grounded contact solve pulling an
  unreachable target, a wrong chain (a helper taken as the shin, as with HD Human b25), a
  hoof or digitigrade segment counted as the shin, or a bad hinge axis.
- Show it with the numbers: bone ids, parents, pivots, and which sample it is.
- Fix it with a rule derived from the rig and the stock clips, not a per-model list.
- Then give the fixture **pass limits**: shin and thigh angle error, foot-facing at grounded
  samples, and seat drift, so that any future regression on any of the 40 rigs fails the test.
  Re-run and put the new table in the note.

## Task 3: the same posture for other Souls players
`drive_remote_retarget` poses relayed Souls players. Give it the same stance layer, with the
same cache and the same attack fade, keyed by that unit's race and sex from its object fields.
Another Firna-like player should then look the same as your own does. Add one test that the
remote and local paths produce the same offset for the same rig.

## Verification (strict)
- Run only:
  - `cargo check -p benilla-app`;
  - `cargo fmt` (before every push);
  - `cargo clippy -p benilla-app` (fix only what you introduced);
  - `souls::retarget::` by name filter, plus at most 20 new or changed cases in total.
- The real-rig fixtures `installed_player_limbs` and `installed_stance_offsets`:
  `WOW_DATA='D:\Projects\wow\server\world of warcraft 3.3.5a hd\data' ERAN_FILE='D:\Projects\wow\client\benilla-config\combat\player_anims.bin' cargo test -p benilla-app --lib <name> -- --ignored --nocapture`
- CI logs are readable with `gh run view <id> --repo Dicetar/RustWrath --log-failed`. Strip
  the literal colour codes with `sed 's/\^\[\[[0-9;]*m//g'`. `player-clippy` only counts if
  your diff adds a finding.
- No client launch, no server, no login. The owner builds, launches and judges the motion.

## Done
- When all three tasks pass, merge `astra/all-rigs` into `astra/sex-stance` (merge commit), so
  the owner has one branch to land. Push it.
- Update `.scratch/project-b/handoffs/2026-11-21-astra-sex-stance.md` with:
  - the corrected design and both tables;
  - the outlier cause and fix;
  - the pass limits;
  - the CI run id and result, read from the log;
  - what to look at in game (races, sexes, clips, the `sex_stance` and `sheathed_stock_gait`
    switches, and what "right" looks like);
  - what is unverified.
- Do not merge to `main` yourself unless the owner says so.
