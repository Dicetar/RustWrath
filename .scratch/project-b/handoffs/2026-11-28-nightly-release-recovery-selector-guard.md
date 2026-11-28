# Nightly: release-set recovery selector guard

Status: ready to merge
Owner: OpenCode / DeepSeek V4.1 Flash
Branch: fix/release-recovery-selector-guard
Issue: none
Next: the integrator can land; no Cargo, client, server or game data was run

Base: `origin/main` `47e5e28b0` (the 2026-11-28 Windows package integration).
Basis: the owner's 2026-11-28 private-alpha assessment names manual package recovery and the
release-set health check as the next source-addressable P1. The older unstarted
`2026-11-15-release-recovery-guardrails` note covers the same area; this is the bounded first
slice, not a redesign.

## Problem

`tools/launch-client.ps1` captures the active version and manifest hash before launching a
release set. On a failed startup health check it calls `Recover-ReleaseSet` with only the
release root, and that function re-reads whichever selector is active at recovery time. If
another activation changes the selector while the client is starting, recovery can roll back a
release that this launch never selected. The default health check's `running` result is also
only "process stayed alive", not login or world success; the launcher already records it as
`health_check=running`, and this slice does not turn that into a gameplay check.

## Change

- `Recover-ReleaseSet` accepts `-ExpectedVersionId` and `-ExpectedManifestSha256`.
- The idempotent-recovery result is returned only when its recovered-from identity matches the
  caller's expected identity. Any other mismatch refuses with a "selector changed since launch"
  error and leaves the selector untouched.
- `launch-client.ps1` passes the captured `$releaseContext` identity.
- The self-contained `tools/release-set.tests.ps1` covers a wrong version, a wrong manifest
  hash, an unchanged selector after refusal, matching expected recovery, and idempotent retry.

## Verification

- Static parser: `release-set.ps1`, `launch-client.ps1` and `release-set.tests.ps1` all
  parse with 0 errors.
- `tools/release-set.tests.ps1`: **68 passed, 0 failed**. The new cases cover a wrong expected
  version, a wrong expected manifest, an unchanged selector after each refusal, matching
  expected recovery, idempotent retry with the launched identity, and a different earlier
  recovery that must not be reported as this launch's rollback.
- No Cargo, client, server, game data or second-machine run. The test harness writes only under
  `F:\AI\tmp\opencode`.
- CI 46151553655 (push `3a4a4d743`): **success**, `gates` 9m31s and `windows` 3m58s.
  The push changed only PowerShell scripts and the task note, so `changed` reported `none`;
  the Rust test steps were skipped. CI compiles the workspace but does not execute the
  PowerShell tests; the 68-case local run above is the test evidence.

## Boundaries

The old worktree and branch `fix/release-recovery-guardrails` are left untouched. Automatic
updates, crash recovery redesign and package assembly remain out of scope. CI does not execute
the PowerShell tests; the focused local run above is the test evidence.
