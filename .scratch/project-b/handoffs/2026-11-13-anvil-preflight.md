# Anvil — "preflight: NET OFF" false positive — fix round handoff

- Owner agent: Anvil (fix round)
- Target file (owned): `crates/benilla-app/src/preflight.rs` (edit scope honored; no edits outside)
- Head: `680c6eac` (branch `main`)
- Date: 2026-11-12 / session continued 2026-11-13

## Change summary

`offline_notice` no longer emits the `preflight: NET OFF` warning when the session is a Wrath
session. A Wrath session owns the net loop (its `WrathNetPlugin` runs the IO thread the stock
`NetPlugin` parks without), so the parked `NetOffline` inserted by the stock `NetPlugin` for a Wrath
run is the Wrath design, not a capture gap.

- Module doc item 4 (`preflight.rs` ~line 13-16) updated: a Wrath run is never warned.
- New const `NET_OFF_WARNING` (`preflight.rs` ~line 105).
- New pure decision fn `net_offline_finding(wrath_owns_net: bool, offline: bool) -> Option<&'static str>`
  (`preflight.rs` ~line 110-115): `None` if `!offline || wrath_owns_net`, else `Some(NET_OFF_WARNING)`.
- `offline_notice` signature changed to
  `fn offline_notice(wrath: Option<Res<crate::wrath::WrathInput>>, net: Option<Res<crate::net::NetOffline>>)`
  and now warns only when `net_offline_finding` returns a line.
- Regression tests added in the `preflight.rs` tests module (~lines 440-456):
  - `a_wrath_session_never_alarms_the_wire_path_offline` — `net_offline_finding(true, true).is_none()`
  - `a_stock_capture_run_without_an_io_thread_alarms_net_off` — `net_offline_finding(false, true) == Some(NET_OFF_WARNING)`
  - `a_run_with_an_io_thread_has_no_net_off_finding` — `net_offline_finding(false, false).is_none()`

Silence (no Wrath banner) was chosen; task permitted it and it is the minimal change.

## Evidence / root cause

- `lib.rs:467-471`: `connect: !capturing && wrath_config.is_none()` — a Wrath run gets
  `NetPlugin { connect: false }`, which inserts `NetOffline` (`net.rs:59-79`).
- Real IO loop for Wrath: `game_plugins.rs:131-138` (`WrathNetPlugin` adds the IO thread);
  `WrathInput` inserted at `wrath.rs:2367` when `config.is_some()`.
- Donor pattern for the same question: `quest_trace.rs:7128-7138` (`resolve_protocol_mode`).

## Verification

- Focused test: `cargo test -p benilla-app --lib preflight`
  - Result: **ok** — 12 passed; 0 failed; 6744 filtered out; finished in ~0.07s (includes the 3 new tests).
- Type check: `cargo check -p benilla-app`
  - Result: **ok — exit code 0** ("Finished `dev` profile [optimized + debuginfo] target(s) in 3m 26s"),
    warnings only (pre-existing dead-code / unused-import warnings in benilla-app and sibling crates);
    no errors.
- An earlier `cargo check -p benilla-app` run had been blocked waiting on the cargo target-dir lock
  held by a peer (Forge's `cargo test -p benilla-app --lib spell::mods`); after it cleared, the check
  completed. A follow-up check run (post-clear) also finished clean: exit 0, Finished line, warnings only.

## sccache

- Post-focused-test snapshot: Compile requests 53, executed 46, cache hits 3 (8.57%), cache misses 32
  (8.57%), failures 0, proc-macro non-cacheable calls 7.
- Final snapshot taken later in the round showed a fresh daemon state (sccache daemon restarted at
  ~2026-11-13 00:00:13 local); latest reading: 1 compile request, 0 executed, 0 hits / 0 misses,
  non-cacheable reason `incremental` (1).

## Findings outside ownership (reported, not fixed by Anvil)

1. `net.rs:65-68` — `NetOffline` doc still says something like "crate::preflight announces it";
   wording is now conditionally stale once preflight's warning became Wrath-aware. Candidate rewording
   belongs to the net-owner (Forge / net module owner) — NOT edited here.
2. Diagnostics log double-prefixes the warning: `benilla_app::preflight: preflight: NET OFF ...`
   (module path + `warn!` prefix) — cosmetic finding, owner of the warn path to decide.
3. Pre-existing unrelated compile error in `crates/benilla-formats/src/pet_stats.rs:184` (E0599
   `PetLoyaltyNames::default` / `PetPersonalities::default`) blocked an early `cargo check`; not fixed
   (outside ownership; Forge's formats owner territory).

## Constraints honored

- No commit, staging, push, client/server launch, migration or config change performed.
- Only `crates/benilla-app/src/preflight.rs` edited by Anvil this round.
- No drive-by refactors.
