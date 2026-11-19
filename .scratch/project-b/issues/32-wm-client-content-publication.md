# 32 — WM publishes a client-content revision and rolls it back

**Status:** ready-for-agent

**Plan reference:** C17. Owner approved ticket sizes and blockers on 2026-10-25. See [the approved completion queue](../completion-index.md).

**Implementation state:** Acceptance open — existing code may cover part of this slice; inspect the current checkpoint before starting.

**What to build:** WM publishes a versioned client-content revision to the intended client, receives application status and can restore the previous revision.

**Blocked by:** [17 — WM consumes native state with fallback](17-wm-consumes-native-state.md); [31 — Reload a client asset and Lua behavior with rollback](31-client-live-content-reload.md)

**Related existing issues:** [15](15-client-hotpatch.md). These are traceability links, not additional blocking edges; existing issues and evidence are preserved.

**Suggested owner:** OpenCode owns publication; GLM owns application adoption.

## Acceptance

- [ ] Use existing WM validation/publication and an explicit content delivery interface; observation-only transport is not already a publication protocol.
- [ ] Bind package identity, version, content hash, compatibility and actor/session scope.
- [ ] Stage and validate before application; interrupted transfer or malformed content preserves the current revision.
- [ ] Distinguish delivered, validated, applied and failed; WM observes the client's actual revision.
- [ ] Demonstrate live adoption and rollback with no client restart for the selected asset/Lua capability.
- [ ] Preserve baseline mode and do not admit arbitrary native code execution as content.

## Boundaries and handoff

Transport selection follows payload/lifecycle needs. Avoid adding a generic bidirectional framework solely because the observation bridge exists.

Keep the slice within one fresh implementation context. If exploration reveals additional independent behavior, identify the smallest complete child slices before expanding scope. Read the current project context, relevant skill/rules and source checkpoint. Preserve unrelated edits and existing ownership.

Execution checks described above are acceptance requirements, not authorization. Follow explicit permissions applicable to the executing agent. Report source implementation, compilation and runtime evidence separately. An unexecuted required check remains UNKNOWN. Do not commit, deploy, start services or mutate live state merely because this published ticket exists.

Handoff: exact source identity and changed paths; externally visible behavior; evidence actually collected; unresolved acceptance; next dependent slice. Close only after acceptance is supported. See [the completion plan](D:/WOW/PROJECT_B_COMPLETION_PLAN.md) for the mandatory full stock-client replacement release gate.

## Source gap list (2026-11-21, OpenCode swarm)

Method: read the issue's six acceptance lines; `AGENTS.md` and `docs/agents/sync.md` for the note
shape; issues 31 and 15 plus `.scratch/project-b/completion-index.md` (C17); handoffs
`2026-11-05-autoswarm-waves`, `2026-11-09-modular-hotpatch-closure`,
`2026-11-17-live-code-hotpatch` and `2026-11-21-oc-gap-30`; and the owning source —
`crates/benilla-assets/src/profile_source.rs`, `crates/benilla-profile/src/lib.rs`,
`crates/benilla-app/src/ui_script/lifecycle.rs`, `crates/benilla-app/src/{wrath,wrath_native}.rs`,
`crates/benilla-wm-native/src/{frame,client,publisher,observation,pump,lib}.rs`,
`crates/benilla-wm-driver/src/main.rs`, `crates/benilla-hotpatch/src/lib.rs`,
`docs/architecture/adr-0001-wrath-compatibility-boundary.md`, `docs/architecture/adr-0002-live-client-modules.md`
and `docs/opencode/live-change-integration.md`. I also read the read-only WM project under
`D:/WOW/wm-project/src/wm/{content,live,proofs}` (a separate repository); no client layout question
required the pinned e67b6ffe reference. No Cargo, cargo fmt, clippy, test, client or server run; no
file outside this worktree was written.

