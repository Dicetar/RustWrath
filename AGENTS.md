# benilla

## Project B / wotlk reference override

This checkout is the Project B 3.3.5a fork. For Wrath build 12340 behavior, consult the preserved
decompilation at `D:/Projects/wowdecomp-reference` before implementing or guessing client
mechanisms. Pin: `e67b6ffef23655dc7da061cb2d36b7963f6ef71a` (Terrorblade/wowdecomp).
Read [the reference workflow](docs/agents/12340-reference.md) for source routing and validation.
The 1.12 instructions below still govern donor compatibility; they do not define Wrath layouts.

A from-scratch World of Warcraft 1.12.1 client in Rust and Bevy. The reference client is the
spec: benilla is a faithful, modern implementation of it, not a place to get creative.

Read, in this order:

1. `docs/METHOD.md` — the rules and how we work. Binding for every session.
2. `docs/MAP.md` — what is built right now, generated from the code. Orient here before
   touching anything.
3. `docs/CONTRIBUTING.md` — what gets in and how a change is judged.

What the real client does comes from [benilla-re](https://github.com/samwhosung/benilla-re), the
reverse engineering of 1.12.1, before you build: clone it beside this repo and grep it, or look an
address up at `https://samwhosung.github.io/benilla-re/?0x5e6853`. `docs/CONTRIBUTING.md`, "The
reference", says how to read it and what to do when it has a gap.

The rest of the root is code and data: `crates/` the workspace, `reference/` benilla-re's
tables vendored at a pinned commit, which the tests check against, `scripts/` the gates and
instruments, `third_party/` vendored code.

@docs/METHOD.md

## Agent skills

### Issue tracker

Project B tickets live as local markdown under `.scratch/project-b/issues/`. See `docs/agents/issue-tracker.md`.

### Triage labels

Default five-label vocabulary in use. See `docs/agents/triage-labels.md`.

### Domain docs

Single-context: `CONTEXT.md` + `docs/adr/` at repo root. See `docs/agents/domain.md`.

### 12340 reference skill

Use `project-b-12340-reference` for this fork's rendering, formats, protocol-facing client
behavior and Lua/UI contracts. Skill: `C:/Users/Qraag/.codex/skills/project-b-12340-reference/SKILL.md`.
It points to the canonical repository guide; availability of source is not permission to copy it
or evidence of runtime correctness.
