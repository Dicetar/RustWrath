# OC — Wrath animation parity: Rotation, looping and setters

Date: 2026-10-28
Owner: OpenCode
Base HEAD: `7727a591` (closed stock-UI animation goal)
Goal: cover the animation kinds and run controls the installed addons actually use, starting with
the frame spinners, so animation parity extends past the stock UI without regressing it.

## Evidence

- Installed AddOns construct only `Alpha` (6), `Rotation` (2) and `Translation` (1); no `Scale`,
  `Color`, `TextureCoord` or `VertexColor` constructors.
- Details' two spinners are frame-owned `Rotation` groups: `janela_principal.lua:2282-2289`
  (`f:CreateAnimationGroup()`, `CreateAnimation("Rotation")`, `SetDegrees(360)`,
  `SetDuration(2)`, `SetLooping("repeat")`, texture inside `f`) and `:3092-3096` with duration 6.
- Stock `TutorialFrame.xml:152` declares `looping="BOUNCE"`, which the engine previously ignored.

## Scope

In:
- `Rotation` on a frame owner: quads in the frame's subtree orbit the rotated frame centers
  (outermost first) and carry the summed angle; text and region quads included.
- `SetDegrees` and `SetLooping("REPEAT"|"BOUNCE"|"NONE")`, with the XML `looping` attribute read at
  build.
- `REPEAT` restarts a finished group's schedule; `BOUNCE` alternates the leg direction, each leg
  capturing the value it starts from (offset-based kinds reset to the un-animated place on a
  fresh forward leg).
- Tests for the orbit, the repeat restart and the bounce reverse leg.

Out / known limits (recorded, not hidden):
- Stock Lua plays the pulser child directly, so stock does not exercise the group loop.
- Region-owned `Rotation` is not applied (no installed usage).
- Hit tests still use the unrotated rects; only draw quads rotate.
- Repeating `Alpha` groups drift because each run captures the owner's alpha as its base.

## File claim

- `crates/benilla-ui/src/script/types.rs` (`ExtractedQuad::rotation`)
- `crates/benilla-ui/src/script/extract.rs` (orbit post-pass)
- `crates/benilla-ui/src/script/messageframe/mod.rs` (new field in its quad)
- `crates/benilla-ui/src/script/model.rs` (`animation_frame_rotations`)
- `crates/benilla-ui/src/script/animation.rs` (Rotation, SetDegrees/SetLooping, repeat)
- `crates/benilla-app/src/ui_script/extract/mod.rs` (carry the angle onto emitted quads)
- this checkpoint

## Verification (2026-10-28)

- `cargo test -p benilla-ui --lib 'animation::tests'`: 6 passed, including
  `a_frame_rotation_orbits_its_region_quad` (center (58,50) → (55.657,55.657) at 45°, quad angle
  45° CCW), `a_repeating_group_restarts_its_rotation` (360° loop reaches a quarter turn after
  restart and stays playing) and `a_bouncing_group_reverses_its_rotation` (forward leg 90°, reverse
  leg half home at 45°, then 0°).
- `cargo check -p benilla-app --lib` clean.
- Wrath stock smoke (real wm-hd): passes, including the stock `TutorialFrameCallOutPulser` step,
  before and after the bounce work.
- Full UI suite not rerun after this slice; the earlier two host-timezone stdlib failures are
  unrelated.

## Next

Rotation-aware hit tests, region-owned rotation, then the (unused) Scale/Color/TextureCoord kinds.
