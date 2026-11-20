# Astra: stock-relative stance and playable-rig validation

Both task branches merge `origin/main`; neither pushes to main. The stance pass now measures the loaded model's mean stock pose and the mean ER pose actually written by the solver on top of the same stock samples. Stand/Idle, Walk and Run contribute 32 equal-phase samples each. Subtract ER from stock, using signed quaternion twists around the same parent-local X/Z axes used to apply the correction rather than XYZ Euler decomposition. Clamp pelvis pitch/roll to ±12°, limb axes to ±15°. Clavicles stay stock; arm abduction is corrected on the upper arm. The stance is opt-in because the installed base Troll female measurement hits the limb clamp.

Local and remote pose passes share the calibration/cache implementation, keyed by race, sex and model animation graph. Remote reads identity from `ObjectStore`. Both style layers ease out before the hit window; the clip transition starts from the previous unstyled pose, so it does not retain the correction at contact. Unknown remote attack timing gets no stance. The local sheathed gait switch remains default-off.

The leg branch's contact solve and pre-change results are described in [its handoff](https://github.com/Dicetar/RustWrath/blob/astra/all-rigs/.scratch/project-b/handoffs/2026-11-21-astra-all-rigs.md). Both branch fixtures passed independently and `astra/all-rigs` was merged into this branch. Both installed fixtures passed again on the combined solver, along with 11 scoped retarget tests and `cargo check -p benilla-app`. Local Clippy was blocked by E0514 (`flate2` metadata/toolchain mismatch) even with separate target directories. Stance-only CI [44802802850](https://github.com/Dicetar/RustWrath/actions/runs/44802802850) passed Windows; the failed-job log showed only unrelated `player-clippy` findings in `benilla-ui`. Combined CI [44807968843](https://github.com/Dicetar/RustWrath/actions/runs/44807968843) passed Windows check, build and tests. Linux workspace Clippy passed, but `test-build` was terminated with exit 143 and no Rust error in the failed-job log. The Linux job was retried; attempt 2 passed workspace Clippy but was still in `test-build` at handoff. Combined Linux tests are not confirmed.

In game: compare both sexes across base and wm-hd, especially Gnome, Tauren and Draenei female on idle, walk/run, moving guard and a high-stepping swing. With `sex_stance = true`, posture should approach the stock pose without over-splaying limbs and should match a similarly equipped peer; heavy, running heavy and jump-heavy landing should still seat the weapon and preserve reach. With `sheathed_stock_gait = true`, F10 should switch the local stowed walk/run to stock without losing draw/stow or guard. No client, server or login used.

## Installed stance table (combined solver)

All 40 rigs measured; the fixture passed. One correction clamped: base Troll female upper arm, −19.80° raw to −15° applied. The Gnome male thigh clamp from the first pass cleared after the hip-spacer fix. Stance stays **off by default**. Columns are pelvis pitch, pelvis roll, thigh spread, shoulder drop, upper-arm abduction (degrees).

