# Autoswarm overseer handoff — RustWrath modular hotpatch — 2026-11-11

This is the next-session runbook for the modular/hotpatch line. It assumes the overseer will
coordinate two AutoSwarm agents per batch, run two independent ZAEBAL audits, commit, merge and
push without asking for routine choices. Read the repository `AGENTS.md`, `CONTEXT.md`,
`docs/agents/workflow.md`, `docs/agents/active-ownership.md` and
`docs/architecture/adr-0002-live-client-modules.md` first.

## 0. TL;DR

- Release verification was recorded at **`e12dc650`**, the follow-up on top of the movement-health
  merge `6d5c8425`. `origin/main` has since advanced to **`329a5151`**
  (`fix(app,ui): resolve benilla-app compile blockers and tick secure state drivers`), which claims
  to repair the app/UI blockers below; it has not been re-checked by this line.
- The last 27 modular issues are closed at **source/API level**. Release verification was attempted
  on 2026-11-11: the hotpatch crates compile and 15 focused hotpatch tests pass, but
  `benilla-app --features hotpatch` was blocked by 21 pre-existing `benilla-app`/protocol/UI errors
  outside the hotpatch files. No client/server/visual run was made.
- The next gate is still **release verification, not another source slice**: rerun the app check and
  app mapping tests on `329a5151`, then the owner-driven login → roster → Firna/Northshire run.
  The production movement controller adapter/handback also remains open.
- Remaining source-only polish if requested: movement/rollback typed reasons, a persisted last-good
  module identity, and the later/active native marker windows.
- Model fallback for AutoSwarm workers: **Flint → Beam → DeepSeek 4.1 Flash**. Flint and Beam
  have repeatedly hit free-usage limits; switch immediately rather than retrying.
- Every batch: **two independent ZAEBAL auditors**, source/API outcome gate, then commit, merge,
  push, update the modular handoff. Source review is not release evidence.
- Audit lessons: `anyhow`'s `{:#}` appends the `source()` chain; a typed event must not attribute
  a predecessor's reason to the active module; a persisted stamp needs its latch text; a tolerated
  refusal must not leave a stale typed channel; and always regenerate the raw diff after
  coordinator edits.

## 1. Hard constraints

- Source-only unless the owner explicitly authorizes otherwise: no `cargo build/check/test/clippy`,
  no `rustfmt`, no client/server/DB/visual run, no unattended combat.
- Never edit, reset, clean or run Cargo in the primary checkout
  `D:\Projects\project-b-client`; it contains other owners' dirty inspect-honor work.
- Preserve dirty source and untracked evidence. Stage explicit owned paths only; no blanket
  `git add .`, no `git reset --hard`, no `git clean`, no branch switch/pull/rebase underneath
  dirty work.
- One coordinator owns Cargo and client slots. This line is holding none. If a build is ever
  authorized, use sccache (the checkout config sets `build.rustc-wrapper = "sccache"`), check
  `sccache --show-stats` before claiming a cache hit, and never start a competing client.
- Keep the targeted verification budget within the current 20-case task limit. Do not treat a
  source review as compilation, receiver or visible-gameplay evidence.
- Never commit game data, extracted FrameXML/GlueXML, credentials or local runtime state.
- Do not stop after one batch while a bounded source-only issue remains; the prior behavioral
  ZAEBAL audit identified premature stopping as the failure mode.

## 2. Current repository state

- Main: `e12dc650` = release-verification follow-up (hotpatch test assertion, mechanical build
  repairs, active-ownership record) on top of the movement-health merge `6d5c8425`.
- Integration worktree:
  `C:\Users\Qraag\.local\share\opencode\worktree\399553\rw-main-merge`
  branch `merge/wrath-diagnostic-stack`, clean, tracks `origin/main`.
- Main history is not linear: other owners push regularly (WMBridge/LFG/talent, pet talents,
  whispers, inventory, addons/secure templates). Always `git fetch origin main` before
  editing/merging and never force-push.
- Modular handoff with closed-item detail:
  `D:\Projects\rustwrath-main-integration\.scratch\project-b\handoffs\2026-11-09-modular-hotpatch-closure.md`
  (items 1–27 plus the release-verification section).
