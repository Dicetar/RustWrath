# 20 — Render and animate a route WotLK actor

**Status:** ready-for-agent

**Plan reference:** C05. Owner approved ticket sizes and blockers on 2026-10-25. See [the approved completion queue](../completion-index.md).

**Implementation state:** Acceptance open — existing code may cover part of this slice; inspect the current checkpoint before starting.

**What to build:** A source-identified WotLK actor appears with the correct mesh, skin, textures and animation in the same application route.

**Blocked by:** [19 — Render route terrain from a selected manifest profile](19-profile-route-in-application.md)

**Related existing issues:** [05](05-b004-route-assets-provenance-loader.md). These are traceability links, not additional blocking edges; existing issues and evidence are preserved.

**Suggested owner:** OpenCode.

## Acceptance

- [ ] Use the sampled v264 model and external skin format; verify actual header/array/layout requirements rather than just widening version acceptance.
- [ ] Select one required creature and the selected player's model variant; handle their required texture/bone/animation dependencies.
- [ ] Read external skins through the manifest asset source and feed the existing renderer-facing types where appropriate.
- [ ] Demonstrate orientation, scale, skin indices, basic animation and relevant attachments against the matched reference profile.
- [ ] Malformed/truncated dependencies fail with useful diagnostics; preserve currently supported older formats.
- [ ] List any other model/animation variants as compatibility ledger gaps instead of claiming universal model support.

## Boundaries and handoff

No donor engine replacement or unsupported all-version promise. Future checks require execution authorization; source implementation alone remains PARTIAL.

Keep the slice within one fresh implementation context. If exploration reveals additional independent behavior, identify the smallest complete child slices before expanding scope. Read the current project context, relevant skill/rules and source checkpoint. Preserve unrelated edits and existing ownership.

Execution checks described above are acceptance requirements, not authorization. Follow explicit permissions applicable to the executing agent. Report source implementation, compilation and runtime evidence separately. An unexecuted required check remains UNKNOWN. Do not commit, deploy, start services or mutate live state merely because this published ticket exists.

Handoff: exact source identity and changed paths; externally visible behavior; evidence actually collected; unresolved acceptance; next dependent slice. Close only after acceptance is supported. See [the completion plan](D:/WOW/PROJECT_B_COMPLETION_PLAN.md) for the mandatory full stock-client replacement release gate.

## Source gap list (2026-11-21, OpenCode swarm)

Method: read `crates/benilla-m2/src/track.rs`, `model.rs`, `error.rs`, `crates/benilla-assets/src/m2.rs` (`M2Model`, `M2ModelLoader`, `parse_m2_render_submeshes_with_skin`), `crates/benilla-app/src/creature_anim.rs` (`select`) and `crates/benilla-app/src/entities/attach/`. No Cargo, test, client or server run.

| Acceptance line | Source that meets it | Gap |
|---|---|---|
| Use the sampled v264 model and external skin format; verify actual header/array/layout requirements rather than just widening version acceptance | `crates/benilla-m2/src/track.rs:18` `M2Track` — v264 record is `0x14` bytes, timestamps at `0x04` and values at `0x0c` are two-level per-sequence arrays; `track_read` flattens them into `ranges` | no test asserts the flattened key count is `min(timestamps, values)` per sequence, or that band boundaries do not leak between sequences; a v256 track is only covered indirectly |
| Select one required creature and the selected player's model variant; handle their required texture/bone/animation dependencies | `crates/benilla-app/src/creature_anim.rs:18` `pub(crate) mod select` — gait/swing/ready picks from the `0x5fd100` tables and `0x5fe2f0` rate lookup | the manifest/appearance skin slot is not shown to reach the per-unit animation selection; the select module takes a model id, not a display/skin dependency set. This needs its own survey before a ticket can name a file. |
| Read external skins through the manifest asset source and feed the existing renderer-facing types where appropriate | `crates/benilla-assets/src/m2.rs` `M2Model` (`skin_slot: Option<u8>`, `geoset_id`, `texture_coord_combo_index`); `M2ModelLoader` and `parse_m2_render_submeshes_with_skin` | the loader reads skin data from the model's own source, but no code path was found that takes `skin_slot` from the selected manifest profile and feeds it into the loader for a creature; this is the same gap as the row above |
| Demonstrate orientation, scale, skin indices, basic animation and relevant attachments against the matched reference profile | structures exist: `crates/benilla-m2/src/model.rs:93` `M2Bone`, `crates/benilla-m2/src/track.rs:18` `M2Track` | **Blocked: needs owner run** — pose, skin and attachment placement are visible behavior; source review cannot demonstrate them against the reference profile |
| Malformed/truncated dependencies fail with useful diagnostics; preserve currently supported older formats | `crates/benilla-m2/src/error.rs:4` `Error::Truncated` ("truncated M2"), `Error::UnsupportedVersion(v)` ("expected 256..=264"), `Error::NotSkin` | no test covers the error text a truncated skin or an unsupported version produces at the parse entry points; the v256 track/skin path is not exercised |
| List any other model/animation variants as compatibility ledger gaps instead of claiming universal model support | no compatibility ledger exists in the crates or docs | add a ledger row for every unsupported M2/skin/animation variant; today the supported range `256..=264` is the only statement, and it is not a ledger |

### Tickets opened by this survey

1. `oc/m2-v264-track-keys` — one behavior: make the v264 track reader's per-sequence key window explicit and test it. Files: `crates/benilla-m2/src/track.rs`. Acceptance: (a) a v264 track with two sequences flattens to each sequence's own key count, not the sum; (b) `ranges[i]` brackets only sequence i's keys; (c) a v256 track still parses. Tests: `v264_track_keeps_each_sequences_keys_in_its_band`, `v256_track_still_parses`.
2. `oc/m2-error-diagnostics` — one behavior: cover the M2 parse errors with the strings the client shows. Files: `crates/benilla-m2/src/error.rs` (tests only). Acceptance: a truncated model yields `truncated M2`, an unsupported version yields `unsupported M2 version N (expected 256..=264)`, and a non-SKIN payload yields `not a SKIN profile`. Test: `m2_parse_errors_name_the_failure`.
3. `oc/m2-variant-ledger` — one behavior: add a compatibility ledger for M2/skin/animation variants. File: a short doc under `docs/` or the `benilla-m2` crate doc. Acceptance: the ledger names each unsupported variant beside the supported `256..=264` range and does not claim universal support. No runtime test; `scripts/doc-links.py` is the check.
4. `Blocked: needs owner run` — acceptance line 4 (orientation, scale, skin indices, animation, attachments against the reference profile) is visible behavior and cannot be judged from source.
