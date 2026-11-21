Astra, an addition to the ER legs task (branch `astra/er-legs`). It is also in `retarget.rs`, so it
is yours.

## Bug: in heavy attacks the weapon sometimes leaves the hand (owner, Firna, wm-hd Human female)
Owner's screenshots, described:
- A one-handed axe/mace during a heavy attack: the weapon hangs about an arm's length out from the
  hand, beside the body. The hand is empty.
- A sword at the end of a heavy attack: the blade lies along the ground by the feet. The hand is up
  at the chest.

"Sometimes": a few frames of a heavy, not every swing. The light chain looked fine.

## Where to look (Claude's reading, not tested)
- `Drive::Grip { side }` (retarget.rs, built where the comment says "Only rotate the actual
  attachment bone", about line 1001; solved about line 1126) gives the hand socket bone
  (`rig.hands[side]`) an absolute model rotation from ER's `R_Weapon`/`L_Weapon` axes
  (`grip_rotation`).
- The rotation is about the socket bone's own pivot. On HD rigs the hand attachment can hang from a
  bone whose pivot is at the model origin (see the wrist comment near line 768). Rotating that bone
  swings the attachment point round the origin. The weapon then lands an arm's length from the
  hand, and a big wrist turn (a heavy's wind-up or follow-through) makes it worse.
- Also check frames where `grip_rotation` returns `None` (opposing samples cancel an axis). The
  socket then falls back to the stock rotation for that frame, a snap under a far pivot.

## Goal
The attachment point stays at the hand for every frame of every attack, on all the rigs in the
main task. Only the weapon's orientation follows ER. For example, rotate the socket about the
attachment position rather than its pivot (pivot-corrected rotation), or carry the last good grip
over a `None` frame.

## Fixture
Extend `installed_player_limbs` so it fails on this: for each `Heavy*` and `RunHeavy`/`JumpHeavyLand`
clip frame, the hand attachment's model position must stay within a few centimetres of the wrist.
