# Domain docs layout

| Document | Responsibility |
|---|---|
| [CONTEXT.md](../../CONTEXT.md) | Scope, vocabulary and stable system boundaries |
| [AGENTS.md](../../AGENTS.md) | Agent bootstrap and standing constraints |
| [workflow.md](workflow.md) | Coordination, AutoSwarm, verification and handoff rules |
| [active-ownership.md](active-ownership.md) | Current ticket/file and build/client reservations |
| [repository.md](repository.md) | Git topology and publication |
| [launching.md](../launching.md) | Player launch/build commands and troubleshooting |
| [owner-decisions.md](../baseline/owner-decisions.md) | Adopted decisions and dated amendments |
| [architecture/](../architecture/) | ADRs, architecture reviews and traceability |
| .scratch/project-b/issues/ | Owning implementation tickets |
| .scratch/project-b/handoffs/ | Dated source/run evidence and continuation instructions |

ADRs live in `docs/architecture/adr-NNNN-<slug>.md`; there is no current docs/adr directory.
Read relevant existing ADRs before changing an interface or extension contract. Record a
new architectural decision when the contract changes, not for every routine bug fix.
Per-ticket file reservations belong in active-ownership, not in an ADR.

Keep generated MAP.md and DEVIATIONS.md generated. Do not replace historical handoffs with
new status claims or turn runtime snapshots into standing rules. Absolute workstation paths
are local conveniences; source/control state must be revalidated at admission.
