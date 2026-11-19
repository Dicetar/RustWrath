# ER clips on classic and HD player limbs

Status: awaiting owner verification
Owner: Astra / Codex
Branch: astra/er-retarget-limbs
Issue: none
Next: Owner builds this branch and compares the idle, run and crouch on the base and wm-hd profiles.

Based on `8b7a42fd`. Local and relayed players use the same pose solver.

## Findings from the installed assets

On the first frame of `a000_000000`, the ER pelvis-to-spine direction is 3.1° from vertical. The old solver used the direction to the first M2 spine helper as the pelvis's bind up. On both Human female models that helper is 0.0092 yards below the pelvis. The resulting bind-to-idle direction error was 163.0° before any leg solve. Its rotation was inherited by both hip sockets.

| Bind up versus idle ER spine | Human female | Human male | Orc male | Tauren male |
| --- | ---: | ---: | ---: | ---: |
| base | 163.0° | 163.0° | 163.0° | 131.7° |
| wm-hd | 163.0° | 15.1° | 155.9° | 4.8° |

The measurements use the same character model paths through the two profile manifests. The HD Human female ankle has two consecutive pivots at 0.09 yards. HD Orc male has a short helper above the actual thigh. On classic Tauren male, the knee, hock and hoof pivots are at 0.62, 0.49 and 0.30 yards. There is no separate toe pivot. Its HD counterpart has an additional pivot at 0.11 yards. Mapping the last pivot to the ER toe by height cannot describe both hooves. The idle ER right ankle is 0.111 m high with the toe at 0.001 m, even with the foot planted. Applying that downward vector directly to the shoe conflates its bind pitch with heel lift.

## Changes

The pelvis now uses anatomical up. The two-link leg solve uses each rig's thigh and effective shin length, targets the baked toe contact height through the rig's sole offset, and bends toward the ER knee. The Tauren hock keeps its bind bend. Coincident deformation pivots follow their limb segment. Foot pitch is measured relative to the planted ER idle foot.

The local hatchet mesh extends from −0.271 to +0.825 yards along raw M2 X. Its long direction is model −Z, while the ER shaft is the baked weapon's +Y axis. Build 12340's `CM2Model::GetAttachmentWorldTransform` (0x00831410, reference e67b6ffe) uses the attachment bone's matrix and a position offset. The pass now orients the distinct weapon socket to the sampled axes and preserves each wrist's thumb/knuckle frame. Older bakes with head axes only keep the hand-based orientation. Both passes retain stock channels independently for fade-out, and remote bodies use the current frame's helper transforms.

## Verification and in-game check

Measured bind pivots on all eight installed rigs, read the local ERAN header and sample positions, and measured a one-handed hatchet. The sccache-backed Cargo check finished on the starting source. The final source was formatted. Final compilation, Clippy, focused tests and visible motion remain unverified locally. The owner requested immediate handoff for the gameplay check.

Enable Souls mode with F10 and set `retarget = true` and `retarget_locomotion = true` in `benilla-config/combat/souls.toml`. Start with Human female and a drawn one-handed axe, then compare Human male, Orc male and Tauren male on both profiles. Observe a relayed player's idle/run and roll as well.

- Front idle/guard (`a000_000000` and `a000_000100`): planted soles, knees following the feet and axe bit following the guard hand.
- Rear walk/run (`a000_020000` and `a000_020100`): alternating support and heel lift, with the support foot facing along its step and the axe staying in the hand.
- Crouch (`a000_300000`) and forward roll (the kit's `RollFront`): pelvis descends over flexed knees. Feet may lift during the roll. Check that the low pose recovers without a lateral split, including on the Tauren hocks.
