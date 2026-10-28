# OC — Wrath animation parity: Rotation, looping and setters

Date: 2026-10-28
Owner: OpenCode
Base HEAD: `7727a591` (closed stock-UI animation goal)
Goal: cover the animation kinds and run controls the installed addons actually use, starting with
the frame spinners, so animation parity extends past the stock UI without regressing it.

**Status: complete (2026-10-28).** Every animation verb or kind with stock or installed-addon
evidence is implemented and covered; the remaining items have no caller and are listed below.

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
- The getters/setter addons call: `GetDuration` (4 uses), `GetElapsed` (2), `GetProgress` (2) and
  `SetToFinalAlpha` (2; a stopped alpha lands on its final value).
- Tests for the orbit, the repeat restart, the bounce reverse leg and the run readback.

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

- `cargo test -p benilla-ui --lib 'animation::tests'`: 7 passed, including
  `a_frame_rotation_orbits_its_region_quad` (center (58,50) → (55.657,55.657) at 45°, quad angle
  45° CCW), `a_repeating_group_restarts_its_rotation` (360° loop reaches a quarter turn after
  restart and stays playing), `a_bouncing_group_reverses_its_rotation` (forward leg 90°, reverse
  leg half home at 45°, then 0°) and
  `the_animation_getters_report_a_run_and_final_alpha_lands`.
- `cargo check -p benilla-app --lib` clean.
- Wrath stock smoke (real wm-hd): passes, including the stock `TutorialFrameCallOutPulser` step,
  before and after the bounce work.
- Full UI suite not rerun after this slice; the earlier two host-timezone stdlib failures are
  unrelated.

## Next (no evidence yet; not started)

- Rotation-aware hit tests (only draw quads rotate today).
- Region-owned rotation (installed addons rotate frames).
- `Scale`/`Color`/`TextureCoord`/`VertexColor` kinds (neither stock nor installed addons
  construct them).
- `BOUNCE` on non-Angle visual proof in a live run (unit-tested only).

## WP1 claim (2026-10-28, Questie timer; source-only)

Claim: `crates/benilla-ui/src/script/animation.rs` and this checkpoint only. Astra's live lane
still owns `object/events_regions.rs`, so this slice does not touch
`CreateAnimationGroup`'s publication.

Contracted edits:

- `AnimationGroup:CreateAnimation([type[, name]])` treats an omitted or explicit nil `type` as
  the reference's default animation kind, `"Alpha"`; a supplied string/number keeps the existing
  coercion and a bad value keeps the existing usage error. Questie-335
  `Compat/Compat.lua:166` calls `anim:CreateAnimation()` with no type.
- `Animation:GetParent()` returns the stored `ANIMATION_PARENT` (a child animation's group), or
  `ANIMATION_OWNER` for a top-level group (the owning frame/region). Questie-335
  `Compat/Compat.lua:136` calls `timer:GetParent():Stop()`.

Proposed regression case (documented, **not added or run**: this session has no test-case grant):

```lua
f = CreateFrame("Frame")
g = f:CreateAnimationGroup()
a = g:CreateAnimation()                 -- omitted type
a:SetDuration(0.05)
HITS = 0
a:SetScript("OnFinished", function() HITS = HITS + 1 end)
p = a:GetParent()
assert(p == g)
assert(g:GetParent() == f)
g:SetLooping("REPEAT")
g:Play()
-- after one duration: HITS == 1
-- after another: HITS == 2
g:Stop()
-- after another duration: HITS still 2
```

Timer-lifecycle review against current source: creation reuse stores the child in
`inactiveTimers`; `Stop` halts the group and its children so the repeat branch does not restart;
a cached child's `SetDuration`/callback/id fields are overwritten on reuse; `Play` resets every
child's run state. The cancellation-from-`OnFinished` ordering is safe because `update` checks the
group's `playing` flag after the child callback. No further source gap was found in the engine for
Questie's timer usage. Installed-addon load acceptance remains open (no grant).
