# Astra: all playable rigs

Branch `astra/all-rigs`, independently based on `d9d35507`. No main changes.

`installed_player_limbs` now visits both sexes of Human, Orc, Dwarf, Night Elf, Scourge, Tauren, Gnome, Troll, Blood Elf and Draenei on base and wm-hd. It keeps the independent landmarks for the original four rigs; the other rigs use the mapped hinges plus stock helper locals as the skin oracle. It discovers `Heavy*`, `RunHeavy` and `JumpHeavyLand` sources from installed kit rows and checks the attachment seat each baked frame. The `RIG` summary includes skin drift, maximum thigh/shin angle error, worst foot-facing dot, heavy frame count, stock grip drift and clavicle indices.

The sccache-backed `installed_player_limbs -- --ignored --nocapture` passed (1 test; all 40 profile/rig combinations). CI run [44782443533](https://github.com/Dicetar/RustWrath/actions/runs/44782443533) was still in progress at the last check. The original HumanFemale grip regression remains.

In game: check idle, moving guard and walk/run knees and feet on all available race/sex/profile combinations. Check heavy, running heavy and the jump-heavy landing with a visible weapon; palm contact should stay fixed and shoulder pads should follow stock clavicles without pinching. Try the stance and sheathed-gait switches separately on the stance branch. No client/server launch or login performed.

## Fixture results

All rows: 10,429 baked heavy frames checked, zero helper skin and wrist-space Stand seat drift to 6 decimal places, and forward-facing feet at idle. The minimum forward dot across sampled poses was -0.307 on every rig at 27110:14 (lifted foot); this is not an idle reversal. The max segment angle is a diagnostic, not a pass threshold. Largest shin deviations on Gnome, Tauren and Draenei female occur on the raised-foot sample with the grounded solve forced on. No production change is claimed for those outliers; next isolate the contact constraint versus a chain-selection issue with a matching gameplay sample.

| Profile | Rig | Max segment angle (deg) | Clavicles R, L |
|---|---|---:|---|
| base | HumanFemale | 14.7 | 17, 16 |
| base | HumanMale | 25.3 | 11, 10 |
| base | OrcFemale | 21.1 | 13, 12 |
| base | OrcMale | 52.5 | 13, 12 |
| base | DwarfFemale | 36.5 | 16, 15 |
| base | DwarfMale | 50.0 | 13, 12 |
| base | NightElfFemale | 13.7 | 13, 12 |
| base | NightElfMale | 28.7 | 13, 12 |
| base | ScourgeFemale | 5.7 | 13, 12 |
| base | ScourgeMale | 24.9 | 13, 12 |
| base | TaurenFemale | 71.2 | 20, 19 |
| base | TaurenMale | 132.9 | 18, 19 |
| base | GnomeFemale | 126.5 | 11, 10 |
| base | GnomeMale | 142.0 | 13, 12 |
| base | TrollFemale | 8.9 | 11, 10 |
| base | TrollMale | 20.7 | 13, 14 |
| base | BloodElfFemale | 4.8 | 22, 19 |
| base | BloodElfMale | 22.9 | 21, 20 |
| base | DraeneiFemale | 85.7 | 28, 25 |
| base | DraeneiMale | 20.5 | 23, 25 |
| wm-hd | HumanFemale | 14.9 | 35, 34 |
| wm-hd | HumanMale | 22.8 | 27, 26 |
| wm-hd | OrcFemale | 21.2 | 23, 24 |
| wm-hd | OrcMale | 39.6 | 24, 25 |
| wm-hd | DwarfFemale | 37.4 | 33, 32 |
| wm-hd | DwarfMale | 45.3 | 32, 33 |
| wm-hd | NightElfFemale | 11.7 | 21, 22 |
| wm-hd | NightElfMale | 33.6 | 21, 23 |
| wm-hd | ScourgeFemale | 7.0 | 35, 32 |
| wm-hd | ScourgeMale | 20.6 | 27, 28 |
| wm-hd | TaurenFemale | 42.9 | 28, 26 |
| wm-hd | TaurenMale | 139.8 | 30, 28 |
| wm-hd | GnomeFemale | 142.9 | 27, 28 |
| wm-hd | GnomeMale | 136.2 | 21, 22 |
| wm-hd | TrollFemale | 9.5 | 31, 30 |
| wm-hd | TrollMale | 18.9 | 31, 30 |
| wm-hd | BloodElfFemale | 9.0 | 22, 21 |
| wm-hd | BloodElfMale | 31.5 | 25, 24 |
| wm-hd | DraeneiFemale | 94.7 | 31, 34 |
| wm-hd | DraeneiMale | 17.1 | 29, 28 |
