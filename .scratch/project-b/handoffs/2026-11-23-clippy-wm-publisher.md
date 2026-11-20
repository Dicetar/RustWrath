# Clippy: box PublishOutcome::Retained payload

Status: ready to merge
Owner: Flint 40B Free
Branch: oc/clippy-wm-publisher
Issue: none
Next: land after green CI

## What changed

- `crates/benilla-wm-native/src/publisher.rs`: fixed the clippy `large size difference between variants`
  finding on `PublishOutcome` (reported at the enum, publisher.rs :289) by boxing the large retained
  payload. The `Retained` variant now holds `Retained(Box<RetainedObservation>)` instead of
  `Retained(RetainedObservation)`, and the busy-handoff construction wraps the retained observation in
  `Box::new`:

  `return Ok(PublishOutcome::Retained(Box::new(RetainedObservation::handoff_busy(observation))));`

## Findings removed

- `large size difference between variants` on `PublishOutcome`: the `Retained(RetainedObservation)`
  variant dominated the size difference against the small `HandedOff(PublishReceipt)` and
  `Superseded` variants. Boxing the payload removes the dominance.

## Construction / match / consumer audit

- Grepped the workspace for `PublishOutcome::Retained` and `RetainedObservation` uses (including
  tests). The only code consumers are in `crates/benilla-wm-native`: the busy-handoff construction, the
  pump match arm (`Ok(PublishOutcome::Retained(_))`, which discards the payload), and the `retry` entry
  that accepts a `RetainedObservation`. All remain valid with the boxed payload; the enum derives
  (`Debug`, `PartialEq`) still hold through the `Box<T>` impls, and no behavior changed.

## Checks not run

- No Cargo, no build, no clippy run, no tests, no client/server (per task). Formatting on the edited
  file was normalized with the standalone `rustfmt` binary and reviewed via `git diff`.

## What is not proven

- No local compilation or clippy confirmation; CI must validate the fix and that no dependent code
  relied on the unboxed shape.
- No runtime/behavioral verification; the change is type-level only and intended to be behavior-neutral.
