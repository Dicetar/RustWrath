# Project B — context

A benilla-derived Rust/Bevy World of Warcraft 3.3.5a build-12340 client for the owner's
existing AzerothCore server. First delivery is a private Windows alpha: working login,
realm and character workflow, then Firna's Northshire quest/combat/loot route. Souls-style
combat, building and generated spaces follow as modules. No complete playable-alpha claim
follows from builds, implemented APIs or the web preview.

Start with [AGENTS.md](AGENTS.md), [the workflow](docs/agents/workflow.md) and
[active ownership](docs/agents/active-ownership.md). Current continuation work is recorded
in .scratch/project-b/handoffs; dated baselines there require revalidation.
Earlier planning in D:/WOW/PROJECT_B_ASTRA_PLAN_DRAFT.md and PROJECT_B_GLM_RESPONSE_02.md
is historical input; adopted decisions and ADRs in this repository guide current work.

## Domain language

- **Fork** — this repo, branch `wotlk`, upstream donor `samwhosung/benilla` (1.12.1).
  This project's compatibility target is 3.3.5a; donor behavior stays behind its version boundary.
- **Baseline mode** — standard 3.3.5 protocol behavior; the regression invariant (no extension traffic, no capability negotiation).
- **WM-enhanced / Experimental modes** — owned carriers/addons; souls/building/custom geometry after capability negotiation.
- **Reducer** — the time-free wire→typed-state decode layer (H2 replay depends on its purity).
- **Oracle** — the pin-verified known-original client executable used for targeted RE questions (narrow, budgeted, artifact-preserved).
- **12340 decomp reference** — preserved full-history Git checkout at `D:/Projects/wowdecomp-reference`, pin `e67b6ffef23655dc7da061cb2d36b7963f6ef71a`; reconstructed source used to investigate Wrath mechanisms before implementing. Distinct from the original-binary Oracle. Workflow: `docs/agents/12340-reference.md`.
- **Profiles** — asset patch chains: `base` (stock archives) vs `wm-hd` (owner's lettered patches); loader is manifest-defined, provenance-tagged.
- **WM** — the owner's Python orchestration service (D:/WOW/wm-project); one brain, two transports (existing DB/SOAP bus; future native client channel).
- **Spellwright v3** — deployed composable-spell system (wm_spellforge_* tables, WM_FORGE3 addon channel); the first live-change proof candidate.
- **Live change** — behavior relation change on a running server, no rebuild, no restart (owner requirement).
- **Seam maps** — one-page owner/hook/gap tables for movement, building, generated spaces.
- **Native client** — the Cargo workspace under crates/, launched as benilla.exe.
- **Web preview** — the separate React/Vite applet at the root; its build is not native-client evidence.
- **Coordinator** — the ticket's accountable agent, managing delegation and the shared Cargo/client slots.

## Standing rules

- Canonical project repository: https://github.com/Dicetar/RustWrath (`origin`). Keep
  `samwhosung/benilla` as `upstream` for donor research. Work in the existing checkout;
  see `docs/agents/repository.md` for branch and collaboration rules.
- Owner decisions and their adopted defaults: `docs/baseline/owner-decisions.md`.
- ADRs live in docs/architecture; documentation responsibilities are mapped in docs/agents/domain.md.
- Server trees under D:/WOW are read-only unless a ticket says otherwise; WM source owner is wm-project.
- Estimates only with assumptions; no schedules derived from commit counts.
- Distinguish observed, reported and proposed behavior, and identify the verification level:
  source, compilation, focused case, receiver/runtime or visible gameplay.