- The actual `rustwrath-main-integration` checkout may contain ignored scratch/build logs. Do not
  delete them; they are evidence.

Recent modular commits, newest first:

```
329a5151 fix(app,ui): resolve benilla-app compile blockers and tick secure state drivers
e12dc650 docs(agents): record release verification blocker
7145198b fix(build): repair pre-existing UI and app compile errors
7d147f15 fix(hotpatch): assert the movement state revision table
6d5c8425 Merge origin/main into the modular hotpatch line
61980f8d feat(hotpatch): add movement health and active retention policy
7e91b794 feat(wrath): evaluate 3.3.5a macro conditions in SecureCmdOptionParse and RegisterStateDriver
95740455 Merge origin/main into the modular hotpatch line
978779da feat(hotpatch): persist health, policy and artifact stamps
ffdd690f feat(hotpatch): type canary and active evaluation reasons
370383c6 feat(hotpatch): split publication rejections into fine reasons
```

## 3. What is already closed at source/API

Do not redo these. Details and residuals are in the modular handoff.

- Modular seam capability gate, terminal pose/movement latching, movement retention, pose
  rig-span tiling, movement commit gate, pose health snapshot and configurable policy.
- Persistent digest quarantine (`quarantine.toml`).
- Pose crash sentinel: loading window around `Loaded::open`, staged-canary window around
  `Loaded::evaluate`, startup recovery by kind, pending-recovery retention.
- Movement crash sentinel: loading window around `MovementLoaded::open`, one-shot first staged
  proposal around `process`, movement-quarantine routing.
- Clean shutdown: `Runtime::drop` and explicit `AppExit` `Last` edge; pending/unowned markers
  retained; panic unwind skips the clear.
- Content-bound sentinel ownership: `ActiveSentinelFile` identity, shared compare-and-clear for
  clean shutdown and the recovery-commit retry, cap/foreign/malformed retention.
- `hotpatch-probe` parity with the plugin's startup recovery and 1 Hz persistence.
- Typed pose publication reasons: `TransientIoPending`, `ManifestUnreadable`,
  `ManifestMalformed`, `InvalidBasename`, `EmptyRevision`, `ArtifactTooLarge`, `DigestMismatch`,
  `ImmutableFilename`, `Quarantined`, `ModuleLoadFailed`, `MissingEntryPoint`, `AbiMismatch`,
  `MissingProcessor`, plus the untyped `ArtifactRejected`/`LoadFailed` fallbacks, carried into
  `quest_trace::HotpatchReason`. The private `PatchRejection` keeps the previous `{}`/`{:#}`
  detail text and exposes the wrapped load error only through `source()`.
- Typed pose canary/active evaluation reasons: `PatchEvaluationReason`
  (`Status(i32)`, `InvalidOutput`, `TopologyChanged`, `FrameTooLarge`, `HostFrameRejected`,
  `Unknown`), private `EvaluationFailure` with unchanged detail text, paired string/typed host
  slots and pair-taking accessors, `RestoreOutcome::Unavailable.reason`, and `detail` on
  `CandidateRejected`/`FrameFailed`/`RestoreUnavailable`, carried into `quest_trace` including the
  restore-unavailable summary. A restore `FrameFailed` keeps the active module's reason; the
  predecessor/outcome reason is carried only by `RestoreUnavailable`.
- Persistent patch state: `<root>/patch-state.toml` (`schema_version = 1`) stores the demotion
  policy, up to 16 digest-keyed per-revision cumulative health records, and the pose/movement
  artifact stamps with their bounded latched manifest text. `load_persistent_state` validates and
  replaces policy/health/stamps/latches; `persist_state` writes only while dirty via
  temp+sync+rename. Promotion/restore seed cumulative total/max; the plugin and probe load at
  startup and save at 1 Hz through separate deduped slots.
- Movement health: `MovementHealthSnapshot`, `MovementHealthPolicy` (default `1/3`), and
  `MovementRefusal::Tolerated`; active-only timing/counting, below-threshold retained active,
  threshold quarantine/clear, staged-canary isolation, commit seeding/unload reset, and an
  independent bounded `[movement_health]` section in `patch-state.toml`. The movement policy is a
  live override and is not serialized; the production adapter/handback is still absent.

