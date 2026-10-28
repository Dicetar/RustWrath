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

- A scripted stock-style animation (`OnLoad` → Translation, and `OnShow` → Alpha) advances
  deterministically on `Model::now` and `OnFinished` fires exactly once.
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

## Execution status

Source-only under the campaign's standing restriction: no build, test, lint, generator, or
graphical run yet. Success criteria above are unverified until execution is authorized. The
first tracer is `OnLoad → Translation` on a Texture with rect movement asserted over time.

## First tracer (next commit)

1. Give every animation object a stable id and mirror its parameters into a `Model`-owned
   registry at build time (the Lua table keeps the script surface).
2. Advance playing/triggered animations in the tick from `Model::now`; apply Translation to the
   owner's offset and Alpha to its alpha.
3. Fire `OnPlay` when a run starts and `OnFinished` once when it ends; `Stop` fires `OnStop`.
4. Trigger `OnLoad` at frame load and `OnShow`/`OnHide` from the visibility walk.
