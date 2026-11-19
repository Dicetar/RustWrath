# 31 — Reload a client asset and Lua behavior with rollback

**Status:** ready-for-agent

**Plan reference:** C16. Owner approved ticket sizes and blockers on 2026-10-25. See [the approved completion queue](../completion-index.md).

**Implementation state:** Acceptance open — existing code may cover part of this slice; inspect the current checkpoint before starting.

**What to build:** A running client adopts a staged asset and Lua behavior revision and returns to its previous working version when validation fails or rollback is requested.

**Blocked by:** [20 — Render and animate a route WotLK actor](20-wotlk-actor-rendered.md) The selected Lua/UI file must load through the actual WotLK host; missing behavior becomes an explicit local blocker before this slice can close.

**Related existing issues:** [15](15-client-hotpatch.md). These are traceability links, not additional blocking edges; existing issues and evidence are preserved.

**Suggested owner:** OpenCode.

## Acceptance

- [ ] Use a revisioned staging path outside the read-only source install; prepare the complete replacement before switching.
- [ ] Carry asset reload through the real GPU/dependency cache lifecycle; archive reload alone is insufficient.
- [ ] Use the existing UI/addon reload lifecycle and preserve declared state/SavedVariables behavior.
- [ ] Show the visible asset and actual Lua behavior change without restarting the client.
- [ ] Reject a corrupt/incompatible revision and keep the prior working content; prove rollback.
- [ ] Record active revision and affected scope; do not claim native code hot swapping.

## Boundaries and handoff

If the required WotLK UI host surface is unavailable, add the smallest behavior slice as a blocker rather than substituting an unrelated script demo.

Keep the slice within one fresh implementation context. If exploration reveals additional independent behavior, identify the smallest complete child slices before expanding scope. Read the current project context, relevant skill/rules and source checkpoint. Preserve unrelated edits and existing ownership.

Execution checks described above are acceptance requirements, not authorization. Follow explicit permissions applicable to the executing agent. Report source implementation, compilation and runtime evidence separately. An unexecuted required check remains UNKNOWN. Do not commit, deploy, start services or mutate live state merely because this published ticket exists.

Handoff: exact source identity and changed paths; externally visible behavior; evidence actually collected; unresolved acceptance; next dependent slice. Close only after acceptance is supported. See [the completion plan](D:/WOW/PROJECT_B_COMPLETION_PLAN.md) for the mandatory full stock-client replacement release gate.

## Source gap list (2026-11-21, OpenCode swarm)

Method: read the issue, `AGENTS.md`, `docs/agents/sync.md`, issue 15, handoffs
`2026-11-05-autoswarm-waves.md` (Wave 3), `2026-10-28-oc-session-handoff.md`,
`2026-10-31-astra-hotpatch-to-oc.md`, `2026-11-09-modular-hotpatch-closure.md`,
`2026-11-21-oc-gap-19.md`, `2026-11-21-oc-gap-21.md`, `docs/opencode/route-asset-handoff.md`,
`docs/profiles/*.mpqlist`, and the owning source: `crates/benilla-assets/src/profile_source.rs`,
`crates/benilla-assets/src/world_assets/mod.rs`, `crates/benilla-profile/src/lib.rs`,
`crates/benilla-app/src/ui_script/{lifecycle,manifest,mod,addons}.rs`, `.../ui_saved.rs`,
`.../quest_trace.rs`, `.../asset_churn.rs`, `.../devtools/hot_reload.rs`,
`crates/benilla-world/src/assets/mod.rs`, `crates/benilla-lua/src/reload.rs`. No Cargo, test,
client or server run; no source changed.