All of these are **source/API closures**, not build/test/runtime evidence. ZAEBAL outcome gates
used two independent auditors and explicitly named residuals.

## 4. Remaining H03 gaps, recommended order

1. **Finer publication rejection codes** — closed at `370383c6` (details in the modular
   handoff).
2. **Canary/active typed reasons** — closed at `ffdd690f` (details in the modular handoff).
3. **Persistent health/policy/artifact stamps** — closed at `978779da`/`95740455` (details in the
   modular handoff).
4. **Movement health** — closed at `61980f8d`/`6d5c8425` (details in the modular handoff).
5. **Later/active native marker windows** — later pre-commit staged movement proposals, active
   movement/pose per-frame calls, and `Loaded` drop/`FreeLibrary`/DLL-detach. Per-frame file
   writes are unacceptable; this needs a performance/behavior decision.
6. **Production movement controller adapter** — cross-crate design: the app controller is
   `pub(crate)` and single-slot, the hotpatch `Runtime`/`Host` are private, and the retention
   contract needs a bridge plus build/launch validation. This is the main missing integration item.
7. **Build/test/receiver/visible evidence** — not authorized in this line so far.
8. **Sentinel residuals** — read/compare/unlink TOCTOU, a same-parsed-identity replacement,
   path-based call-scoped clears, cross-process root ownership, a second clear failure, AppExit
   dev-only `Last` writers, and `process::exit`/hard kills.

## 5. Next gate — release verification, not another source slice

The source/API hotpatch line is closed through item 27. The owner authorized **build/check and
focused tests only** on 2026-11-11; no client/server/visual run. Results:

- `cargo check -p benilla-hotpatch -p benilla-hotpatch-bevy` passed. The rustc command line showed
  the `sccache.exe rustc ...` wrapper; `sccache --show-stats` still reported 0 compile requests, so
  no cache-hit claim is made.
- 15 focused `benilla-hotpatch` tests passed after correcting one test assertion
  (`7d147f15`): the movement state round-trip expected `[movement_health]`, while TOML serializes
  the array-of-tables as `[[movement_health.revisions]]`.
- `cargo check -p benilla-app --features hotpatch` remains blocked by 21 pre-existing errors outside
  the hotpatch files: `ui_quest::is_shown`, the `wrath.rs` PvP surface (`BattlefieldStatus` fields,
  `PvpLogRow::bonus_honor`, `SessionEvent::PvpLogData` `winner`/`rows`), and boxed
  `LfgJoinResult`/`LfgProposalUpdate` mismatches. The mechanical repairs in `7145198b`
  (`secure.rs` `ObjectLike`, `pvp` `MultiValue`, GM raw string, `EntityKind` path) reduced the list
  but did not make the app compile-clean.
- No client, receiver or visible-gameplay evidence was produced.

`329a5151` landed after this verification and claims to resolve the app/UI compile blockers and
tick secure state drivers. It has not been re-checked by this line: rerun
`cargo check -p benilla-app --features hotpatch` and the app mapping tests before treating the
first gate item below as passed.

The remaining gate:

1. **Compile-clean app.** Rerun the app check and app mapping tests on `329a5151` (the owners'
   claimed repair). If errors remain, hand them back to the owning `ui_quest`/`wrath.rs` PvP/LFG
   surface rather than repairing another owner's code without assignment.
2. **Focused app tests.** Run the hotpatch/app mapping tests within the 20-case limit. A green test
   run does not establish receiver or visible behavior.
3. **Receiver/runtime.** Owner-driven login, realm/roster, enter world, `/reload`, and the
   hotpatch observation path. Verify the state file/quarantine/sentinel behavior in the real
   client; do not automate combat or create/delete persistent characters.
4. **Visible route.** Firna’s Northshire quest/combat/loot route, owner-driven, with the client
   PID/session and observed behavior recorded. Source closures cannot substitute for this.
