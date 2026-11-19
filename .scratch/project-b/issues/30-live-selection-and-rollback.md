# 30 — Change WM selection A to B and roll back live

**Status:** ready-for-agent

**Plan reference:** C15. Owner approved ticket sizes and blockers on 2026-10-25. See [the approved completion queue](../completion-index.md).

**Implementation state:** Acceptance open — existing code may cover part of this slice; inspect the current checkpoint before starting.

**What to build:** WM selects revision B for a logical action while active revision A instances obey their pinning contract, then rolls the selection back without restarting the server.

**Blocked by:** [29 — Publish one immutable Spellwright revision through WM](29-typed-live-graph-publication.md) Existing WM/server evidence paths suffice; native observation delivery is not a prerequisite.

**Related existing issues:** [03](03-b001b1-live-rule-source-contract.md), [10](10-b001b2-live-change-proof.md). These are traceability links, not additional blocking edges; existing issues and evidence are preserved.

**Suggested owner:** OpenCode with lab owner execution scope.

## Acceptance

- [ ] Use a relation/behavior change between A and B, not only a numeric slider.
- [ ] Implement the WM-owned logical-action-to-owned-spell binding through existing selection owners.
- [ ] Capture separate evidence for A behavior, B behavior, actual WM selection and the real resulting action.
- [ ] Demonstrate invalid-B rejection with A usable and rollback with coherent active-instance policy.
- [ ] Record identical running server PID and binary identity across the routine A/B/rollback proof, plus content revisions and timestamps.
- [ ] Keep platform-level authoring/casting proof separate from WM-owned selection proof; no fabricated proof fields.

## Boundaries and handoff

A one-time prior deployment, if explicitly approved, does not waive the no-restart acceptance for the demonstrated live change itself.

Keep the slice within one fresh implementation context. If exploration reveals additional independent behavior, identify the smallest complete child slices before expanding scope. Read the current project context, relevant skill/rules and source checkpoint. Preserve unrelated edits and existing ownership.

Execution checks described above are acceptance requirements, not authorization. Follow explicit permissions applicable to the executing agent. Report source implementation, compilation and runtime evidence separately. An unexecuted required check remains UNKNOWN. Do not commit, deploy, start services or mutate live state merely because this published ticket exists.

Handoff: exact source identity and changed paths; externally visible behavior; evidence actually collected; unresolved acceptance; next dependent slice. Close only after acceptance is supported. See [the completion plan](D:/WOW/PROJECT_B_COMPLETION_PLAN.md) for the mandatory full stock-client replacement release gate.

## Source gap list (2026-11-21, OpenCode swarm)

Method: read the issue's six acceptance lines; `docs/agents/sync.md` for the note shape; handoffs
`2026-11-21-oc-gap-19..24`, `2026-11-20-oc-issues-25-27-gaps`, `2026-11-17-live-code-hotpatch`,
`2026-11-17-live-code-dx-workspace`; and the owning source on both sides of the seam —
`crates/benilla-assets/src/profile_source.rs`, `crates/benilla-app/src/ui_script/lifecycle.rs`,
`crates/benilla-app/src/wrath.rs`, `crates/benilla-app/src/multiclass.rs`,
`crates/benilla-app/src/ui_action/{mod,drain,feed}.rs`, `crates/benilla-ui/src/script/`
`{action,multiclass,addon_message,cursor/bar}.rs`, `crates/benilla-wm-native/src/`
`{lib,action,publisher,frame,health,observation}.rs`, `crates/benilla-protocol-wrath/src/`
`multiclass.rs`, `crates/benilla-buildstamp/src/lib.rs`, and the read-only 12340-side owners under
`D:/WOW/wm-project` (pinned `native_modules/mod-multiclass`, `native_modules/mod-wm-bridge`,
`src/wm/{spells,proofs,runtime,abilities}`) plus `docs/live-rules/` and
`docs/opencode/live-change-integration.md`. No Cargo, cargo fmt, clippy, test, client or server
run; no file outside this worktree was written.

