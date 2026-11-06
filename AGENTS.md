# RustWrath agent instructions

RustWrath is the Project B fork of benilla: a Rust/Bevy client targeting WoW 3.3.5a
build 12340 and the owner's existing AzerothCore server, extended with new features.
1.12 donor code is retained only where useful and fitting, not as a mandatory version
boundary. First delivery: a private Windows alpha with a working login/roster flow,
then Firna's Northshire quest/combat/loot route.

## Bootstrap

1. Read [CONTEXT.md](CONTEXT.md) for scope and domain language.
2. Read [the workflow](docs/agents/workflow.md) and [active ownership](docs/agents/active-ownership.md).
3. Read the relevant ticket/handoff; inspect HEAD, index, dirty diffs and live processes.
4. Consult [METHOD](docs/METHOD.md), [CONTRIBUTING](docs/CONTRIBUTING.md), and relevant
   parts of [MAP](docs/MAP.md). MAP is generated orientation, not runtime acceptance.
5. For Wrath repairs, follow [the 12340 reference workflow](docs/agents/12340-reference.md).
6. For an explicitly requested local launch, read [the server and client runbook](docs/agents/local-launch.md).

System/developer instructions and direct owner requests take precedence. The Project B
workflow qualifies inherited donor verification/release procedures; those procedures
do not authorize broad test runs, builds or live sessions.

## Ownership and execution

- Use one owner per ticket across whichever crates the fix needs. Permanent Astra/OC
  crate lockouts are retired. Agree disjoint edits or sequence a shared file explicitly.
- When collaboration is authorized, use AutoSwarm to assign, communicate and collect work.
  Inspect tracked calls and manually opened sessions before starting duplicate work.
  Handle routine coordination directly; do not make the owner relay messages.
- One coordinator owns Cargo and client slots. Never start a competing build/client or
  terminate another session to acquire a slot. Revalidate stale notes and PIDs.
- All Cargo build/check/test runs use sccache. The checkout's `.cargo/config.toml` sets
  `build.rustc-wrapper = "sccache"` and CI installs it with `mozilla-actions/sccache-action`.
  If `sccache` is missing, install it with the wrapper bypassed for that command
  (`$env:RUSTC_WRAPPER=''; cargo install sccache --locked`) or set `RUSTC_WRAPPER` to its full
  path before running Cargo; a missing wrapper is a build failure, not a reason to bypass it.
  Selecting the wrapper does not prove a cache hit: check `sccache --show-stats` before claiming
  one, and use `CARGO_INCREMENTAL=0` when a build should populate the cache.
- Use existing owners and real native contracts. Nil/constant returns and broad error
  suppression are not completed APIs. Name unsupported branches and missing consumers.
- Close visible bugs against their original symptom. Separate source, compilation,
  focused checks, receiver behavior and visible gameplay. No alpha claim from tests alone.
- Preserve dirty source and untracked evidence. Stage explicit owned paths; no blanket
  staging, reset/clean, shared-branch switch or pull/rebase underneath dirty work.

## Boundaries

- Game install, server/DB and preserved reference are read-only unless explicitly assigned.
  Never commit game data, extracted FrameXML/GlueXML, credentials or local runtime state.
- The owner enters credentials and drives combat. No unattended combat, automated persistent
  character creation/deletion, or competing login during an owner's live session.
- Use only authorized verification. Keep essential targeted cases within the current
  20-case task limit; do not fill that budget unnecessarily. Documentation maintenance
  needs no Cargo build, test suite, client or server run.
- Keep at most three completed source backups. Do not duplicate target or MPQ archives.
  See [repository rules](docs/agents/repository.md) and [backups](docs/agents/backups.md).

## Reference and documentation routing

- Wrath reference: D:/Projects/wowdecomp-reference, pin
  e67b6ffef23655dc7da061cb2d36b7963f6ef71a. Investigate behavior; do not copy implementation.
- Donor research: samwhosung/benilla-re establishes 1.12 behavior, not Wrath layouts.
- Read the installed project-b-12340-reference skill before target-specific repairs when
  available, and the installed autoswarm skill before delegation. Names do not install tools.
- Module/ABI work: [ADR-0002](docs/architecture/adr-0002-live-client-modules.md).
  A commit or staged DLL does not demonstrate activation in a running client.
- [Documentation map](docs/agents/domain.md), [issues](docs/agents/issue-tracker.md),
  [triage](docs/agents/triage-labels.md), [owner decisions](docs/baseline/owner-decisions.md).

Keep durable rules here and in linked guides, current reservations in active-ownership,
and results/evidence in the owning handoff. Avoid accumulating session history here.