| Profile | Rig | Stock | ER | Applied offset | Clamped |
|---|---|---|---|---|---|
| base | HumanMale | [0.00, 0.00, -4.25, 0.36, -5.68] | [-0.69, 0.72, -0.26, 0.36, 4.29] | [0.69, -0.72, -3.99, 0.00, -9.96] | false |
| base | HumanFemale | [0.00, 0.00, 2.54, 3.97, 9.96] | [-0.69, 0.72, 6.01, 3.97, 24.46] | [0.69, -0.72, -3.47, 0.00, -14.50] | false |
| base | OrcMale | [0.00, 0.00, -4.97, 2.37, 9.67] | [-0.69, 0.72, -4.31, 2.37, 10.93] | [0.69, -0.72, -0.66, 0.00, -1.26] | false |
| base | OrcFemale | [-5.31, -0.03, -4.47, 3.24, 3.97] | [-0.69, 0.72, -1.52, 3.24, 13.16] | [-4.62, -0.75, -2.94, 0.00, -9.19] | false |
| base | DwarfMale | [0.00, 0.00, 3.23, -0.31, 13.73] | [-0.69, 0.72, -2.36, -0.31, 6.04] | [0.69, -0.72, 5.58, 0.00, 7.69] | false |
| base | DwarfFemale | [0.00, 0.00, -2.73, 0.38, 7.54] | [-0.69, 0.72, -2.20, 0.38, 7.49] | [0.69, -0.72, -0.53, 0.00, 0.05] | false |
| base | NightElfMale | [0.00, 0.00, -1.87, -0.73, 6.03] | [-0.69, 0.72, -0.46, -0.73, 3.57] | [0.69, -0.72, -1.41, 0.00, 2.46] | false |
| base | NightElfFemale | [-8.38, -1.01, -3.37, -0.42, 4.11] | [-0.69, 0.72, -1.25, -0.42, 17.46] | [-7.69, -1.72, -2.13, 0.00, -13.35] | false |
| base | ScourgeMale | [-9.01, -1.56, 6.68, 1.99, 16.39] | [-0.69, 0.72, 5.31, 1.99, 21.99] | [-8.31, -2.27, 1.36, 0.00, -5.60] | false |
| base | ScourgeFemale | [-0.83, 0.00, 4.40, -2.99, 21.65] | [-0.69, 0.72, 7.78, -2.99, 14.28] | [-0.14, -0.72, -3.38, 0.00, 7.36] | false |
| base | TaurenMale | [0.00, 0.00, 1.86, -0.31, 9.39] | [-0.69, 0.72, -4.74, -0.31, 5.99] | [0.69, -0.72, 6.60, 0.00, 3.40] | false |
| base | TaurenFemale | [0.00, 0.00, -5.66, 0.00, 0.46] | [-0.69, 0.72, -0.14, 0.00, 2.89] | [0.69, -0.72, -5.52, 0.00, -2.43] | false |
| base | GnomeMale | [0.00, 0.00, -9.83, 0.19, 1.04] | [-0.69, 0.72, -2.55, 0.19, -3.85] | [0.69, -0.72, -7.28, 0.00, 4.88] | false |
| base | GnomeFemale | [0.00, 0.00, -12.85, 0.19, -0.29] | [-0.69, 0.72, -5.61, 0.19, -2.56] | [0.69, -0.72, -7.24, 0.00, 2.27] | false |
| base | TrollMale | [-4.02, 0.00, 1.78, -2.07, 13.56] | [-0.69, 0.72, 1.99, -2.07, 18.56] | [-3.33, -0.72, -0.21, 0.00, -5.00] | false |
| base | TrollFemale | [0.00, 0.00, -1.19, -0.50, 2.89] | [-0.69, 0.72, 4.22, -0.50, 22.69] | [0.69, -0.72, -5.41, 0.00, -15.00] | true |
| base | BloodElfMale | [0.00, 0.00, -1.07, -3.03, -5.32] | [-0.69, 0.72, -1.37, -3.03, 2.32] | [0.69, -0.72, 0.29, 0.00, -7.63] | false |
| base | BloodElfFemale | [-1.86, -0.00, -5.20, -2.86, -2.63] | [-0.69, 0.72, -1.73, -2.86, 11.77] | [-1.16, -0.72, -3.48, 0.00, -14.40] | false |
| base | DraeneiMale | [0.00, 0.00, -3.75, -5.71, 7.22] | [-0.73, 0.88, -4.50, -5.71, -2.68] | [0.73, -0.88, 0.74, 0.00, 9.90] | false |
| base | DraeneiFemale | [-1.23, 0.00, -4.07, -2.01, 3.29] | [-0.69, 0.72, -3.05, -2.01, 9.66] | [-0.53, -0.71, -1.02, 0.00, -6.36] | false |
| wm-hd | HumanMale | [0.00, 0.00, -2.27, -0.61, -1.19] | [-0.69, 0.72, -2.08, -0.61, 2.53] | [0.69, -0.72, -0.20, 0.00, -3.72] | false |
| wm-hd | HumanFemale | [0.00, 0.00, 0.91, 3.34, -23.32] | [-0.69, 0.72, 3.83, 3.34, -15.30] | [0.69, -0.72, -2.92, 0.00, -8.02] | false |
| wm-hd | OrcMale | [0.00, 0.00, 3.30, 3.06, 15.86] | [-0.69, 0.72, -4.40, 3.06, 11.78] | [0.69, -0.72, 7.70, 0.00, 4.09] | false |
| wm-hd | OrcFemale | [0.00, 0.00, -3.28, 0.13, 1.06] | [-0.69, 0.72, -2.89, 0.13, 3.70] | [0.69, -0.72, -0.40, 0.00, -2.64] | false |
| wm-hd | DwarfMale | [0.00, 0.00, 4.21, 2.81, 16.55] | [-0.69, 0.72, -2.37, 2.81, 9.16] | [0.69, -0.72, 6.58, 0.00, 7.39] | false |
| wm-hd | DwarfFemale | [0.00, 0.00, -0.87, 3.12, -24.26] | [-0.69, 0.72, -0.31, 3.12, -21.01] | [0.69, -0.72, -0.56, 0.00, -3.25] | false |
| wm-hd | NightElfMale | [0.00, 0.00, -0.92, 5.57, 6.90] | [-0.69, 0.72, -1.18, 5.57, 13.71] | [0.69, -0.72, 0.26, 0.00, -6.81] | false |
| wm-hd | NightElfFemale | [-0.31, 0.00, -2.65, 1.48, -34.79] | [-0.69, 0.72, -0.98, 1.48, -28.12] | [0.38, -0.72, -1.67, 0.00, -6.67] | false |
| wm-hd | ScourgeMale | [-5.68, 0.00, 10.65, 5.11, -14.57] | [-0.69, 0.72, 6.02, 5.11, -9.14] | [-4.99, -0.72, 4.63, 0.00, -5.42] | false |
| wm-hd | ScourgeFemale | [-0.83, 0.00, 2.82, -3.56, -25.62] | [-0.69, 0.72, 5.97, -3.56, -24.89] | [-0.14, -0.72, -3.15, 0.00, -0.73] | false |
| wm-hd | TaurenMale | [0.00, 0.00, 7.75, 0.48, -12.30] | [-0.69, 0.72, 0.11, 0.48, -17.99] | [0.69, -0.72, 7.64, 0.00, 5.69] | false |
| wm-hd | TaurenFemale | [0.00, 0.00, -2.51, 0.75, -23.01] | [-0.69, 0.72, -1.74, 0.75, -18.35] | [0.69, -0.72, -0.77, 0.00, -4.66] | false |
| wm-hd | GnomeMale | [0.00, 0.00, -0.47, -0.28, 2.19] | [-0.69, 0.72, -2.70, -0.28, -4.54] | [0.69, -0.72, 2.23, 0.00, 6.73] | false |
| wm-hd | GnomeFemale | [0.00, 0.00, 1.47, -0.20, 7.47] | [-0.69, 0.72, -4.89, -0.20, -2.22] | [0.69, -0.72, 6.36, 0.00, 9.69] | false |
| wm-hd | TrollMale | [0.00, 0.00, 5.30, -2.28, -21.32] | [-0.69, 0.72, 0.49, -2.28, -17.05] | [0.69, -0.72, 4.82, 0.00, -4.27] | false |
| wm-hd | TrollFemale | [0.00, 0.00, 0.30, -3.41, -28.12] | [-0.69, 0.72, 1.87, -3.41, -19.04] | [0.69, -0.72, -1.56, 0.00, -9.08] | false |
| wm-hd | BloodElfMale | [0.00, 0.00, 0.20, -3.99, -19.86] | [-0.69, 0.72, 0.01, -3.99, -16.40] | [0.69, -0.72, 0.19, 0.00, -3.46] | false |
| wm-hd | BloodElfFemale | [0.02, -0.14, 0.21, -0.30, -19.95] | [-0.69, 0.72, 3.63, -0.30, -4.97] | [0.71, -0.85, -3.42, 0.00, -14.98] | false |
| wm-hd | DraeneiMale | [-3.08, -0.08, 1.01, -6.83, -27.78] | [-0.69, 0.72, -4.96, -6.83, -30.49] | [-2.38, -0.79, 5.97, 0.00, 2.71] | false |
| wm-hd | DraeneiFemale | [0.39, -0.00, -3.26, -0.19, -40.21] | [-0.69, 0.72, -2.23, -0.19, -30.66] | [1.08, -0.72, -1.03, 0.00, -9.56] | false |

