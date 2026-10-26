# OC — experimental combat authority seam (source audit)

Base `bf9aaf5e` plus the native gameplay contract commits. This file records the actual server
ownership points for the toggleable combat mode. It is not an implementation and does not claim
server capability.

## Current client state

`crates/benilla-app/src/combat_mode.rs` exposes `Baseline` as the only effective mode,
`Unavailable(ServerModeChannelMissing)` as availability, and answers an experimental request
`NotSent` without emitting traffic. It projects the native resources/cast/channel state; it does
not claim stamina, dodge, i-frames or server acceptance.

## Missing server implementation (exact owners)

1. **Mode negotiation and effective mode**
   - No combat-mode opcode exists in `src/server/game/Server/Protocol/Opcodes.h`; a module cannot
     register a `CMSG` opcode without changing `Opcodes.h` and `Opcodes.cpp`.
   - Missing: a `CMSG_COMBAT_MODE`/`SMSG_COMBAT_MODE` pair (requested mode, accepted/refused,
     reason, effective mode, revision), a session handler, and persistence/reconnect policy.
   - Decision required: core patch vs an installed module that is allowed to add a core opcode
     table entry. The module-only option is not sufficient by itself.

2. **Stamina ownership**
   - `Powers` (`src/server/shared/SharedDefines.h`) has mana/rage/focus/energy/happiness/rune/
     runic only; no stamina resource or player field exists.
   - Missing: one server-owned stamina state with authoritative spend/regeneration/death/reconnect
     rules and a defined client representation (custom state packet or a deliberately leased
     field). Reusing an existing power type would change unrelated class behavior.
   - Decision required: custom field vs state packet, and its persistence/DB policy.

3. **Hit-outcome construction (dodge/roll)**
   - Melee outcome is built in `Unit::CalculateMeleeDamage` (`Unit.cpp:1652`); miss/dodge is
     selected around `HITINFO_MISS` (`Unit.cpp:1777-1788`). Delivery follows in
     `Unit::SendAttackStateUpdate` (`Unit.cpp:6870`), before `Unit::DealMeleeDamage`
     (`Unit.cpp:1994`) and `Unit::ProcSkillsAndAuras` (`Unit.cpp:2815`, definition `6746`).
   - Direct spells resolve in `Spell::DoSpellHitOnUnit` (`Spell.cpp:2947`).
   - `sScriptMgr->ModifyMeleeDamage` (`Unit.cpp:1748`) runs after the hit result is chosen and
     cannot replace it with a dodge consistently across packets/procs/logging.
   - Missing: a pre-outcome hook that can force the dodge result at construction, plus a
     dodge-eligible source allowlist. A late damage sink is not acceptable.
   - Decision required: core patch at the outcome owners vs a new script hook owned by the module.

4. **Monotonic windows and action commitment**
   - Server uses `GameTime::GetGameTimeMS()` (`Spell.cpp:4826`, GO timestamp) and per-update
     ordering; no dodge/stamina window or movement-commitment state exists.
   - Missing: a server monotonic eligibility interval resolved at authoritative hit time (no
     rewind), duplicate/out-of-order rejection, and death/transfer/logout cleanup.
   - Decision required: where the interval is stored and how movement commitment is enforced
     without breaking ordinary root/movement authority.

5. **Telegraph and presentation source**
   - Current stock telegraphs are real `SMSG_SPELL_START`/visual traffic; there is no custom
     telegraph owner. The client must not invent one.
   - Missing: a server telegraph/cast source that the existing spell/visual owners can render.

## Required server contract (proposed, not enabled)

- `CombatModeState { session_epoch, world_generation, revision, requested, effective,
  availability, refusal_reason }`
- `StaminaState { current, max, revision }`
- `DodgeWindow { start_monotonic_ms, end_monotonic_ms }` with a single authoritative resolution
  point; no client-side invulnerability.
- `CombatActionResult { request_id, status, reason, revision }` for mode/dodge/commitment.
- Cleanup on death (`Unit::setDeathState`, `Player.cpp:11341`), logout
  (`WorldSession::LogoutPlayer`, `WorldSession.cpp:637`), transfer and reconnect.

## Acceptance (future, not run)

Only after the server owner lands the above: normal hit, eligible dodge, insufficient stamina,
before/after window, duplicate/out-of-order, interruption, death, transfer, disconnect and
reconnect all agree between the server and the client. Baseline mode must remain byte-for-byte
ordinary and send no experimental traffic.
