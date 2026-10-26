# OC — native gameplay contract campaign (2026-10-26)

Base HEAD: `bf9aaf5e`. This claim covers protocol/session/reducer, native gameplay contracts,
command semantics and the server/mod authority boundary. Astra owns `benilla-lua`, `benilla-mlua`,
`benilla-ui`, app `ui_script`/`wrath_queries.rs`, UI-facing query adapters and `script_calls.rs`
additions; those paths are read-only here.

## Claimed paths

- `crates/benilla-protocol-wrath/src/gameplay.rs` — wire layouts/parsers/encoders.
- `crates/benilla-protocol-wrath/src/reducer.rs` — field/aura observation exposed to projections.
- `crates/benilla-protocol-wrath/src/session.rs` — self observation only where a native resource
  contract needs it.
- `crates/benilla-protocol-wrath/src/opcode.rs` — new opcodes.
- `crates/benilla-app/src/wrath.rs` — feed, view, translation, native projection wiring.
- `crates/benilla-app/src/wrath_native.rs` (new) — native cast/channel/resource/aura read model.
- `crates/benilla-app/src/lib.rs`, `crates/benilla-app/src/game_plugins.rs` — module/plugin lines.
- `crates/benilla-app/src/combat_mode.rs` — transport of the new native state into the mode view.
- `.scratch/project-b/checkpoints/2026-10-26-oc-*` — records.
- `D:/WOW/wm-project/src/wm_native/*` — only if the observation schema is extended; that tree is
  currently untracked (owner WIP), so edits stay uncommitted and are recorded.

Not touched: procgen, `docs/procgen/**`, `.scratch/project-b/compatibility/**`,
`docs/architecture/**`, the untracked `tools/patch_procgen_dungeon.py`.

## Immediate defects confirmed in source

- `SMSG_AREA_TRIGGER_MESSAGE` writer (`WorldSession.cpp:268`) is `u32 length` (includes the NUL)
  then the terminated line; the parser reads a C string from byte zero.
- `SMSG_QUERY_TIME_RESPONSE` (`QueryPackets.cpp`) writes `ServerTime` then `TimeResponse`; the
  parser reads only the first and the translation drops the second.
- `SMSG_PLAYED_TIME` carries a third `TriggerScriptEvent` byte; it is parsed but discarded at the
  shared-event boundary, so the native meaning is lost.
- `SMSG_SPELL_START`, `SMSG_SPELL_GO`, `SMSG_SPELL_FAILED_OTHER` and `SMSG_CAST_FAILED` carry
  `cast_count` (and START/GO the cast item); the parser discards it, so terminal edges cannot be
  matched to the cast they end. The stock `CastBarEdge::Stop` carries no identity.
- The native unit state keeps only self health/power and the aura translation only the vanilla
  1.12 aura fields; Wrath's seven power pairs, caster/duration/stack aura data are not exposed as
  one coherent native snapshot.

## Order

1. Parser/layout corrections with explicit malformed refusal; native clock/played-time retention.
2. Native cast/channel contract: identity, provenance, timing domains, terminal distinctions,
   scope/revision; wire into the view and a public read API for Astra.
3. Native unit resources (seven pairs, observed-zero vs unavailable) and full aura state.
4. Command gaps: trade-item cast, item/mail text, quest-share decline, item/equipment failures,
   action-button identity/range.
5. Scope/lifecycle integration for the native state.
6. Combat authority seam: implement only concrete client-side source; record exact missing
   server owners.

## Delivered source contract (for Astra)

`crates/benilla-app/src/wrath_native.rs` is the native contract module. It is a read model, not a
gameplay authority: events come from `WrathNativeUpdate` wire facts, the session IO thread pushes
them on their own bounded queue, and `wrath_net_drain` resolves identity against
`WrathWorldView` before publishing.

Read methods on `WrathWorldView`:

- `casting(unit) -> Option<&WrathActiveCast>`; `channeling(unit) -> Option<&WrathActiveChannel>`.
- `resources(guid) -> Option<WrathUnitResources>`: health/max plus all seven power current/max
  pairs. Each field is `Some` only when that word was delivered; `Some(0)` is an observed zero,
  `None` unavailable. Values are protocol units; display scaling belongs to the UI.
- `auras(guid) -> &[AuraSlot]`: native slot, spell, flags, level, charges, caster and both
  durations. Helpful/harmful and dispel class are not in the aura packet; they require the spell
  catalog and are therefore not inferred.
- `last_cast_end_in_scope()`: the last terminal edge only in the current session/world scope.
- `last_inventory_failure`: native reason, both item guids, bag type and the reason tail.

Bevy messages (registered by `WrathNetPlugin`):

- `MessageReader<WrathCastEvent>`: `scope`, `attempt_serial`, `identity`, `target`,
  `targets: Option<SpellTargets>` (the complete START block: unit/gameobject/item/source/dest,
  transport and string all retained), `observed_at_unix_ms`, `start_ui_now_seconds`,
  `ui_now_seconds`, and `kind` (`Started`, `Delayed`, `Launched`, `Failed`, `Interrupted`,
  `ChannelStarted`, `ChannelUpdated`, `ChannelEnded`, `ScopeInvalidated`).
- `MessageReader<WrathItemText>`: `scope`, `item_guid`, `has_text`, `text`.

Semantics that must not be invented in the UI:

- `attempt_serial` is this native model's stable per-attempt handle. It is not the wire
  `cast_count` and not a client-global line id; use it to pair one attempt's edges. `cast_count`
  remains the wire identity, and `cast_item` is the item provenance from START/GO.
- `start_remaining_ms` is START's `m_timer` (time left at packet write), not total cast time.
- `observed_at_unix_ms` is packet-arrival wall time; `server_time_ms` on `Launched` is the 12340
  game-time timestamp on GO. They are different clocks.
- `ui_now_seconds` is the app `GetTime()` value when this specific edge was drained; it is
  `None` when no UI clock was available, so a valid zero is never confused with unavailable.
- `start_ui_now_seconds` is the origin captured on START and never rewritten by pushback. Use it,
  not the delayed edge clock, in `start = start_ui_now_seconds - (total - start_remaining_ms)/1000`;
  the total comes from the spell catalog. Channel snapshots keep the same start anchor plus the
  latest update anchor, and their START/UPDATE/END events carry both. Neither wall time nor the
  wire timestamp is a `GetTime` value.
- `interruptible` is `None`: the wire does not establish it. Rank/display and the "not
  interruptible" bit require the spell catalog.
- `targets` carries the whole parsed target block; the scalar `target` is only the unit
  convenience field. A unit+destination or source+destination cast keeps every field.
- `cancel_requested` records a local cancel command, not a terminal result.
- A native cast without a matching START/GO identity is a wire-only observation; no cast id or
  start timestamp is synthesized.

Scope rules: all native state is keyed by `(session_epoch, world_generation)`; a transfer, clean
logout, replacement session or worker death invalidates active casts/channels and emits
`ScopeInvalidated`. `last_cast_end_in_scope` refuses a record from an older scope.

WM consequence: the native state lives in the app process. It is not yet in the WM observation
schema; the exact additional observation fields are the next dependency (self powers, self
cast/channel, self auras). That is recorded as a gap, not claimed as delivered.

Execution: source inspection/implementation/commits only. No builds, tests, new tests, linters,
formatters, interpreter/AST scripts or runtime operations.