## Leg fixture: final contact and hinge revision

All listed lifted-left shin errors at 27110:14 fell to 0.0° with grounded and free solves agreeing. The first pass still failed wm-hd Gnome male right shin at Run 20100:0 (77.7°, ER toe Y 0.341, foot nearly vertical). A follow-up rejects sideways hip spacers and gates sole contact on forward sole orientation. The follow-up passed on all 40 combinations. Limits: thigh and shin ≤70°, planted feet (donor toe ≤0.02 yd above floor after scaling) must face forward, seat drift <0.003 yd, and skinned helper drift <0.0001 yd. All rows checked 10,429 heavy frames; skin and seat drift rounded to zero.

| Profile | Rig | Thigh max ° | Shin max ° | Planted forward dot |
|---|---|---:|---:|---:|
| base | HumanFemale | 15.1 | 12.5 | 0.492 |
| base | HumanMale | 16.6 | 13.8 | 0.492 |
| base | OrcFemale | 13.1 | 11.0 | 0.492 |
| base | OrcMale | 10.1 | 8.1 | 0.492 |
| base | DwarfFemale | 16.9 | 16.4 | 0.492 |
| base | DwarfMale | 16.0 | 13.3 | 0.492 |
| base | NightElfFemale | 13.7 | 11.7 | 0.492 |
| base | NightElfMale | 15.7 | 12.2 | 0.492 |
| base | ScourgeFemale | 5.3 | 5.4 | 0.492 |
| base | ScourgeMale | 14.3 | 11.7 | 0.492 |
| base | TaurenFemale | 10.9 | 8.4 | 0.492 |
| base | TaurenMale | 24.5 | 42.0 | 0.492 |
| base | GnomeFemale | 13.3 | 22.9 | 0.492 |
| base | GnomeMale | 26.5 | 19.6 | 0.492 |
| base | TrollFemale | 8.8 | 8.3 | 0.492 |
| base | TrollMale | 19.3 | 18.1 | 0.492 |
| base | BloodElfFemale | 4.3 | 4.4 | 0.492 |
| base | BloodElfMale | 13.1 | 11.1 | 0.492 |
| base | DraeneiFemale | 15.2 | 19.7 | 0.492 |
| base | DraeneiMale | 9.7 | 12.9 | 0.492 |
| wm-hd | HumanFemale | 15.2 | 12.6 | 0.492 |
| wm-hd | HumanMale | 3.4 | 3.8 | 0.492 |
| wm-hd | OrcFemale | 9.2 | 7.6 | 0.492 |
| wm-hd | OrcMale | 15.2 | 13.3 | 0.492 |
| wm-hd | DwarfFemale | 14.4 | 13.4 | 0.492 |
| wm-hd | DwarfMale | 9.9 | 9.3 | 0.492 |
| wm-hd | NightElfFemale | 11.4 | 10.1 | 0.492 |
| wm-hd | NightElfMale | 16.6 | 13.1 | 0.492 |
| wm-hd | ScourgeFemale | 6.9 | 7.0 | 0.492 |
| wm-hd | ScourgeMale | 3.0 | 3.7 | 0.492 |
| wm-hd | TaurenFemale | 7.4 | 7.7 | 0.492 |
| wm-hd | TaurenMale | 15.4 | 14.5 | 0.492 |
| wm-hd | GnomeFemale | 14.3 | 28.3 | 0.492 |
| wm-hd | GnomeMale | 24.9 | 26.7 | 0.492 |
| wm-hd | TrollFemale | 9.9 | 9.1 | 0.492 |
| wm-hd | TrollMale | 17.5 | 16.9 | 0.492 |
| wm-hd | BloodElfFemale | 4.5 | 4.5 | 0.492 |
| wm-hd | BloodElfMale | 9.3 | 8.2 | 0.492 |
| wm-hd | DraeneiFemale | 14.9 | 19.4 | 0.492 |
| wm-hd | DraeneiMale | 12.0 | 12.7 | 0.492 |

