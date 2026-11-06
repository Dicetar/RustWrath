# Working on RustWrath

Direct owner requests and host system/developer rules take precedence. This guide defines
the Project B working loop; donor release procedures do not authorize command execution
or redefine the Wrath target.

## Admission and coordination

Inspect Git status, HEAD, index and relevant diffs. Read the owning ticket/handoff and
[active ownership](active-ownership.md). Check actual Cargo/rustc/client processes;
reconcile stale reservations with their owner or recorded completion.

Assign one accountable owner per ticket, including cross-crate fixes. Divide work by
concrete deliverable and files, not permanent agent/crate boundaries. Sequence shared-file
edits explicitly. Preserve other owners' work, including mechanical compilation repairs.
Neither a quiet process nor a clean tree proves a task complete.

When collaboration is authorized, the coordinator handles agent communication. Ask the
owner only for decisions or access the agents cannot resolve within their assignment.
Do not make the owner relay routine ownership questions.

## AutoSwarm on this workstation

Read the installed autoswarm SKILL.md and use its exact launcher. Current skill paths:

- Codex: `C:/Users/Qraag/.agents/skills/autoswarm/SKILL.md`.
- OpenCode: `C:/Users/Qraag/.config/opencode/skills/autoswarm/SKILL.md`.
- Shared home: `C:/Users/Qraag/.autoswarm`, outside redirected Codex AppData.

Known setup, recorded 2026-11-02; revalidate before relying on it:

| Component | Configuration / evidence |
|---|---|
| Source | D:/Projects/AutoSwarm, with local OpenCode v2 compatibility changes |
| Codex | Initial reply and native resume previously verified |
| OpenCode | v2.2.5 at D:/Projects/opencode-v2-test/node_modules/.bin/opencode2.cmd, ACP transport |
| OC default model | opencode-go/deepseek-v4.1-flash, selected by owner; reply/resume verified |
| OC plugin | ~/.config/opencode/plugins/autoswarm.js, previously verified active |
| Result collection | Explicit wait; automatic callbacks are not configured |
| Gemini | Additional setup remains unverified; inspect current runtime and configuration |

Use v2; the v1 CLI under local-coding-engine/integrations/opencode is a separate install.
Keep the owner's OC default. Select occasional alternate models only when authorized.
Antigravity GUI, Gemini CLI and an OpenCode Antigravity model are different entry points;
do not assume shared sessions, model access or quota.

1. Inspect agents, calls, retained results and native sessions before assigning work.
2. Resume/message known managed tasks where appropriate. AutoSwarm does not automatically
   discover or control manually opened OC/Gemini chats. Inspect a supported native binding
   before claiming connection; do not guess session IDs or duplicate active editing.
3. Assign bounded deliverables, disjoint files and explicit build/client restrictions.
   Retain call IDs. Collect results with wait and handle intermediate questions/approvals
   under the installed skill and the owner's authorization.
4. Review actual diffs and outputs before integrating. A child's completion report is
   evidence to examine, not receiver/runtime acceptance.

Each provider retains its own authentication and quota. AutoSwarm does not bypass Codex
limits or require Codex to lead; OpenCode can coordinate independently. Installing the skill
does not establish an unattended scheduler or restart/resume service.

Installed skills contain copied runtimes. A deliberate update must preserve local compatibility
patches, install the updated package, refresh each host in use, and verify the affected route.
Changing only one copied runtime can leave hosts inconsistent. Do not update as a side effect
of an unrelated client task.

## Builds, runs and acceptance

One coordinator owns Cargo and graphical-client slots. Record owner, task, command/log and
state in active-ownership; release the slot on completion. PIDs and hashes are run evidence,
not permanent instructions. Never terminate another job to acquire a slot.

Every Cargo invocation goes through sccache. The checkout's `.cargo/config.toml` selects
`build.rustc-wrapper = "sccache"`; CI installs it with the pinned sccache action. A missing
wrapper is a build failure, not permission to bypass it. Verify with `sccache --show-stats`
before claiming a cache hit; use `CARGO_INCREMENTAL=0` when a build should populate the cache.
`cargo clippy` may bypass the wrapper (rust-lang/rust-clippy#3840), so clippy runs are not
sccache-cached; build and test runs are.

Use only authorized verification. The existing bounded client-repair grant permits essential
targeted cases, a coherent integration build and coordinated runtime checks, with at most
20 essential targeted test cases per task. It does not require filling that budget, repeated
suites, automatic logins, combat or persistent character changes. Documentation maintenance
needs no compilation or client run. Report unauthorized/unperformed checks as unrun.

| Evidence | What it establishes |
|---|---|
| Source/reference review | A mechanism or contract exists in the inspected source |
| Compilation | The recorded command built that source configuration |
| Focused check | The exercised case passed, with stated limits |
| Runtime/receiver | The actual producer, owner and consumer exchanged the result |
| Visible gameplay | The owner-visible behavior worked in the recorded scene/route |

For visible bugs, return to the original symptom. Record source revision and dirty paths,
binary hash, profile, run/log and observed result. Old binary evidence does not validate new
source. Missing errors do not prove working APIs; stubs and unsupported branches remain
explicit. Do not claim playable alpha until the agreed route is demonstrated.

The owner enters credentials and drives combat. Firna is the dedicated Northshire character;
select her through the roster and verify identity before interacting. Respect live sessions.
The [launcher guide](../launching.md) explains the player path.

## Git, storage and handoffs

Follow [repository rules](repository.md). No pull/rebase/branch switch beneath dirty shared
work, blanket staging, automatic stash/reset, or unassigned push. Launching requires no Git
publication. A commit does not rebuild or hotpatch a running executable.

Keep logs/state in designated local directories; keep assets and the reference outside the
repository. Follow [backup policy](backups.md): three completed source backups maximum,
no target/MPQ copies. Build caches can be much larger than backups; inspect size and live
ownership before proposing cleanup. Do not bypass a denied deletion through another tool.

Standing rules belong in AGENTS and these guides, current reservations in active-ownership,
architectural decisions in [docs/architecture](../architecture/), and session outcomes in
the owning .scratch/project-b/handoffs file. Name reference anchors, checks actually run,
remaining failures and the next concrete step. Keep historical evidence separate from current fact.
