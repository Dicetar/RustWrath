# OC — Wrath stock UI animation timeline

Date: 2026-10-28
Owner: OpenCode (client/gameplay integration lead)
Base HEAD: `4a7d5a2f` (Astra's tooltip commits on top of this campaign's `b89af3a2`)
Goal: make the 3.3.5a stock UI animate on the reference's timeline, so `<Animations>` reach the
screen instead of sitting static, and every stock animation script (`OnPlay`/`OnStop`/
`OnFinished`) runs when the reference runs it.

## Why this goal

The load slice, the `Cooldown` widget and the bootstrap false-answer removal are done. The
largest remaining stock-UI fidelity gap in the checkpoint trail is that the animation object
graph exists but no engine advances it: `script/animation.rs` stores XML parameters and a
playing flag, and `Play`/`Stop` only flip that flag. Action-button flashes, `UIParent` fades,
TutorialFrame call-outs and every addon animation are therefore static.

This is engine/UI-owned work, not Astra's item/container lane.

## Current state (source-verified)

- `crates/benilla-ui/src/loader/animations.rs` builds every `<Animations>` child for the Wrath
  target: names and `parentKey` published, `<Scripts>` compiled and stored through `SetScript`,
  group children built, all params kept as strings in `__benilla_animation_params`.
- `crates/benilla-ui/src/script/animation.rs` gives each object `Play`, `Stop`, `IsPlaying`,
  `GetName`, `GetRegionParent`, `SetScript`, `GetScript`; no timeline, no interpolation, no
  trigger, no signal firing.
- The frame/region update walk and `UiScript::extract` run on `Model::now`, the same `GetTime`
  clock the cooldown sweep uses; nothing reads animation state there.
- Local 12340 FrameXML (`F:\AI\tmp\opencode\full\Interface`) shows the stock mix is narrow:
  14 `AnimationGroup`, 20 `Alpha`, 2 `Translation`; no `Rotation`/`Scale`/`TextureCoord`/
  `VertexColor` animation tags in that tree. So the stock-critical kinds are Alpha and
  Translation, with the group/trigger machinery shared by all kinds.

## Scope

In:
- Timeline state per animation: duration, start delay, order, smoothing, `change`/offset,
  loops, playing/paused/finished; the group's own delay/spread and child ordering.
