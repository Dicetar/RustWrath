# Firna graphics repair and playable route

OpenCode keeps playable combat. Astra owns graphics and native reload integration. Canonical
repository is https://github.com/Dicetar/RustWrath; work in the existing checkout on `wotlk`.

## Concrete graphics finding

The production Wrath bone parser read sequence-local timestamp/key offsets in the main M2 even
when the sequence payload lives in a sibling `.anim`. Real wm-hd HumanFemale Talk (ID 60,
variation 0, slot 131, flags 0x81) reproduced 15 bone-39 translation keys with five non-finite
values. With its actual external payload the same keys contain zero non-finite values.

Reference pin e67b6ffef23655dc7da061cb2d36b7963f6ef71a:
`Engine/Source/Model2/M2Shared.cpp`, M2Init track relocation, MakeAnimFileName (0x00835A20),
LoadLowPrioritySequence (0x0083DA10). Inline flag 0x20 selects the model buffer; aliases 0x40
follow their source slot. Sibling naming is stem + four-digit animation ID + '-' + variation.
This affects stock as well as HD models; the previous stock override did not solve decoding.

Astra changes: source-aware bone/event decoding, bounded alias resolution, omit unavailable
external clips, nested global-track decoding, and Bevy read_asset_bytes for both animation and
skin siblings. The latter preserves the model's source and records loader dependencies.
No install writes or decompiled implementation copied.

Focused parser verification: four Wrath cases passed. Real selected HD Talk regression now
reports the same 15 translation keys with nonfinite=0. Evidence remains ignored under
`.scratch/project-b/verification/2026-10-28-oc-alpha/astra-external-anim-tests.log` and
`astra-talk-source-{before,after}.log`. No rendered repair claim yet. A current client build and
same-interaction capture are required; full asset hotpatching remains unfinished.

## Shortest playable milestone

1. Astra: build the current repair once, check Firna before/during/after NPC interaction.
2. OpenCode: complete target/name/health and action/combat state blockers in its claimed files.
3. Owner: drive Firna's warrior attack/kill leg when the client is ready; no automated combat.
4. Observe quest progress, loot, turn-in/reward, logout/relog persistence on that same character.

Do not broaden this into implementing every decompiled function or all content reload modes.
No playable-alpha claim until the connected route is observed. Commit explicit owned paths;
unfinished combat and hotpatch work must not be swept into another owner's commit.

## 2026-11-01 build and live handover

`cargo build -p benilla --features hotpatch` passed after fixing the loader's borrowed source ID.
The real-file diagnostic now checks all Talk channels: 121 animated bones, zero non-finite
components. This is decoder evidence, not a claim that every animation or visual is repaired.

Live run: `connected-run-20261101-001028-726.stderr.log`, Firna 0x1522 entered world with
276 entities. Binary SHA-256:
`5F4BD0DABE7B1252D9E67A62CD00BCC9013D3936112164C2661A67F6CD46B50A`.
It was built from 6edea9f5 plus the pending graphics/hotpatch changes. The initial live view
shows a coherent player body and wolves, without the previous screen-filling planes. No
non-finite rig warning was observed in this initial interval. Private memory sampled at
3.32 GB. This does not close NPC-interaction acceptance: Firna is currently near the wolves,
not beside the previous questgiver. `/talk` was typed through native key events, but no
gesture was established; the log reports an EmotesTextData.dbc 10-vs-18-field schema failure.
Do not claim that this command exercised Talk. Clipboard paste into chat also failed;
native individual keys worked. No movement, attacks or kills were automated.

The client was handed to the human owner, PID 49848, with the existing 20-minute/5-GiB wrapper
bound (approximately 00:30 Moscow time). Astra stops sending controls. Do not open a competing
account session. OpenCode's 6edea9f5 loot/bag/action-button fixes are included; their actual
combat/loot/reward/persistence acceptance remains pending.