5. **Production movement adapter.** Design the cross-crate bridge: the app controller is
   single-slot and `pub(crate)`, the hotpatch `Runtime`/`Host` are private, and the retained-active
   / tolerated-refusal handback needs a controller contract. Then build and launch-validate it.
6. **Release packaging/launch.** Only after the above: a private Windows alpha artifact, launch
   instructions, and the owner’s acceptance. No release/tag claim from source review alone.

Remaining source-only polish, if the owner asks for more code before verification: movement/
rollback typed reasons, a persisted last-good module identity, the later/active native marker
windows, and the sentinel TOCTOU/cross-process residuals. These are not substitutes for the gate
above.

### Acceptance for the release gate

- Build/check and the focused tests actually pass under sccache, with logs and revision recorded.
- Owner-driven receiver/runtime and the Northshire route are observed, not inferred.
- The production movement adapter is present and validated, or explicitly descoped by the owner.
- Every remaining residual is named; no build/test/receiver/visible claim is made from source
  review.
## 6. AutoSwarm workflow

### 6.1 Create fresh worktrees

Use the Code Mode `tools.opencode.worktree_create({name})` tool from an `execute` block. Worktrees
land under:

`C:\Users\Qraag\.local\share\opencode\worktree\399553\<name>`

Create an implementer worktree and a reviewer worktree, then branch both from the current
`origin/main`:

```powershell
git -C <impl-dir> switch -c fix/<slice>-a <main-sha>
git -C <review-dir> switch -c review/<slice>-b <main-sha>
```

Use fresh names; old worktrees from prior batches are stale. If a name already exists, the tool
appends `-2`; that is fine. Do not reuse an agent name already present in the controller:
AutoSwarm rejects duplicate conversation names.

### 6.2 Launch the two agents

Home: `C:\Users\Qraag\.autoswarm`

Python wrapper:
`C:\Users\Qraag\AppData\Roaming\uv\tools\autoswarm\Scripts\python.exe`
Launch script:
`C:\Users\Qraag\.agents\skills\autoswarm\scripts\launch.py`

Implementer:

```powershell
& 'C:\Users\Qraag\AppData\Roaming\uv\tools\autoswarm\Scripts\python.exe' `
  'C:\Users\Qraag\.agents\skills\autoswarm\scripts\launch.py' `
  --home 'C:\Users\Qraag\.autoswarm' run --agent opencode `
  --cwd '<impl-dir>' --name '<slice>-implementer' `
  --model 'opencode/flint-40b-free' `
  --allow-peer '<slice>-reviewer' `
  --communication-rules 'Concrete evidence only. Share the typed-error design, exact reason mapping and tests. Do not ask the parent to relay routine questions. Do not expand scope without supervisor approval.' `
  --input 'F:\AI\tmp\opencode\<slice>-worker-a.md' --background
```

Reviewer:

```powershell
& 'C:\Users\Qraag\AppData\Roaming\uv\tools\autoswarm\Scripts\python.exe' `
  'C:\Users\Qraag\.agents\skills\autoswarm\scripts\launch.py' `
  --home 'C:\Users\Qraag\.autoswarm' run --agent opencode `
  --cwd '<review-dir>' --name '<slice>-reviewer' `
  --model 'opencode/flint-40b-free' `
  --allow-peer '<slice>-implementer' `
  --communication-rules 'Concrete evidence only. Challenge reason classification, transient preservation, movement compatibility and tests. Do not edit, build or test. Send findings to the implementer and parent. Do not expand scope without supervisor approval.' `
  --input 'F:\AI\tmp\opencode\<slice>-worker-b.md' --background
```

### 6.3 Model fallback — Flint → Beam → DeepSeek

Observed exact IDs:

1. `opencode/flint-40b-free` — primary.
2. `opencode/beam-free` — first fallback.
3. `opencode-go/deepseek-v4.1-flash` — final fallback and the most reliable in recent batches.

What happens in practice: Flint and Beam often accept the launch and then fail in a few seconds
with `provider.free-limit` / “Free usage limit reached”. Inspect the call; if it is in `returned`
with that error, immediately relaunch the same prompt under the next model with a new unique agent
name (for example `-beam`/`-ds` suffix). Do not keep retrying an exhausted model. DeepSeek has
completed every fallback batch.

