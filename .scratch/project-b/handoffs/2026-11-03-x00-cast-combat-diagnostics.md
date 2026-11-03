# X00 handoff — cast and combat diagnostics producer

Owner: Combat/UI (cast and combat call sites). Shared recorder/inspector/JSONL schema remain
Codex-owned; this change uses the existing `quest_trace::Source` / `Stage::Cast` interface and
adds only two `CastEdge` variants.

## Changed code

- `crates/benilla-app/src/quest_trace.rs`
  - `CastEdge`: added `Requested` and `LocalRefusal { reason: u8 }`. Existing edges and native
    outcomes are unchanged. Raw GUIDs/packet bodies/free-form messages are still excluded.
- `crates/benilla-app/src/spell/cast_send.rs`
  - `CastLadder` gains `trace: Option<Res<crate::quest_trace::Source>>`.
  - `send_spell_cast` gains `trace: Option<crate::quest_trace::Source>` and records
    `Stage::Cast { spell_id: Some(spell_id), attempt: None, edge: CastEdge::Requested }` at
    entry.
  - The `refuse!` macro records `Stage::Cast { ..., edge: CastEdge::LocalRefusal { reason } }`
    before reporting, using the existing `CastFail::reason`.
  - `CastLadder::report_local` records the same local-refusal boundary for targeting/item
    surfaces that refuse through the reporter.
  - `send_bound` forwards `self.trace.clone()`.

## Success path (source inspection)

1. UI calls `CastLadder::send_spell`/`send` → `send_bound` → `send_spell_cast`.
2. `send_spell_cast` records `CastEdge::Requested` with the spell id.
3. Ladder passes all local rungs (no `refuse!`).
4. `commands.0.send(ClientCommand::CastSpell { spell_id, target })` succeeds (channel admission).
5. `wrath_tap_commands` drains the tap, converts to `CMSG_CAST_SPELL`, records
   `Outbound { opcode, state: Queued }` (and `Encoded` on the receipt path), pushes to the Wrath
   outbox with `OutboxAdmitted`.
6. `run_session_with_cancel` drains the outbox, calls `world.send`, records
   `Outbound { opcode, state: Written }` (or `WriteFailed` + `SocketWriteFailed`).
7. On `SMSG_SPELL_START`, `wrath_native` mints an `attempt_serial`, emits
   `Stage::Cast { spell_id, attempt: Some(serial), edge: Started }`.
8. Instant casts surface as `CastEdge::Launched`; channels as `ChannelStarted/Updated/Ended`.

A socket write records transmission only; authoritative success still requires the native
`Started`/`Launched` edge.

## Refused path (source inspection)

1. `send_spell_cast` records `CastEdge::Requested`.
2. A rung fires `refuse!(CastFail::local(spell_id, reason))`.
3. The macro records `CastEdge::LocalRefusal { reason }` before `refuse()` reports the line.
4. No `ClientCommand` is sent; no outbox entry; no socket write.
5. The inspector therefore shows the cast stopped at local refusal with the reason code.

## Repeated casts / ambiguity

- Each `SMSG_SPELL_START` mints a new `attempt_serial` in `wrath_native`, so repeated casts of
  the same spell keep distinct attempt ids.
- `Stage::Cast` records from native drain carry `attempt: Some(serial)`; local records carry
  `attempt: None` and are correlated by scope/spell/time, preserving ambiguity when two
  attempts could explain one server event (no exact match is forced).

## Scope changes

- `native.invalidate_scope()` on cancel/world transfer clears native cast state.
- `run_session_with_cancel` rejects outbox entries whose session/world scope changed.
- Native `ScopeInvalidated` edges are recorded in `wrath.rs` drain.
- Open interactions in retired scopes are closed by the existing scope-retirement logic.

## Remaining integration dependency

- Codex owns the shared recorder correlation/inspector/JSONL schema. The new `CastEdge`
  variants are additive and serde-compatible; Codex may want to extend incident correlation
  and inspector summaries for `Requested`/`LocalRefusal` (not done here).
- Combat-mode experimental refusal (`CombatModeView`) is not yet routed through the recorder;
  it has no cast spell id. Left for a separate combat-mode diagnostics task.
- Local cancel (`inflight::local_self_cancel`) records only through the outbound `CancelCast`
  command; no dedicated cast edge is emitted for the cancel request itself.

## Source paths for three cases

### 1. Local refusal (e.g. not enough power, 0x4d)

- `crates/benilla-app/src/spell/cast_send.rs` `send_spell_cast`:
  - `cast_trace_interaction = trace.begin(scope, None, Operation::Observed)` mints the scoped
    identity (interaction id) before any guard runs.
  - records `Stage::Cast { spell_id, attempt: None, edge: Requested }`.
- On the refusing rung, `refuse!` records
  `Stage::Cast { spell_id, attempt: None, edge: LocalRefusal { reason } }` on the same
  interaction.
- Correlation stops: no `ClientCommand` is sent, so `wrath_tap_commands` and
  `run_session_with_cancel` produce no outbound/socket records and no native edge arrives.
  The interaction id is the only evidence; reason code is the terminal fact.

### 2. Instant cast (server replies with SMSG_SPELL_GO, no bar)

- Identity originates in `send_spell_cast` as above (`Requested`).
- `commands.0.send(ClientCommand::CastSpell { ... })` admits to the channel.
- `crates/benilla-app/src/wrath.rs` `wrath_tap_commands`: records
  `Outbound { opcode: CMSG_CAST_SPELL, state: Queued }` and
  `Stage::Cast { spell_id, attempt: Some(wire cast count), edge: Admitted }`.
- `run_session_with_cancel`: records `Outbound { state: Written }` (transmission only).
- Native drain in `wrath.rs`: on `SMSG_SPELL_GO`/launch, `wrath_native` emits
  `Stage::Cast { spell_id, attempt: Some(native attempt_serial), edge: Launched }`.
- Correlation stops at the id-space boundary: the local interaction id and the native
  attempt serial are different identifiers; they are correlated by scope + spell + time
  window, preserving ambiguity if two casts overlap.

### 3. Two rapid casts of the same spell

- Each request mints a distinct interaction id in `send_spell_cast` (`Requested`).
- Each reaches `wrath_tap_commands` with its own wire cast count (`Admitted`).
- Each server START mints a distinct `attempt_serial` in `wrath_native`.
- If one native outcome could be explained by either request, the recorder keeps both
  candidates (existing multi-candidate handling) instead of forcing an exact match.

## Shared recorder changes needed (for Codex)

1. `Operation` may gain a cast-specific variant (currently reusing `Operation::Observed`).
2. `Stage::Cast.attempt` already carries `Option<u64>`; local records use `None`,
   transport uses the wire cast count, native uses `attempt_serial`.
3. To carry the local interaction through transport, add
   `receipt: Option<CommandReceipt>` to the cast `ClientCommand` variants
   (`CastSpell`, `CastSpellCorpse`, `CastSpellAtDest`, `CastSpellAtSource`,
   `CastSpellGameObject`, `CastSpellItem`, `CastSpellTradeItem`, `UseItem`,
   `PetCastSpell`, `CancelCast`, `CancelChannelling`) and call `receipt.report(...)`
   in `wrath_tap_commands` and `run_session_with_cancel`, mirroring the quest path.
4. Correlation/inspector summaries should surface `Requested`/`LocalRefusal`/`Admitted`.

## Evidence level

- Source inspection only. No Cargo build, tests, or client run were performed (reserved for
  explicit authorization). Compilation of the added fields/parameters is inferred from
  existing patterns, not verified.
