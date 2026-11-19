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

Method: files and handoffs read; no Cargo, test, client or server run.

| Acceptance line | Source that meets it | Gap |
|---|---|---|
| Use sampled v264 model and external skin format; verify header/array/layout requirements | `benilla-m2/src/track.rs:M2Track` | none |
| Select creature model variant; handle texture/bone/animation dependencies | `benilla-app/src/creature_anim.rs:select` module | creature-to-model-variant selection with texture/bone/animation dependency resolution from manifest not fully verified |
| Read external skins through manifest asset source to renderer-facing types | `benilla-assets/src/m2.rs:M2Model` loader | external skin reading through manifest asset source to renderer types not fully verified |
| Demonstrate orientation, scale, skin indices, basic animation and relevant attachments against the matched reference profile | `benilla-m2/src/model.rs:M2Bone` | no demonstration/verification pipeline comparing parsed data against the wowdecomp-reference profile |
| Malformed/truncated dependencies fail with useful diagnostics; preserve currently supported older formats | `benilla-m2/src/error.rs:Error` | malformed/truncated M2 dependencies may not produce useful diagnostic messages; older format preservation not explicitly verified |
| List any other model/animation variants as compatibility ledger gaps instead of claiming universal model support | none found | compatibility ledger for model/animation variants not formally documented as a source code file; project practice per acceptance criteria |
## Source gap list (2026-11-21, OpenCode swarm)

Method: files and handoffs read; no Cargo, test, client or server run.

| Acceptance line | Source that meets it | Gap |
|---|---|---|
| Use sampled v264 model and external skin format; verify actual header/array/layout requirements rather than just widening version acceptance | `benilla-m2/src/track.rs:18:M2Track` — Wrath (v264) shrank record to `0x14` bytes with per-sequence timestamps @`0x04` and values @`0x0c` as two-level arrays | verify v264 `M2Track` key-count padding (`min(timestamps, values)`) and sequence-band disambiguation against reference; current reader flattens ranges but does not validate per-sequence key bounds |
| Select one required creature and the selected player's model variant; handle their required texture/bone/animation dependencies | `benilla-app/src/creature_anim.rs:18:pub(crate) mod select` — gait/swing/ready picks from `0x5fd100` tables; `0x5fe2f0` speed/sequence lookup | verify creature-to-model-variant selection propagates texture/bone/animation dependencies from manifest asset source; `select` module exists but manifest-sourced skin-slot-to-animation mapping not wired |
| Read external skins through the manifest asset source and feed the existing renderer-facing types where appropriate | `benilla-assets/src/m2.rs:M2Model` — `skin_slot: Option<u8>` (Monster1/2/3 variant), `geoset_id: u16`, `texture_coord_combo_index: u16`; `parse_m2_render_submeshes_with_skin` applies skin at spawn | verify external skin reading through manifest asset source feeds `M2Model.skin_slot` and renderer-facing types; current loader decodes embedded skin but manifest→M2 skin-slot pipeline not verified end-to-end |
| Demonstrate orientation, scale, skin indices, basic animation and relevant attachments against the matched reference profile | — | **Blocked: needs owner run** — the pinned reference at `D:/Projects/wow/reference/wowdecomp-reference` (pin e67b6ffe) requires a running client to demonstrate externally visible behavior (pose, skin, animation, attachments). Source structures (`M2Bone` at `benilla-m2/src/model.rs:93`, `M2Track` at `benilla-m2/src/track.rs:18`) exist but pose/skin/animation verification against the reference profile needs live client |
| Malformed/truncated dependencies fail with useful diagnostics; preserve currently supported older formats | `benilla-m2/src/error.rs:4:Error::Truncated` — `write!(f, "truncated M2")` with Display impl; `Error::UnsupportedVersion(v)` writes `"unsupported M2 version {v} (expected 256..=264)"` | verify that parse entry points produce these diagnostic messages on truncated/unsupported M2 files; older format (v256) preservation in the track/skin readers not explicitly checked |
| List any other model/animation variants as compatibility ledger gaps instead of claiming universal model support | none found — no compatibility ledger file exists in the crate | add a compatibility ledger (e.g., `benilla-formats` or project doc) listing unsupported model/animation variant IDs instead of claiming universal model support; currently no such ledger exists |

### Tickets opened by this survey

1. `oc/wotlk-actor-v264-track-verification` — one behavior: verify v264 M2 track reader handles per-sequence layout and key-count padding; 1-2 files: `benilla-m2/src/track.rs` (lines 18-78); 1-3 checkable acceptance sentences: (a) `M2Track::keys` length matches `min(timestamps, values)` per the Wrath spec, (b) `ranges` windows correctly disambiguate per-sequence key search, (c) v256-era track format (interp@0, gseq@2, values@0x0c, timestamps@0x04) still parses without panic. Named test: `bench track v264 parsing · handles per-sequence keys` in `benilla-m2`.

2. `oc/wotlk-actor-manifest-skin-pipeline` — one behavior: verify external skin from manifest asset source feeds M2Model skin_slot and renderer types; 1-2 files: `benilla-assets/src/m2.rs` (M2Model struct), `benilla-app/src/creature_anim.rs` (select module); 1-3 checkable acceptance sentences: (a) manifest-loaded skin_slot value propagates to M2Model.skin_slot, (b) texture_combo_index references valid texture unit in renderer, (c) malformed manifest skin data logs diagnostic rather than silent failure. Named test: `manifest skin slot propagation · fills skin_slot from appearance DB`.

3. `oc/wotlk-actor-error-diagnostics` — one behavior: verify M2 parse errors produce useful diagnostics for truncated/unsupported formats; 1 file: `benilla-m2/src/error.rs` (Error enum, lines 4-15); 1-3 checkable acceptance sentences: (a) truncated M2 file yields `Error::Truncated` with "truncated M2" message, (b) unsupported version yields `Error::UnsupportedVersion` with "expected 256..=264" message, (c) error Display impl formats variants for user-facing logs. Named test: `M2 parse error messages · match reference string outputs`.

4. `Blocked: needs owner run` — which acceptance line: bullet 4 "Demonstrate orientation, scale, skin indices, basic animation and relevant attachments against the matched reference profile" — why only a live client/server can judge it: the wowdecomp-reference profile at `D:/Projects/wow/reference/wowdecomp-reference` (pin e67b6ffe) is read-only and requires a running client to demonstrate externally visible behavior (pose, skin, animation, attachments) against matched reference profile.