The reviewer can be launched after the implementer's first diff exists, instead of concurrently,
to avoid the reviewer finishing before there is code to review. If launched concurrently, send it
the final diff path and ask for a final-diff pass once the implementer completes.

### 6.4 Monitor and collect

```powershell
& 'C:\Users\Qraag\AppData\Roaming\uv\tools\autoswarm\Scripts\python.exe' `
  'C:\Users\Qraag\.agents\skills\autoswarm\scripts\launch.py' `
  --home 'C:\Users\Qraag\.autoswarm' inspect <call_id>

& 'C:\Users\Qraag\AppData\Roaming\uv\tools\autoswarm\Scripts\python.exe' `
  'C:\Users\Qraag\.agents\skills\autoswarm\scripts\launch.py' `
  --home 'C:\Users\Qraag\.autoswarm' wait --any <call_id_a> <call_id_b>
```

Do not poll in a tight loop; a 3–5 minute wait between checks is reasonable. Check the worktree
`git status`, `git diff --stat` and the actual diff. Workers sometimes leave the old test asserting
the pre-change behavior or omit tests; fix that before ZAEBAL.

### 6.5 Coordinator responsibilities

- Read the worker diff yourself before auditing.
- Regenerate the raw diff after every coordinator edit; auditors can otherwise see a stale
  snapshot.
- Verify `git diff --check`.
- Keep a source-only scope: no build/test commands in prompts or coordinator actions.
- If a worker reports “fixed a pre-existing brace” or another surprising edit, inspect the diff;
  do not trust the summary.

## 7. ZAEBAL protocol

Run **two independent audits in parallel** after the implementation and coordinator fixes. Use
the `subagent` tool with `agent: "general"`, read-only, and give each auditor the raw context:
original request, issue statement, base SHA, worktree path, branch, raw diff path, files, and the
exact assumptions. Do not let them edit, build, test, clippy, fmt or run the client.

Auditor 1 focuses on contract/interpretation/closure. Auditor 2 focuses on causal correctness,
tests, compile hazards and docs. Each must return the L1 template:

1. `CONTRACT` — quote the original user request and the ADR/issue requirement; list assumptions.
2. `DIVERGENCE POINT` — first earlier divergence, or “not established”.
3. `FACTS` — only named file/line evidence.
4. `HYPOTHESES` — at least two competing interpretations.
5. `DISCRIMINATING CHECK` — one read-only check and what would disprove the chosen view.
6. `PREVIOUS AUDIT` — “none” if this is a new issue.
7. `WRONG BELIEF` — only after the check; otherwise “not established”.
8. `STATUS` — CONFIRMED / PARTIAL / UNVERIFIED / DISPROVED for the diagnosis.
9. `OUTCOME GATE` — closed / partial / unverified at **source/API**, exact remaining gaps, and
   whether the lack of build/test changes the claim.

Rules:

- Regenerate the diff immediately before launching the auditors and state the exact path.
- Tell them the worktree is authoritative if the diff could be stale.
- If an auditor finds a real defect, fix it, regenerate the diff, and send both auditors a short
  final-gate follow-up asking them to confirm only the fixes and the scoped closure.
- A source/API “closed” is acceptable for this line only when the scoped issue is narrow and
  every residual is named. Do not claim build, test, receiver or visible acceptance.
- Recent audits caught: clear-before-persist ordering, `poll` clearing a retained recovery marker,
  an unsound panic-through-`extern "C"` test, event-detail dedupe swallowing a reason change, a
  rustfmt-width violation, attempt-summary limitations, an `anyhow` `{:#}` display/source-chain
  duplication, a stale raw diff, an active-vs-predecessor `FrameFailed` misattribution, a latent
  helper fallback that could re-open it, a persisted stamp with no persisted latch text
  (write-only across restart), a stale tolerated-refusal channel, a disabled-stage reset bypass,
  and a policy that dirtied unpersisted state. Expect similar concrete findings.

## 8. Commit, merge, push

