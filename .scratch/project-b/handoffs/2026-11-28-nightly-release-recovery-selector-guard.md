# Nightly: release-set recovery selector guard

Status: in progress
Owner: OpenCode / DeepSeek V4.1 Flash
Branch: fix/release-recovery-selector-guard
Issue: none
Next: add expected selector identity to Recover-ReleaseSet and cover it with the self-contained PowerShell test

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
- After the existing idempotent-recovery check, a mismatch refuses with a
  "selector changed since launch" error and leaves the selector untouched.
- `launch-client.ps1` passes the captured `$releaseContext` identity.
- The self-contained `tools/release-set.tests.ps1` covers a wrong version, a wrong manifest
  hash, an unchanged selector after refusal, matching expected recovery, and idempotent retry.

## Boundaries

No Cargo, client, server or game data. Only the existing PowerShell test harness (writes under
`F:\AI\tmp\opencode`) and a static parser check. The old worktree and branch
`fix/release-recovery-guardrails` are left untouched. Automatic updates, crash recovery
redesign and package assembly remain out of scope.