| Acceptance line | Source that meets it | Gap |
|---|---|---|
| Revisioned staging path outside the read-only install; complete replacement before switching | `crates/benilla-assets/src/profile_source.rs:369` `stage_content_revision` (candidate opened and validated before `profile_source.rs:417` `commit_content_revision`); `crates/benilla-profile/src/lib.rs:146` `Profile::reload` candidate-first; the manifest is external (`crates/benilla-app/src/lib.rs:369`, `tools/launch-client.ps1:228`) | The candidate is in memory only (`StagedContentRevision`, `profile_source.rs:216`) and every archive resolves under the read-only `Data` dir (`benilla-profile/src/lib.rs:117` `self.data_dir.join(name)`); no revisioned staging path outside the install is read or written. Too large for one ticket; smallest slice: resolve `wm` archives from a user-owned staging dir before `Data`, test `a_wm_archive_stages_outside_the_install`. |
| Carry asset reload through the real GPU/dependency cache lifecycle | `crates/benilla-app/src/ui_script/lifecycle.rs:1356` swaps `WorldAssets.chain` only | `WorldAssets.textures`/`model_materials` (`world_assets/mod.rs:34,43`) and `SpriteCaches` (`world_assets/mod.rs:574`) are not cleared; the only clearer is `evict_world_art` on `MapChange` (`benilla-world/src/assets/mod.rs:23-33`), and `lifecycle.rs` emits no `AssetEvent::Modified`/`AssetServer` reload. Already-decoded GPU art and UI sprites keep the old revision, so archive reload alone is insufficient. |
| Existing UI/addon reload lifecycle; preserve declared state/SavedVariables | `lifecycle.rs:1260` `run_pending_reload`, `:1451` `save_live_ui_state`, `:1585` `read_saved_variables`; `ui_saved.rs:32`/`:102`; `addons.rs:855` `save_enable_state`; tests `world_entry_tests.rs:948` and `lifecycle.rs:2106` | none in source; the live `/reloadui` preservation is owner-run |
| Show the visible asset and actual Lua behavior change without restarting | `mod.rs:306` `console_reload_ui` -> `ReloadUiPending` (`lifecycle.rs:1243`) -> `run_pending_reload` (`:1260`) -> `load_ingame_ui_seated_with` (`:594`) -> `load_ingame_ui` (`manifest.rs:302`) | The asset half is blocked by the cache gap above; the Lua half is a live-client fact. `load_ingame_ui`'s failure list is discarded (`lifecycle.rs:789` `let _ =`), so a partial Lua load is not surfaced. Blocked: needs owner run. |
| Reject a corrupt/incompatible revision, keep prior content; prove rollback | `profile_source.rs:369`/`:542` `validate_input`/`:67` `ContentInputKind::check`; `:417`/`:527` `check_commit_base`; `lifecycle.rs:1428` `refuse_reload`/`:1423` `UiReloadFailure`; tests `profile_source.rs:735,755,878` and `lifecycle.rs:2106` | `RELOAD_CONTENT_INPUTS` (`lifecycle.rs:1414`) validates only two UI files, so a corrupt model/texture/DBC replacement is not validated; and after a successful commit the previous `Profile`/`Chain` is dropped and `load_ingame_ui`'s failures are discarded (`lifecycle.rs:789`), so a post-commit load failure has no rollback. |
| Record active revision and affected scope; no native hot-swap claim | `profile_source.rs:355` `revision_serial`, `:216`/`:265` `provenance`; `lifecycle.rs:1365` logs `ReloadUI adopted {provenance}`; `quest_trace.rs:127` `Stage::UiReload`/`:134` `UiReloadLoaded` | `revision_serial` is never recorded in the trace or any status surface (grep: only `profile_source.rs`); `Stage::UiReload` carries no content revision serial or affected-input list, so active revision and affected scope are only a log line. The no-native-swap half is met by construction (the reload touches assets/Lua; `benilla-hotpatch` is separate). |

### Tickets opened by this survey

1. `oc/reload-asset-cache` — On a successful content commit, evict the GPU/dependency caches so the reloaded asset is re-decoded. Files: `crates/benilla-app/src/ui_script/lifecycle.rs`, `crates/benilla-assets/src/world_assets/mod.rs`. Acceptance: after `run_pending_reload` commits a revision, `WorldAssets.textures` and `model_materials` are empty and the sprite caches hold no chain-opened entries; a `Handle<Image>` cached before the reload is not served after it. Tests: `a_reload_evicts_the_texture_and_material_caches`, `a_reload_drops_the_ui_sprite_caches`.
2. `oc/reload-rollback` — Restore the previous content revision when the post-commit UI load fails. Files: `crates/benilla-assets/src/profile_source.rs`, `crates/benilla-app/src/ui_script/lifecycle.rs`. Acceptance: a candidate that passes staging but whose `load_ingame_ui` reports failures leaves the active `Profile`/`Chain` and the live VM on the previous revision; the refusal is recorded in `UiReloadFailure`. Tests: `a_failed_load_after_commit_restores_the_previous_revision`, `a_successful_load_keeps_the_new_revision`.
3. `oc/reload-record-revision` — Record the committed revision serial and affected inputs in the reload trace. Files: `crates/benilla-app/src/quest_trace.rs`, `crates/benilla-app/src/ui_script/lifecycle.rs`. Acceptance: `Stage::UiReload` carries the committed revision serial and the affected input paths; the human summary prints them. Tests: `a_reload_records_the_content_revision_and_scope`.
4. `oc/reload-validate-affected-input` — Validate the reloaded asset input during staging instead of only the two fixed UI files. Files: `crates/benilla-app/src/ui_script/lifecycle.rs`, `crates/benilla-assets/src/profile_source.rs`. Acceptance: the reload request carries an affected-input list, `stage_content_revision` validates each with `ContentInputKind::check`, and a corrupt affected input refuses the reload before the boundary. Tests: `a_corrupt_affected_asset_refuses_the_reload`, `a_clean_affected_asset_stages`.
5. `Blocked: needs owner run` — "Show the visible asset and actual Lua behavior change without restarting the client": only a live client shows the reloaded pixels and the new Lua behavior; source tests cannot.
6. `Blocked: needs owner run` — "Use the existing UI/addon reload lifecycle and preserve declared state/SavedVariables behavior": `saved_variables_round_trip_through_a_reload` covers the host path, but a real `/reloadui` with the owner's addons and WTF state is a live-client fact.
7. `Blocked: needs owner run` — the ticket's own blocker 20: the selected Lua/UI file must load through the actual WotLK host; missing behavior is a live-client fact, not a source one.