| Acceptance line | Source that meets it | Gap |
|---|---|---|
| Use a relation/behavior change between A and B, not only a numeric slider | `docs/live-rules/graph-A.json:110` (effect 7, Mending) against `graph-B.json:111` (effect 18, Swiftness), both read; the validator that guards the line is `SpellmakingV3Graph.cpp:65` (percent effects above 95) | The relation difference exists only in two data files and prose. Nothing asserts it, and the invalid-B fixture (`docs/opencode/live-change-integration.md:126`) is not a file, so "relation change, not slider" is re-argued by hand each time. |
| Implement the WM-owned logical-action-to-owned-spell binding through existing selection owners | `crates/benilla-wm-native/src/action.rs:22` `ActionIntent` has no slot/binding variant; `Cast { spell_id }` at `:30` is the only spell intent and `crates/benilla-app/src/wrath.rs:3234` casts that raw id. The client's selection owners are `crates/benilla-ui/src/script/action.rs:41` `ActionBarCall` (`Set { id, packed }` at `:48`), written only from Lua at `crates/benilla-ui/src/script/cursor/bar.rs:138` and applied at `crates/benilla-app/src/ui_action/drain.rs:532`. Server side `SpellmakingV3.cpp:399` loads `wm_spellforge_design`; `wm_spellforge_binding` (`sql/characters/011_spellforge_v3.sql:8`) maps spell → node → carrier, not logical action → spell. | **none found.** No `logical_action` symbol exists under `crates/` or `D:/WOW/wm-project/src/wm`. A WM selection cannot be expressed on the wire (no intent variant), cannot be applied to a slot without Lua, and has no server mapping to resolve. The gap is two-sided: a small client intent + command, and a new WM binding owner. |
| Capture separate evidence for A behavior, B behavior, actual WM selection and the real resulting action | Per-action client evidence exists: `crates/benilla-wm-native/src/action.rs:226` `ActionEvidence` (attempt_serial, cast_count, terminal, correlation) and `observation.rs:454`/`:590` cast/channel `spell_id`, carried by `publisher.rs:27` `ActionBridge`. WM side `wm-project/src/wm/proofs/runner.py:19` `ProofPacket`, run at `:187`, status derived at `:266`. | The packets (`runner.py:32-162`) have no spell or selection kind — `runtime_startup, chat_action, scene, ambient, memory, content, rollback, failure, living_lane`. No packet can hold A pulses, B pulses, the WM selection id and the resulting action as four distinct fields, and no pulse key `(instance_id, node, event, scheduled_time)` exists in either tree. |
| Demonstrate invalid-B rejection with A usable and rollback with coherent active-instance policy | `SpellmakingV3.cpp:348-349` validates then resolves before any write, so a rejected B never reaches `Configure` (only the purchase callback `:329` calls it); `SpellmakingV3Graph.cpp:65` is the rejection. Rollback `DELETE` `:352` ends only instances whose `context.design->spell == id` with reason 13, `End` `:220` cascades children with 13, sweep `:299-300`, aura-remove `:380`, logout `:390`. | Source meets the policy; no caller drives it. `Command` is file-static with addon chat as its only entry (`:392`). The WM spell owners are `wm.spells.publish` (`publish.py:266`, direct SQL) and `wm.spells.rollback` (`rollback.py:70`, snapshot restore); neither knows Spellwright `DESIGN`/`DELETE`, and `live_publish.py:27` still ends "restart worldserver". Nothing asserts "A still castable" after a rejected B. |
| Record identical running server PID and binary identity across the routine A/B/rollback proof, plus content revisions and timestamps | `wm-project/src/wm/runtime/status.py:66` `collect_runtime_status` records `pid`, `started_at` (`:190`) and `command_hash` (`:195`); `proofs/runner.py:341` checks service state only. Client content revision: `profile_source.rs:355` `revision_serial`, staged/committed at `:369`/`:417`, provenance at `:276`; build identity `benilla-buildstamp/src/lib.rs:29` `emit()`. | `_command_hash` (`status.py:451`) is a SHA-1 of command lines, **not a binary hash**; no `binary_sha256` and no start token exist in the world-service record, so "same PID" is checkable but "same binary" is not. Separately the client never reports its content revision or build id: `frame.rs:22` `HelloFrame` carries protocol/schema/token only, and `observation.rs:894` `state_revision` is world state, not a content revision — that half is a protocol change and larger than one small slice. |
| Keep platform-level authoring/casting proof separate from WM-owned selection proof; no fabricated proof fields | `docs/opencode/live-change-integration.md:277-296` pins `"wm_publication_adapter": "missing"` and `"wm_logical_binding": "missing"`, and `:351` repeats both as gates; `runner.py:266` derives `player_visible_verified` from server status **and** a client observation rather than asserting it. | No machine-checked separation: a spell proof today would be filed as `chat_action` or `content`, and the platform/selection split lives only in prose. `ProofPacket` has no `tier` or selection field, so a tier (i) "actor-casts-B" claim and a tier (ii) `wm_owned_binding` claim are indistinguishable in a filed packet. |

### Tickets opened by this survey