Left leg bind landmarks at 27110:14 before the hip-spacer follow-up, hip/knee/ankle; each entry is bone←parent (x, y, z), yards:

- base DwarfFemale: 5←3 (-0.132, 0.721, 0.102); 11←5 (-0.161, 0.374, 0.033); 20←11 (-0.179, 0.113, 0.130)
- base DwarfMale: 7←3 (-0.139, 0.724, 0.053); 16←7 (-0.175, 0.413, 0.070); 22←16 (-0.209, 0.109, 0.087)
- base TaurenFemale: 5←3 (-0.194, 1.161, 0.000); 14←5 (-0.212, 0.718, -0.164); 44←26 (-0.250, 0.306, 0.054)
- base TaurenMale: 6←3 (-0.189, 1.005, -0.003); 14←6 (-0.233, 0.620, -0.156); 46←26 (-0.242, 0.302, 0.039)
- base GnomeFemale: 5←3 (-0.135, 0.326, 0.089); 12←5 (-0.156, 0.184, 0.080); 23←12 (-0.174, 0.065, 0.113)
- base GnomeMale: 5←3 (-0.114, 0.324, 0.036); 14←5 (-0.127, 0.200, 0.025); 21←14 (-0.139, 0.082, 0.063)
- base DraeneiFemale: 7←2 (-0.100, 1.336, -0.050); 15←7 (-0.155, 0.756, -0.120); 21←15 (-0.181, 0.481, 0.065)
- wm-hd DwarfFemale: 24←7 (-0.132, 0.721, 0.102); 30←24 (-0.157, 0.396, 0.078); 47←41 (-0.179, 0.113, 0.130)
- wm-hd DwarfMale: 8←2 (-0.139, 0.724, 0.053); 20←8 (-0.175, 0.413, 0.063); 41←30 (-0.209, 0.109, 0.087)
- wm-hd TaurenFemale: 8←2 (-0.194, 1.161, 0.000); 16←8 (-0.223, 0.715, -0.166); 35←25 (-0.250, 0.306, 0.054)
- wm-hd TaurenMale: 6←2 (-0.201, 1.002, -0.003); 14←6 (-0.227, 0.612, -0.105); 34←23 (-0.248, 0.300, 0.039)
- wm-hd GnomeFemale: 16←4 (-0.099, 0.326, 0.089); 21←16 (-0.121, 0.183, 0.080); 39←33 (-0.138, 0.066, 0.113)
- wm-hd GnomeMale: 7←2 (-0.044, 0.409, 0.029); 13←7 (-0.114, 0.324, 0.036); 19←13 (-0.126, 0.201, 0.025)
- wm-hd DraeneiFemale: 6←2 (-0.100, 1.336, -0.050); 19←6 (-0.153, 0.757, -0.120); 27←19 (-0.182, 0.482, 0.065)

The wm-hd Gnome male left chain changed from 7/13/19 to 13/19/34, with parents 7/13/28 for the new hinges. Bone 7 to 13 falls only 0.084 yd while moving 0.070 yd sideways: the old height test alone classified that spacer as thigh. The new chain gate rejects it. New left hip, knee and ankle pivots: (-0.114, 0.324, 0.036), (-0.126, 0.201, 0.025), (-0.140, 0.082, 0.063).
