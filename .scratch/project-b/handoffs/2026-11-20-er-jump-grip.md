# Jump attack and weapon grip

The November 19 22:19 recording and its interaction diagnostics identify build `c54d862e` and the wm-hd profile. The recording shows repeated jump attacks and a visible separation between weapon and hands. It predates the first socket-translation fix. The recording does not identify the equipped kit.

## Character model

On HD Human female the right-hand attachment is bone 210, below helper 115 and wrist 62. The renderer composes the socket bone's posed transform with the attachment's local offset. The previous retargeter set helper translations during pose construction. Helper rotations, scales and later pose blends can still move the final point relative to the wrist.

The retargeter now reads each attachment's offset and composes the complete standing socket chain to get its grip point in wrist coordinates. After the action, gait and arm layers blend, it solves the socket translation against the current helper transforms. The same calculation runs for either hand on local and relayed rigs. Draw, stow and fade-out interpolate back to the stock attachment point.

An offline calculation on the installed HD Human female asset sampled Attack2HTight (18), variation 1, and jump clips 37, 38, 39 and 187 at 20 ms intervals. Maximum wrist-relative displacement was 0.782777 yd with the recorded build's transform rules and 0.001318 yd with the first translation fix. The new placement equation held the reference grip within 1e-9 yd in this calculation.

## Jump playback

The input handler could release and refund an airborne attack on early touchdown, then start a second attack from frame zero. After the hit it could cut the recovery short. An earlier launcher log shows JumpHeavy followed by JumpHeavyLand about 430 ms later, before the frame-15 hit at 500 ms. The installed greataxe air clips have 19 samples for actions spanning 67 light or 75 heavy frames. Holding the last air sample until the action ends also produces a frozen pose. Ground-height correction in the leg solver was active even in midair.

An accepted jump swing now keeps its original hit, stamina cost and recovery clock. Touchdown changes the visual clip using the two rows' hit opening, closing and end frames. A movement cancel can carry the upper body over the gait. Kits with only a landing row still submit it on touchdown. The leg solver uses the baked hip-to-ankle offset while airborne and the sole-contact height on the ground. Remote playback uses relayed movement flags and the same phase mapping.

## Owner's in-game check

Enable F10 Experimental with `jump_attacks`, `retarget` and `retarget_locomotion` on. On wm-hd Human female, repeat standing and running jump lights and heavies. Press once just after takeoff and again on a separate jump near touchdown. The knees should tuck in the air, the swing should continue across landing without a second windup, and the grip should stay in the palm through impact and recovery. Repeat a grounded heavy, then continue running through recovery. A second player with `server` enabled can check the relayed pose. Compare a two-handed and a paired kit, then a classic rig.

No local build, test suite or client launch for this follow-up. The numerical grip comparison uses installed rig data and does not measure rendered motion. In-game appearance, two-hand contact and peer timing remain for owner review.

## Branch CI

Linux and Windows compiled the changed test targets. Linux passed formatting, workspace Clippy and the live-code build. Its app library reported 6,838 passing tests, including all nine enabled retarget tests. The installed-rig fixture was ignored in CI. Both runners then failed the same combat-core parser test. Its fixture now emits a `stance` line and expects the malformed `total` field on line 10. The parser reported its actual position on line 11. Commit `a5668cf1` introduced the extra fixture line. The parser and its test are identical to the merged base `40649fd7`. [CI for 9f9001b3](https://github.com/Dicetar/RustWrath/actions/runs/44653039687)