1. Stage only the owned files, e.g.
   `crates/benilla-hotpatch/src/lib.rs`, `crates/benilla-hotpatch-bevy/src/lib.rs`,
   `crates/benilla-app/src/hotpatch.rs`, `crates/benilla-app/src/quest_trace.rs`,
   `docs/architecture/adr-0002-live-client-modules.md`.
2. Run `git diff --check` and `git diff --cached --check`.
3. Commit with a message file to avoid PowerShell quoting problems:
   `git commit -F F:\AI\tmp\opencode\<slice>-commit.txt`.
4. Push the feature branch.
5. In `rw-main-merge`:
   ```powershell
   git fetch origin main
   git log --oneline -3 origin/main
   git merge --ff-only <commit>          # if origin/main has not moved
   # otherwise:
   git merge --no-ff origin/main -m "Merge origin/main into the modular hotpatch line"
   git push origin HEAD:main
   git ls-remote origin refs/heads/main
   ```
   Never force-push. Other owners may push during a batch; integrate and re-run the merge.
6. Update both handoff files:
   - this runbook if the process changed,
   - `2026-11-09-modular-hotpatch-closure.md` with a new numbered closed item, current main SHA,
     residuals and the next slice.

## 9. Bootstrap checklist for the next session

1. Read `AGENTS.md`, `CONTEXT.md`, `docs/agents/workflow.md`,
   `docs/agents/active-ownership.md`, ADR-0002, this runbook and the modular handoff.
2. Inspect `git status`, HEAD, index, dirty diffs and live processes. Do not touch
   `D:\Projects\project-b-client`.
3. `git fetch origin main`; record the current SHA. Revalidate all references before editing.
4. Create fresh implementer/reviewer worktrees from that SHA.
5. Launch the implementer with Flint; on a free-limit failure, relaunch with Beam, then DeepSeek.
6. When the first diff exists, launch the reviewer (or send it the final diff) with the same
   fallback chain.
7. Read/fix the diff, regenerate it, run two independent ZAEBAL audits, fix findings, final gates.
8. Commit, push, merge to `main`, verify `git ls-remote`, update handoffs.
9. Continue to the next bounded issue; do not stop merely because one batch closed.

## 10. Reference map

- `crates/benilla-hotpatch/src/lib.rs` — `Host`, `PatchRejectReason`, `PatchRejection`,
  `PatchEvaluationReason`, `EvaluationFailure`, `MovementHealthSnapshot`,
  `MovementHealthPolicy`, `MovementRefusal::Tolerated`, `verify_publication`, `poll`, `evaluate`,
  `Loaded`, `patch-state.toml` (`PatchStateFile`, pose/movement health, policy/stamps), movement
  seam, health, quarantine, sentinels.
- `crates/benilla-hotpatch-bevy/src/lib.rs` — `HotpatchPlugin`, `Runtime`, `HotpatchEvent`,
  `HotpatchReason`, poll/ApplyPose/Drop/AppExit wiring, event detail dedupe.
- `crates/benilla-app/src/hotpatch.rs` — Bevy event -> `quest_trace::PatchEvent` mapping.
- `crates/benilla-app/src/quest_trace.rs` — `HotpatchReason`, `PatchEvent`, incident output.
- `crates/benilla-hotpatch/src/bin/hotpatch-probe.rs` — dev probe with plugin-parity startup/persist.
- `docs/architecture/adr-0002-live-client-modules.md` — H03 contract, current behavior, residuals.
- `docs/architecture/release-plan-2026-11-05.md` — movement lifecycle/adapter scope.
- `.scratch/project-b/handoffs/2026-11-09-modular-hotpatch-closure.md` - closed items 1-27.
- `F:\AI\tmp\opencode\` — prompt/diff/commit-message scratch area (pre-approved).

## 11. Definition of done for the next slice

- Build/check and the named focused tests actually pass under sccache, with source revision,
  command, log and cache evidence recorded.
- The owner-driven receiver/runtime login → roster → world entry and Firna's Northshire route are
  observed; source/API closures are not substituted for them.
- The production movement adapter/handback is present and launch-validated, or explicitly
  descoped by the owner.
- Every remaining residual is named and no build/test/receiver/visible claim is made from source
  review alone.