1. `oc/wm-action-set-slot` — add `ActionIntent::SetActionSlot { slot: u8, spell_id: u32 }` beside `Cast` and map it to `ClientCommand::SetActionButton` in `wrath_wm_action_intake`, under the same scope and deadline refusals, answering `Unsupported` for a slot outside 0..144. Accept when: a set-slot intent reaches `ClientCommand::SetActionButton` with `packed = spell_id` in slot `slot`; a wrong-scope or past-deadline intent is answered `Rejected`/`Expired` and sends nothing. Files: `crates/benilla-wm-native/src/action.rs`, `crates/benilla-app/src/wrath.rs`. Tests (in `wrath.rs`): `set_slot_sends_the_packed_spell_and_answers_accepted`, `set_slot_out_of_range_is_rejected_without_a_send`.

2. `oc/wm-spell-selection-binding` — give WM a `logical_action_id -> purchased spell_id` mapping with bind/resolve/rollback, where rollback repoints the logical action to A and never deletes A. Accept when: `resolve(logical_action_id)` returns the B id after bind and the A id after rollback; an unknown logical action id is refused rather than defaulted; no path reaches a rollback by deleting a design. Files: `D:/WOW/wm-project/src/wm/spells/selection.py` (new) plus one `control/actions/*.json`. Tests: `D:/WOW/wm-project/tests/test_spell_selection.py` — `test_bind_then_resolve_returns_b`, `test_rollback_restores_a`, `test_unknown_logical_action_is_refused`. This ticket lands in the separate WM project, not this repository.

3. `oc/spell-live-change-proof-packet` — add a `spell_live_change` `ProofPacket` that keeps platform authoring/casting apart from WM selection: `tier` (`actor_casts_b`|`wm_owned_binding`), `spell_a`, `spell_b`, `wm_selection_id`, `resulting_spell_id`, `a_pulses`, `b_pulses`, and a `duplicate_check` over `(instance_id, node, event, scheduled_time)`; refuse a packet claiming `wm_owned_binding` while the binding is missing. Accept when: the packet records the four evidence groups as separate keys; a tier (i) packet is accepted with `wm_selection_id` null; a tier (ii) packet without a binding is refused with a named blocker. Files: `D:/WOW/wm-project/src/wm/proofs/runner.py`. Tests: `D:/WOW/wm-project/tests/test_proofs_spell_live_change.py` — `test_tier_i_packet_is_recorded_with_null_selection`, `test_tier_ii_without_binding_is_refused`, `test_duplicate_pulse_keys_fail_the_packet`.

4. `oc/runtime-world-binary-identity` — record a real binary identity for the world service so "same binary" is checkable: add `binary_path`, `binary_sha256` and `start_token` (`CreationDate`/`StartTime`) to the world record instead of `command_hash` alone. Accept when: the world record for a running `worldserver.exe` carries a non-empty `binary_sha256` and `start_token`; a changed hash or start token is reported as a new identity rather than the same one. Files: `D:/WOW/wm-project/src/wm/runtime/status.py`. Tests: `D:/WOW/wm-project/tests/test_runtime_status_identity.py` — `test_world_record_has_binary_hash_and_start_token`, `test_changed_binary_hash_is_a_new_identity`.

5. `oc/spellforge-graph-validation-tests` — commit the invalid-B fixture and a static validator mirror so "relation change, not slider" and "invalid B rejected, A untouched" are asserted rather than argued: `graph-B-invalid.json` (B with effect-18 magnitude 96) plus the percent/name/icon/root/child rules of `SpellmakingV3Graph.cpp`. Accept when: the mirror rejects `graph-B-invalid.json` at field `magnitude` and accepts `graph-A.json` and `graph-B.json`; A and B are asserted to differ in operation effect (7 against 18), not only in magnitude; the fixture SHA-256 values are pinned in the test. Files: `docs/live-rules/graph-B-invalid.json` (new), `D:/WOW/wm-project/tests/test_spellforge_graph_validation.py` (new).

### Blocked: needs owner run

- Row 3's "actual WM selection and the real resulting action": only a live client produces the cast/channel observation that proves the resulting action, and only the owner starts the client. The four evidence groups cannot be populated from source.
- Row 4's "invalid-B rejection with A usable" and "rollback with coherent active-instance policy": `Command` is reachable only from a connected player's addon chat (`SpellmakingV3.cpp:392`) and `DELETE` requires `!IsInCombat && !IsNonMeleeSpellCast` (`:352`), so the sequence needs the owner's lab character, Naram present and a live server. Source shows the policy; it does not show it happening.
- Row 5's "identical running server PID and binary identity across the routine proof": the PID is observable, but "identical binary" is unproven for any run today because no hash is recorded (ticket 4), and re-reading it across A→B→rollback is a live action.
- Rows 1 and 6 need no live run beyond the above; their gaps are source-side and are tickets 5 and 3.



