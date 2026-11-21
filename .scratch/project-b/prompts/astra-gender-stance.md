# Astra: per-gender stance for the ER locomotion and combat clips

Owner request: "Is there a male and female animations/stance? Can we adjust them per character
gender?"

## Where things stand

- `crates/benilla-app/src/souls/retarget.rs` already fits each ER clip to every race and sex
  skeleton (`Character\{race}\{sex}\{race}{sex}`), so bone lengths are per model.
- The motion itself is one ER rig's. Every race and sex gets the same posture: foot spacing, hip
  and shoulder set, arm hang and idle sway. Stock WoW clips do differ by sex. Female models carry
  narrower stances, different arm carriage and their own idle and run cycles.

## Task

Add a per-sex style layer to the retarget, owned by you in `retarget.rs`:

1. A small per-(race, sex) pose-offset table: pelvis tilt and sway, thigh spread (stance width),
   shoulder drop, upper-arm abduction (elbows in or out), and optionally a spine lean. Apply it
   as an additive rotation on top of the retargeted ER pose, for idle, locomotion and guard.
   Fade it out during attack actives so swings keep their reach.
2. Seed values by sampling the stock clips: measure the mean deltas between the stock
   HumanMale and HumanFemale Stand/Run poses, and the same for the other races. Do not invent
   numbers.
3. Optionally, for walk and run with the Souls weapon sheathed, prefer the model's stock clip
   over the ER clip per sex. Put this behind a `souls.toml` switch, off by default.
4. Make it togglable: a `souls.toml` feature switch (F10 lists switches), on by default.

Done means: Human male and female standing side by side in Souls mode visibly differ in stance
width and arm carriage; attacks are unchanged in reach; the switch off restores today's pose.
Keep tests within the 20-case limit (one table lookup test, one additive-pose test). Land on main
alone (`git push origin HEAD:main`).
