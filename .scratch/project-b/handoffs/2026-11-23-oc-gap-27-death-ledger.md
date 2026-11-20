# Record instance and pet death edge cases in the compatibility ledger

Status: ready to merge
Owner: OpenCode / hy4-flash-free (Hy4 flash Free)
Branch: oc/gap-27-death-ledger
Issue: 27
Next: Integrator lands the two new death-edge rows on main; the live-only checks (instance corpse marker/route and a real dead-pet revive) stay owner-run.

## What changed

Added the subsection `## Death edge cases (2026-11-23)` to
`.scratch/project-b/compatibility/first-loop-behaviors.md` with two rows in the ledger's
existing five-column format (Behavior / Source assessment / N and R cases / T and P cases):

- **D07 — instance-bound corpse (corpse map differs from display map).** Marked Partial,
  source owner OpenCode / death owner, proof live-only.
- **D08 — pet death and revive.** Marked Partial for the dead-pet gating and refusal paths,
  Missing for the Revive Pet command, source owner OpenCode / pet owner, proof live-only.

No source code, Cargo, client or server was touched. This is acceptance-5 documentation only:
it records the gap, it does not close any acceptance item or promote a behavior to WORKING.

## Method

Read the issue's Small tickets section and the ledger's death rows D01–D06, then verified each
cited symbol by grep in the checked-out source (no execution):

- `crates/benilla-app/src/death/net.rs`: `on_corpse_query` destructures
  `SessionEvent::CorpseQuery { found, display_map, position, corpse_map }` and calls
  `corpse_query`, which stores `CorpsePoint`; `on_corpse_not_in_instance` pushes
  `ERR_CORPSE_IS_NOT_IN_INSTANCE`.
- `crates/benilla-app/src/death/mod.rs`: `CorpsePoint` carries `display_map`, `position`
  and an unadjusted `corpse_map`; the corpse-run range gate computes
  `same_map = display_map == corpse_map` and selects `CorpseRange::InInstance` /
  `CORPSE_IN_INSTANCE` when the maps differ.
- `crates/benilla-app/src/ui_pet/usable.rs::actions_usable` (verified with
  `ui_pet/usable_tests.rs::a_dead_pet_is_still_usable`) reads unit flags, not pet health;
  `ui_action/cast_fail.rs` maps `ERR_PET_SPELL_DEAD` / `ERR_PET_SPELL_TARGETS_DEAD`;
  `spell/cast_target.rs` has the dead-pet assist (`TF_CORPSE_ALLY`) leg.
- `crates/benilla-protocol/src/events.rs::PetTameFailure` is the only decoded revive/tame
  refusal; `crates/benilla-protocol/src/messages/stable.rs` explicitly has no
  `CMSG_STABLE_REVIVE_PET` verb, and no PetRevive `ClientCommand` exists in `ui_pet`.

Citations use file paths and symbol names only. No offsets were invented and no reference
addresses were copied into the ledger.

## Checks not run

- No Cargo build, check, clippy or test run (docs-only ticket; not authorized here).
- No client, server, database or live session; no corpse-query, ghost-instance or dead-pet
  revive was exercised. No CI compile evidence is expected for a Markdown-only change.

## What is not proven

- Whether a live dungeon/instance corpse on a different map actually produces the
  `CORPSE_IN_INSTANCE` marker and a correct entrance route (D07 is source Partial).
- Whether a real dead pet can be revived and what refusal/state the server returns; the Revive
  Pet command is not wired in source, so D08's revive leg is Missing.
- All N/R/T/P cases for both rows remain UNKNOWN as connected behavior; the rows record source
  support and the live-only verification needed to accept them. No behavior row is promoted and
  no percentage or full-replacement claim follows from this documentation change.
