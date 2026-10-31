# Project B — context

A benilla-derived (Rust + Bevy) World of Warcraft 3.3.5a client fork, speaking to the owner's existing AzerothCore server, built as an extensible platform: souls-style combat, building, and generated spaces arrive later as modules. Planning record: `D:/WOW/PROJECT_B_ASTRA_PLAN_DRAFT.md` (rev 3), `D:/WOW/PROJECT_B_GLM_RESPONSE_02.md` (five contract resolutions), conversation dump in `D:/WOW/`.

## Domain language

- **Fork** — this repo, branch `wotlk`, upstream donor `samwhosung/benilla` (1.12.1). The 3.3.5 retarget is ours; upstream will never do it.
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

## Standing rules

- Canonical project repository: https://github.com/Dicetar/RustWrath (`origin`). Keep
  `samwhosung/benilla` as `upstream` for donor research. Work in the existing checkout;
  see `docs/agents/repository.md` for branch and collaboration rules.
- Owner decisions and their adopted defaults: `docs/baseline/owner-decisions.md`.
- Server trees under D:/WOW are read-only unless a ticket says otherwise; WM source owner is wm-project.
- Estimates only with assumptions; no schedules derived from commit counts.
- Every claim carries confidence: source-observed / reported / proposed.
