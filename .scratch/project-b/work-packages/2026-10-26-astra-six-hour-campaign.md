# Astra six-hour implementation campaign

## Owner priority correction — authoritative current scope

The owner's later instruction overrides the procgen-oriented wording below: faithful replacement of the CURRENT AzerothCore server, client and installed mods is the priority; toggleable Elden Ring-style combat and suitable UI are the next product capability. Procgen is a side project and its work is stopped. The six-hour campaign remains active with its original start/checkpoint times. Do not resume procgen from the older persistent-goal wording or treat the completed item-content commit as the campaign outcome.

OC's replacement prompt assigns protocol/session/reducer, gameplay authority, world/assets and server/mod boundary to OC. Astra takes UI-host/API and installed-addon compatibility plus combat-mode presentation. Active implementation claim: `crates/benilla-ui/**`, `crates/benilla-lua/**`, `crates/benilla-mlua/**` and existing app UI lifecycle/loader owners under `crates/benilla-app/src/ui_script/**`, including its actual `lifecycle.rs`. Claim exact files in checkpoints before each edit. Do not edit OC's wrath.rs/session/bridge or assets claims concurrently. No new dependency/lock change is currently planned.

Source findings directing the first main-project subsystem: the scripting engine enforces donor Lua 5.0 grammar/libraries; addon gates and build APIs are fixed to donor 11200/5875; source selection still needs tracing against the active Wrath profile. Implement real version-aware engine/host behavior and installed-mod API paths, not merely a version-string change. Preserve donor-mode semantics and existing owners. No build/test/new tests/runtime authorization is inferred.

The remaining procgen description is the superseded initial assignment, retained only to explain the already committed 8998d3d1. It does not determine current priority or completion criteria.

Direct owner instruction: a six-hour coding goal, continuing through substantial implementation rather than closing at a short milestone. Started 2026-10-26 09:00:47 Europe/Moscow (06:00:47 UTC). Six-hour checkpoint: 15:00:47 Europe/Moscow (12:00:47 UTC). Record actual work and remaining scope; no artificial waiting, background-work claims or invented execution evidence. Keep the persistent goal active across turns/compactions and intermediate commits.

Source admission: 08601ddd plus OC-owned in-flight `benilla-app/src/wrath.rs`. The old untracked patch helper remains preserved. OC owns the native WM observation/action/runtime work; this campaign must not absorb its changes or compete for that ownership.

## Active claim and system outcome

### Six-hour checkpoint assessment under corrected priority

The wall-clock checkpoint has passed. This session spans inter-turn gaps; no claim of six uninterrupted active coding hours is made. Source delivery is substantial but the corrected product outcome is incomplete, so the goal is not achieved.

Committed Astra source milestones include explicit Lua dialect and library/collector work; target/interface-aware addon admission and production UI construction; addon chunk arguments and private namespaces; handler argument/global compatibility; named XML handlers and post-hooks; shared sizing and tooltip integration; synchronous spell/known-spell/resource queries through existing gameplay owners; frame attributes/click callbacks; and targeted addon whisper transport. Latest source checkpoints are `aa5f5059` (handler compatibility) and `dd316dd3` (XML frame attributes). Detailed scope and limitations remain in the existing Astra UI-host checkpoint; E08 in the first-loop inventory now reflects partial source integration rather than its superseded fixed-version contradiction.

Required remaining work includes complete Lua/client-host semantics (notably coroutines, unsupported incremental collector controls and secure UI execution), native cast/channel/aura UI consumption after OC's contract lands, complete installed-mod API/event coverage, atomic UI/render profile reload, faithful gameplay/world/WM integration and actual server-authorized combat mode with UI. The current authoring/procgen system is deferred, not counted as the main-project outcome. OC's dirty native gameplay work remains owned by OC and is not accepted as delivered here.

2026-10-27 continuation: coroutines, native cast/channel queries/events, full-slot aura queries/tooltips/change events and native power notifications now have committed source paths through existing owners. Exact commits and remaining limitations are recorded in the UI-host checkpoint and first-loop inventory. They have no new compilation/runtime acceptance. A single bounded check/existing-test pass has been requested explicitly; the previous grant was consumed, so no checks run without a new answer. The inherited benilla client supplies the substantial renderer, assets, gameplay, networking and UI baseline; the campaign closes verified Wrath/mod/WM differences rather than treating those whole vanilla systems as unimplemented. Upstream README coverage is baseline context, not proof that this fork's adaptations work.

Evidence is source/Git inspection only. No builds, tests/new cases, linters, interpreter/check scripts, addon/client/server/WM/socket/graphical runs or deployment occurred. Compilation, connected gameplay, WM consumption and rendered acceptance remain UNKNOWN. Full stock-client replacement remains the release gate; ADR-0001 remains Proposed. There is no defensible whole-project completion percentage from the local first-loop subset.

Astra claims `crates/benilla-procgen/**`, `docs/procgen/**`, this package, Astra checkpoints and `.scratch/project-b/compatibility/**`. No current new dependency/lockfile change is planned. WM, server, bridge and client source are inspection-only unless ownership is explicitly transferred. Keep exact-path commits immediate; no long-lived staged index entries.

Complete procedural content authoring around the existing artifact/encounter pipeline: generated item mapping into existing WM managed-item drafts, existing-owner reservation handoff, a retained offline authoring package connecting candidate/content/encounters, explicit refusal and lifecycle/reconciliation handling, recovery/provenance, practical diagnostics and source-grounded replacement coverage. Inspect real owner contracts first and preserve their authority. Supported conversion must retain original stat bits and explicit conversion policy; unsupported meanings must refuse, not silently turn into a different stat.

Implement cohesive subsystems and continue through the dependencies. Intermediate commits and documentation are progress checkpoints, not completion of this campaign. At the six-hour checkpoint evaluate the entire outcome against current source and report delivered code and exact remaining requirements. If implementation is incomplete, retain the full goal rather than redefining it around the delivered subset.

## Execution and release boundaries

Source inspection, implementation and commits only. No builds, tests/new test cases/files, linters/formatters/typechecks, Python imports, generator/CLI execution, game/graphical/browser/socket/receiver/WM/server processes, installs, SQL/DB/deploy operations, external messages or subagents without an applicable explicit human grant. Earlier reports and other sessions' grants do not transfer.

WM retains allocation, publish/preflight, native execution, content retirement and runtime mutation. No second runner, registry or freeform mutation lane. The client install remains read-only. Existing baseline behavior and the full stock-client replacement release gate remain mandatory. ADR-0001 remains Proposed. Source, compilation, roundtrip, owner consumption, authoritative behavior and rendered acceptance remain separate evidence levels.
