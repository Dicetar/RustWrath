# Astra: per-sex stance

Branch `astra/sex-stance`, based on `d9d35507`. No main changes.

The local ER solve now layers a stock-derived posture over grounded idle, movement and guard. The cache is per race/sex on the loaded model, so classic and HD don't share angles. Each row is the mean rotation relative to bind across 32 evenly spaced, endpoint-excluded samples each of primary Stand (0), Walk (4) and Run (5): pelvis X/Z, signed bilateral thigh Z, clavicle Z (drop), and upper-arm Z. Shoulder drop is applied at the upper arm so the clavicle itself remains stock. The attack blend and its first hit frame cap the style at zero by contact; the saved blend source excludes the additive layer.

`[features]` has `sex_stance = true` and `sheathed_stock_gait = false`. F10 reload logs the feature struct, including when the file is absent. The stock gait switch opts out only for walk/run while visually stowed, not guard, crouch, sprint, or attacks.

Measurement fixture: `souls::retarget::tests::installed_stance_offsets`, ignored, with `WOW_DATA` and `ERAN_FILE`. It prints female-minus-male across the same five axes in degrees. Numeric results, scoped compilation/tests and CI verdict are not yet confirmed.

In game: compare both sexes on base and wm-hd, starting with Human, Orc and Tauren. Toggle the stance off/on via F10 and compare idle, walk, run and moving guard; look for a posture difference without shoulder pinching or blade reach changes. Enable stock gait and compare drawn/stowed walk/run, including draw/stow overlays; then check charged and jump-heavy contact. No client/server launch or login performed.