| Acceptance line | Source that meets it | Gap |
|---|---|---|
| Use existing WM validation/publication and an explicit content delivery interface; observation-only transport is not already a publication protocol | Server-side WM validation/publication exists: `D:/WOW/wm-project/src/wm/content/release.py:502 validate_content_release_spec`, `.../content/workbench.py:893 publish_result_allows_runtime`, `.../quests/publish/__init__.py:397 publish`, `.../items/publish.py:248 publish`. The client's only WM surface is `crates/benilla-wm-native/src/frame.rs:21 HelloFrame`, `:31 ObservationFrame`, `:53 ServerFrame` and `publisher.rs:380 publish` (Observation); the inbound loop at `client.rs:301` handles only Ack, ActionRequest, ActionCancel, HealthAck and Error. | `ServerFrame` has no content-delivery or content-status variant, and `ui_script/lifecycle.rs:1260 run_pending_reload` reopens a fixed local manifest (`benilla-profile/src/lib.rs:90 Profile::open`). Nothing delivers a package to the client, so the observation bridge is not a publication protocol. The WM sender half lives in `D:/WOW/wm-project`, outside this repository. |
| Bind package identity, version, content hash, compatibility and actor/session scope | `profile_source.rs:218 StagedContentRevision`, `:268 CommittedContentRevision`, `:135 ContentArchive`, `:125 ValidatedContentInput` and `:255 provenance()`; session scope exists only on observations at `crates/benilla-app/src/wrath_native.rs:78 WrathScope`. | Identity is one monotonic serial (`profile_source.rs:236 serial()`, `:355 revision_serial()`, `:404 fetch_add`); `provenance()` is a display string. No package id, version, content hash, compatibility field or actor/session scope is bound to a revision. `commit_content_revision:417` checks only manifest path and base serial (`:441`, `:527 check_commit_base`), and `stage_content_revision:369` computes no digest of the candidate. |
| Stage and validate before application; interrupted transfer or malformed content preserves the current revision | `profile_source.rs:369 stage_content_revision` opens the candidate off-lock (`:379`), builds a chain (`:385`) and validates required inputs (`:398`); `:417 commit_content_revision` is guarded by `:527 check_commit_base`; `lifecycle.rs:1277` stages, `:1280 refuse_reload` handles refusal, `:1352` commits after `close_ui_session`, and `:1370` keeps the old content when commit fails. | There is no transfer to interrupt: staging reopens the same on-disk manifest (`benilla-profile/src/lib.rs:49 Manifest::parse`, `:90 Profile::open`), so "interrupted transfer" is unrepresented and no partial-delivery state exists. Validation is header-only by class (`profile_source.rs:67 ContentInputKind::check`: WDBC, MD20/MD21, BLP1/BLP2, UTF-8) with no digest, so a header-correct but malformed file is still committed. Handoff `2026-11-05-autoswarm-waves.md:161` also records a post-boundary FrameXML failure as not rollback-able. |
| Distinguish delivered, validated, applied and failed; WM observes the client's actual revision | `profile_source.rs` exposes only staged, committed and error (`:218`, `:268`, `:143 ContentRevisionError`); `lifecycle.rs:1423 UiReloadFailure` and `:1332`/`:1390` `Stage::UiReload`/`UiReloadLoaded` are local trace records; `wm_native/frame.rs:62 Ack { state_revision, ... }` and `observation.rs:894 state_revision` carry world state only. | No delivered/validated/applied/failed status type exists, and no frame or observation carries the client's content serial. The applied revision is only logged at `lifecycle.rs:1365 info!("... adopted {provenance}")`; WM sees world `state_revision` and cannot distinguish the four states or read back the actual content revision. |
| Demonstrate live adoption and rollback with no client restart for the selected asset/Lua capability | `lifecycle.rs:1260 run_pending_reload` stages content and a candidate VM, crosses one boundary (`:1347`), commits content at `:1352` and swaps the VM at `:1378` without a restart; `profile_source.rs:455 reload` and the reload tests named in `2026-11-05-autoswarm-waves.md:156-157` cover stage/commit. | No rollback owner exists: `commit_content_revision:417` overwrites `active` at `:440` and retains no handle to the previous profile or chain, so the only way back is re-staging the now-changed on-disk manifest. `RELOAD_CONTENT_INPUTS:1414` names only two global Lua files, so no selected asset or Lua capability is targeted or proven. The no-restart adoption is source-visible but unrun. |
| Preserve baseline mode and do not admit arbitrary native code execution as content | `benilla-profile/src/lib.rs:26 Origin::{Stock,Wm}` and `:49 Manifest::parse` accept only `stock` or `wm` `.mpq` lines; `profile_source.rs:290 describe_archives` labels origin but does not gate it. Native code is a separate opt-in path behind `benilla-hotpatch/src/lib.rs:35 Library::new` and `verify_publication`; mode contracts live at `docs/architecture/adr-0001-wrath-compatibility-boundary.md:141` and `docs/architecture/adr-0002-live-client-modules.md:859 capability.required_mode`. | No mode or payload guard on the content path: `stage_content_revision:369` stages any manifest of `.mpq` names and `ContentInputKind::check:67` proves signatures only. Nothing marks a revision stock/baseline, rejects an executable payload, or ties the hotpatch DLL feature to a content revision, so a content revision could become a native-code delivery channel with no check to refuse it. |

