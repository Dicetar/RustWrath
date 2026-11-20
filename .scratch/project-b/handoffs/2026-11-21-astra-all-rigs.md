# Astra: all playable rigs

Branch `astra/all-rigs`, independently based on `d9d35507`. No main changes.

`installed_player_limbs` now visits both sexes of Human, Orc, Dwarf, Night Elf, Scourge, Tauren, Gnome, Troll, Blood Elf and Draenei on base and wm-hd. It keeps the independent landmarks for the original four rigs; the other rigs use the mapped hinges plus stock helper locals as the skin oracle. It discovers `Heavy*`, `RunHeavy` and `JumpHeavyLand` sources from installed kit rows and checks the attachment seat each baked frame. The `RIG` summary includes skin drift, maximum thigh/shin angle error, worst foot-facing dot, heavy frame count, stock grip drift and clavicle indices.

The requested sccache-backed ignored fixture was started with the installed `WOW_DATA` and `ERAN_FILE`; no per-rig pass, outlier repair, or green CI is claimed yet. This is fixture coverage, not evidence that all rigs pass. The original HumanFemale grip regression remains.

In game: check idle, moving guard and walk/run knees and feet on all available race/sex/profile combinations. Check heavy, running heavy and the jump-heavy landing with a visible weapon; palm contact should stay fixed and shoulder pads should follow stock clavicles without pinching. Try the stance and sheathed-gait switches separately on the stance branch. No client/server launch or login performed.
