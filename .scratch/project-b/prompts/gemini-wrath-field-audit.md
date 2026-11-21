You are working on RustWrath (github.com/Dicetar/RustWrath, PUBLIC repo): a Rust/Bevy WoW client
for 3.3.5a build 12340 against AzerothCore. Long, uninterrupted session. Do not stop to report
progress; work until the task is finished or truly blocked, then write the task note.

## Setup
1. Clone/pull `origin`, read AGENTS.md, CONTEXT.md, docs/agents/workflow.md and docs/agents/sync.md.
2. Create branch `gemini/wrath-field-audit` from current `origin/main`. Never push to `main`, never
   force-push, never rebase shared branches. Commit and push your branch at every stopping point.
3. Do NOT touch: `crates/benilla-app/src/souls/**`, `combat_kit*`, `combat_playback*`,
   `combat_profiles*`, `combat_mode*`, `integrations/ale/**`, creature strike/clip/weapon-kit code.
   Those are owned by other agents (Claude: Souls/ER retarget; Astra: strikes, remote clips,
   weapon kits). If your fix needs one of them, write it in the task note instead.

## Task: Wrath update-field mapping audit, finished on current main
Your earlier branch `origin/argon/field-mapping-audit` (commit 04c16dbc, "project
GameObject/DynamicObject/Corpse and missing Unit/Player fields") is 190+ commits behind main and
was never landed. Redo it on current main; do not merge that branch, port its intent.

Goal: every Wrath 3.3.5a update field the client receives for Object, Item, Container, Unit,
Player, GameObject, DynamicObject and Corpse is either projected to the right donor field in
`crates/benilla-app/src/wrath.rs` (and protocol-wrath where the layout lives) or explicitly listed
as intentionally unmapped with a reason. Specifically:
- Gate UNIT_FIELD_* extraction/projection on Unit/Player objects only.
- Project GameObject (display id, flags, state/type/art kit, rotation, dynamic, faction, level),
  DynamicObject (caster, spell, radius, cast time) and Corpse (owner, display, item displays,
  bytes, flags, dynamic) fields.
- Project missing Unit fields (power cost/modifiers, pet fields, combat reach/bounding radius,
  mount display, bytes 0/1/2 pieces) and Player fields (honor/arena currency, kills, explored
  zones, skill and quest log ranges) that consumers read.
- Annotate each mapping with Wrath source field name and donor destination.
- Use the 12340 field offsets from the protocol-wrath crate; verify against
  docs/agents/12340-reference.md workflow. Do not invent offsets: if unsure, list as unverified.
- Remove nil/constant stubs you find on this path; name unsupported branches.

## Verification (strict)
- Add at most 20 focused unit test cases total (e.g. one projection test per object type, from
  a synthetic update block). Do not add broad suites.
- Run only: `cargo check -p benilla-app -p benilla-protocol-wrath`, `cargo fmt`, and your new
  tests by name filter. sccache is required (see AGENTS.md). No client launch, no server, no login.
- Run `cargo clippy` on the files you changed only and fix what you introduced.

## Finish
- Merge `origin/main` into your branch if it moved (merge, not rebase), recheck, push.
- Write the task note `.scratch/project-b/handoffs/2026-11-19-wrath-field-audit.md` in the existing
  handoff format (Status, Owner: Gemini, Branch, What changed, CI run id and result, What is not
  proven in game, the full field table: mapped / unmapped-with-reason / unverified).
- Commit messages end with `Co-Authored-By: models/gemini` and say what was not run.
- Never commit game data, extracted FrameXML/GlueXML, credentials, `benilla-config/`, or Cargo.lock
  changes you did not need.