### Tickets opened by this survey

1. `oc/wm-content-package-identity` — give a staged revision a checkable identity and digest. Accept when: `StagedContentRevision` carries `package_id`, `version`, `content_hash` and `actor_scope`, and `provenance()` prints them; re-staging after the candidate bytes change yields a different `content_hash`; a commit whose staged digest no longer matches refuses and leaves the active revision untouched. Files: `crates/benilla-assets/src/profile_source.rs`, `crates/benilla-assets/Cargo.toml`. Tests (in `profile_source.rs`): `a_staged_revision_reports_identity_version_hash_and_scope`, `a_changed_candidate_produces_a_different_content_hash`.
2. `oc/wm-content-delivery-frame` — add an inbound typed content offer to the WM native bridge and hand it to the driver without applying it. Accept when: `ServerFrame::ContentOffer` with identity, version, hash and scope round-trips and reaches the caller; an offer whose session/actor scope does not match is refused with a named error and sends nothing; no path in the intake loads a library or calls a DLL. Files: `crates/benilla-wm-native/src/frame.rs`, `crates/benilla-wm-native/src/client.rs`. Tests: `a_content_offer_frame_round_trips_identity_version_hash_and_scope`, `an_unscoped_content_offer_is_refused_without_a_send`.
3. `oc/wm-content-revision-status` — let WM see the four delivery states and the applied content serial. Accept when: `Observation` carries `content_revision: Option<u64>` and `content_status` in `{delivered, validated, applied, failed}`, populated from `ProfileAssetReader::revision_serial()` at export; an unset status is distinct from `failed`; `Ack`'s `state_revision` remains world state and is not reused for content. Files: `crates/benilla-wm-native/src/observation.rs`, `crates/benilla-app/src/wrath.rs`. Tests (in `observation.rs`): `a_content_status_is_exported_with_the_revision_serial`, `an_unset_content_status_is_distinct_from_failed`.
4. `oc/client-content-rollback` — remember the previous active revision and restore it at one commit. Accept when: after an A-to-B commit, `rollback_content_revision()` makes `revision_serial()` and the profile/chain resolve from A again; a rollback after a second commit is refused as superseded; readers that already cloned B finish on B. Files: `crates/benilla-assets/src/profile_source.rs`, `crates/benilla-app/src/ui_script/lifecycle.rs`. Tests (in `profile_source.rs`): `rollback_restores_the_previous_serial_and_profile`, `rollback_after_a_second_commit_is_refused`.
5. `oc/client-content-mode-gate` — refuse content revisions that are not data/UI-only. Accept when: a candidate whose manifest or inputs name an executable module, or an origin other than stock/wm, is refused at staging with a named reason; a stock-plus-Lua/DBC revision still stages; the guard is reachable only through `stage_content_revision`. Files: `crates/benilla-assets/src/profile_source.rs`, `crates/benilla-profile/src/lib.rs`. Tests (in `profile_source.rs`): `a_revision_with_an_executable_payload_is_refused_at_staging`, `a_stock_and_lua_revision_stages`.
6. `Blocked: outside this repository` — the WM sender half of tickets 2 and 3 (the publish/send command and rollback command in `D:/WOW/wm-project`). The client-side intake and status types can land here first; the overseer decides who owns the WM project changes before this row can close.

### Blocked: needs owner run

- Row 5's live adoption of a selected asset/Lua capability with no restart: only the owner starts the client and drives the UI. Source shows the stage/commit/VM boundary, not that a specific asset or Lua behavior visibly changed in a running client.
- Row 4's "WM observes the client's actual revision": proving that observed status matches the applied revision needs a live WM bridge, client and server; source can only show that the fields are missing.
- Row 3's "interrupted transfer preserves the current revision": once a delivery protocol exists, only a live interrupted transfer can prove the current revision survives; today there is no transfer to interrupt.
- Rows 1, 2 and 6 have source-side gaps only; their smallest slices are tickets 1, 2 and 5.