- Triggers: `OnLoad` (at frame load), `OnShow`/`OnHide` (with the frame's own transitions),
  `OnEvent` where the stock XML declares it; `Play`/`Stop`/`Pause`/`SetDuration`/`SetOffset`
  API.
- Signals: `OnPlay`, `OnStop`, `OnFinished` (once per run), fired through the stored scripts.
- Effects: Alpha (frame/region alpha) and Translation (frame/region offset) first; Rotation,
  Scale, Color and TextureCoord only where the reference contract is recovered.
- Extract: animated values are the state the extract reads, so no renderer change should be
  needed for Alpha/Translation.

Out:
- Renderer-side animation, particle systems, model animation, sound cues.
- Addon-specific behavior beyond the API.

## Success criteria

- A scripted stock-style animation (`Play` → Alpha/Translation, as the stock Lua drives them)
  advances deterministically on `Model::now` and `OnFinished` fires exactly once.
- `UIParent`'s fade and an ActionButton flash load and run in the stock smoke with no animation
  faults.
- No stock XML reports an unknown animation node or a missing script handler.

## Evidence plan

- The 1.12 reference implementation (benilla-re) is not cloned in this workspace. Clone it
  beside this repo (or use `https://samwhosung.github.io/benilla-re/?<addr>`) when implementing
  the timeline: the animation engine's update, trigger and signal addresses are the contract.
- The 12340 FrameXML at `F:\AI\tmp\opencode\full\Interface` supplies the grammar and the stock
  usage set (do not invent attributes it never writes).

## File claim

Editing:
- `crates/benilla-ui/src/script/animation.rs` (engine, signals, API)
- `crates/benilla-ui/src/loader/animations.rs` (registration at build time, `OnLoad`)
- `crates/benilla-ui/src/widget/mod.rs` and/or `widget/kinds/mod.rs` (animation registry/state
  handle if it lives in the arena)
- `crates/benilla-ui/src/script/tick.rs` (the update walk's animation step)
- `crates/benilla-ui/src/script/extract.rs` (only if animated state must be projected)
- this checkpoint

Coordinating: Astra's item/container/addon lane is untouched; her commits below `b89af3a2` stay.

## Execution status (verified 2026-10-28, execution authorized)

- `cargo check --locked --offline -p benilla-ui` clean (only the pre-existing deprecated
  `Atomic::fetch_update` warnings).
- `cargo test --locked --offline -p benilla-ui --lib 'animation::tests'`: both new tests pass.
  `alpha_groups_sequence_and_fire_finished_once` proves the ordered fade-out/fade-in, the
  per-animation and group `OnFinished` firing exactly once, and no refire. The translation test
  proves the frame rect half-moves and lands through `frame_rect`/extract.
- Full `benilla-ui` lib suite: 3332 passed, 2 failed — `script::tests::stdlib` date/time zone
  tests, which the 2026-10-26 Astra verification log already records failing on this host
  (environmental, untouched by this work).
- Wrath stock smoke (real wm-hd data): **passes** with an absolute `WM_ASSET_MANIFEST`; the
  relative `docs/profiles/wm-hd.mpqlist` fails to open from the test process, so acceptance runs
  should pass the absolute path.
- `ui_trade` tests: 45 passed after restoring the stale `snapshot`/`resolve_slot` call shapes that
  blocked every app-lib test build (test-only fix, no behaviour change).

The smoke loads the stock UI to PlayerFrame.xml; it does not itself play a stock animation. The
engine's proof is the two focused tests above.

## Implemented (Alpha tracer, uncommitted at the time of writing)

- `script/animation.rs`: a named-registry timeline. `Play` on a group schedules its children in
  `order` batches (parallel within an order, `startDelay` honoured, later batches starting when
  the previous batch's longest run ends); a lone animation starts at `now + startDelay`.
- The tick (`script/tick.rs`, after the frame walk) advances every playing animation on
  `Model::now`: an `Alpha` run captures the owner frame's alpha at its own start and writes
  `from + change * t` through `WidgetArena::set_alpha`, so ordered fades chain correctly.
- `OnPlay` fires when a run actually starts, `OnFinished` once at the end, `OnStop` on `Stop`;
  a group completes when every child has finished. Handlers get the animation table as `self`,
  so `self:GetRegionParent()` is the frame/region that declared the block.
- The loader now passes the declaring frame/region as a child's owner (previously the group, so
  `GetRegionParent` was wrong) and links a group's children in document order.
- `SetStartDelay(seconds)` writes the run's `startDelay` parameter, which is what stock
  `AlertFrames` Lua calls on its `animOut` child before `Play` (five call sites).

Alpha applies to both owners: a frame through `WidgetArena::set_alpha`, a region through
`RegionData::alpha` (the same slot `Region:SetAlpha` writes), so texture- and font-string-owned
groups such as `AlertFrames.xml`'s texture animations work.

`Translation` moves the owner (frame or region) through `Model::animation_frame_offsets` /
`animation_region_offsets`, which `frame_rect`, `region_rect` and `region_kept_rect` add at read
time (a frame's chain includes its ancestors, so the subtree rides together; draw and hit tests
share the readers).

Known limits of this tracer: the translation is added in root units after the space transform,
so a frame scaled away from 1 moves by an unscaled offset; frames anchored to a translated frame
(the layout, not the subtree) do not follow it; `Rotation`/`Scale`/`Color`/`TextureCoord` run
their timeline and signals but apply no visual change; easing (`smoothing`) is linear; the
group's static batch schedule assumes durations do not change mid-run.

## First tracer (next commit)

1. Give every animation object a stable id and mirror its parameters into a `Model`-owned
   registry at build time (the Lua table keeps the script surface).
2. Advance playing animations in the tick from `Model::now`; apply Translation to the owner's
   offset and Alpha to its alpha.
3. Fire `OnPlay` when a run starts and `OnFinished` once when it ends; `Stop` fires `OnStop`.

Verified while scoping: the stock 12340 XML declares no `trigger=` attributes and no
`OnLoad`-triggered groups; stock Lua calls `group:Play()` (often from its frame's `OnShow`).
So the engine's trigger input is the existing `Play`/`Stop` verbs, not an XML trigger parser.
A group's `Play` must cascade to its child animations in `order`, honouring `startDelay`.
