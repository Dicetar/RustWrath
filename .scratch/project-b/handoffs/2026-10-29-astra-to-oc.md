# Astra handoff to OC — in-world failure and private alpha

The user asked Astra to stop and hand this work to OC. Continue toward the fastest
private Windows 3.3.5a alpha on the existing local AzerothCore server, with the
stock UI first. The user authorized necessary builds, focused tests and bounded
runtime diagnostics. Preserve the read-only WoW installation, identity secrets,
all evidence, and other people's work; do not print `.probe-identity` contents.

## Bootstrap and ownership

Work in `D:/Projects/project-b-client`, branch `wotlk`. Revalidate HEAD, status,
index, live processes, and current ownership before changes. Read root `AGENTS.md`,
`CONTEXT.md`, `docs/METHOD.md`, and `.scratch/project-b/handoffs/2026-10-29-oc-inworld-failure.md`.
The detailed measurements and current claims are in
`.scratch/project-b/checkpoints/2026-10-26-astra-wrath-ui-host.md`.

OC's existing uncommitted changes remain untouched: the first-alpha checkpoint,
`ui_script/extract/mod.rs`, `ui_script/world_map_tests.rs`, `script/keybind.rs`,
and `script/worldmap.rs`. Preserve all untracked logs and `tools/patch_procgen_dungeon.py`.
The built client includes those uncommitted files, so a commit hash alone does
not identify its full source. No client, cargo or rustc process remained at handoff.

## Proven findings and fixes

- `68585d5b` adds Wrath `WorldMapArea`/`WorldMapContinent` DBC layouts. OC's
  dirty UI work enabled a world-sheet map to paint and toggle; current-zone
  dropdowns and full map acceptance remain open.
- `849af7d8` bounds malformed Wrath bone-animation track reads; it did not solve
  the memory explosion.
- `3b0b0e0f` stops the real memory cause: the vanilla particle decoder interpreted
  264 particle data as vanilla records and baked huge bogus tracks. It explicitly
  refuses nonempty Wrath particle data; **real 264 particle effects are disabled**.
  This is containment, not fidelity. The previous run crossed 5 GiB in <20 seconds;
  the latest 55-second run peaked at 2,550,558,720 private bytes.
- `7a3ab86a` registers seven existing fixture-only Wrath startup CVars in the
  production host before FrameXML loads. The focused selected-profile stock
  PlayerFrame smoke passes, including initialization of its combat feedback.
- The four additional status/threat settings in the subsequent commit are also
  registered via the host. Their off defaults are **provisional**, not verified
  against build 12340. Selected-profile smoke and client build passed. The latest
  live log contains zero missing target/party/pet/threat setting errors and zero
  `CombatFeedback.lua:107` errors, versus 1,693 such errors in the previous run.

## Immediate next actions

1. Fix the Wrath unit-token resolution and production state path, including
   `vehicle`, `focus`, and `boss1`–`boss4`, while preserving vanilla behavior.
   Do not make real units silently unavailable merely to suppress errors.
   The last run logged 2,915 unknown-focus and nine unknown-vehicle errors;
   TargetFrame OnLoad reached `TargetFrame.lua:652` and failed on `vehicle`.
   Establish how each token is populated and test absent, present and target-hop
   cases against the correct host state and events.
2. Implement a proper 264 M2 particle decoder, including versioned record and
   track layout, bounds, sequence/global clocks, and over-life data, then remove
   the temporary refusal. Maintain vanilla behavior and validate real profile
   assets and visual effects; do not treat the current warning as a final fix.
3. Verify player-model visibility with a GUID-to-rendered-entity trace rather
   than inferring identity from the earlier ambiguous rider screenshot. Complete
   map-zone selection, dropdowns and open/close behavior.
4. Repeat a bounded connected run and exercise actual movement, targeting,
   combat, quest, loot, turn-in, logout and persistence on the dedicated account.
   Only claim playable alpha when these are demonstrated; current runs do not
   prove a connected in-world acceptance route.

## Reproduction and evidence

Evidence directory: `.scratch/project-b/verification/2026-10-28-oc-alpha/`.
The latest build log is `astra-more-cvars-build.log`, test log is
`astra-more-cvars-test.log`, memory run is `astra-track-probe-20261029-103028-456`,
and client stderr is `connected-run-20261029-103029-076.stderr.log`.
Latest binary SHA-256 is `CC86191ABF5734F5235E2C303DCFEAE7C45304DC8539A5D8B3FBC70E1724CDD1`.

The existing `astra-memory-probe.ps1` runs the current binary against the
gitignored identity file and the existing shadow-data profile, samples process
memory, and stops at 5 GiB or after 55 seconds. It refuses to start if another
benilla process exists. Avoid unattended combat or modifying the server/account
outside the authorized acceptance route. Refer to the OC handoff for setup and
limitations. Do not use the logs or fixture as evidence of full gameplay.
