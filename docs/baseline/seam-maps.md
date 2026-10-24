# B-001A — Extension seam maps (movement / building / generated spaces)

2026-10-24. One page per feature: existing owners, what the server actually does, missing hooks, restart needs. All file:line source-observed this session unless [TV]. Companion facts: server DataDir = `D:\WOW\Azerothcore_WoTLK_Rebuild\run\data` → Repack (has `maps/ vmaps/ mmaps/ dbc/` — mmap meshes precomputed offline); full native verb registry = `wm-project/src/wm/sources/native_bridge/action_kinds.py` (94 verbs).

## 1. Movement

| Aspect | Owner / evidence | Status |
|---|---|---|
| Input path | `MSG_MOVE_*` (all `PROCESS_THREADSAFE`, `Opcodes.cpp:312-333+`) → `WorldSession::HandleMovementOpcodes` (`MovementHandler.cpp:344`) → `ReadMovementInfo` (WorldSession) → `HandleMoverRelocation` (`:412`) | exists |
| Server validation | **No anticheat module in this tree; no speed/teleport-distance checks found in the movement path** — server largely trusts client movement opcodes (transport handling is the main branch in `HandleMoverRelocation`) | key finding: custom locomotion is unblocked structurally |
| Granted movement | 16 forced-mechanism sites: `SMSG_FORCE_*` speed/root/unroot, `MSG_MOVE_KNOCK_BACK` (`:838`), feather-fall/water-walk/hover compound (`SMSG_MULTIPLE_MOVES`), teleport acks (`:276`, `:47`) | exists |
| Speed change today | verb `player_set_speed` **declared, not implemented**; Spellwright op effect 18 (MOD_INCREASE_SPEED aura) is expressible **hot** today | dash = hot via Spellwright; roll displacement = knockback opcode or trusted plain movement |
| Missing hook | none for baseline locomotion; souls-authoritative stamina/state = combat contract (ticket 13), not a movement hook | — |
| Restart needs today | implementing `player_set_speed` = native module edit (rebuild+restart, one-time); movement-validation changes = core (avoid; none needed) | low |

## 2. Building

| Aspect | Owner / evidence | Status |
|---|---|---|
| Placement (session) | verbs `gameobject_spawn/despawn/set_state`, `creature_spawn/...` (bridge files `wm_bridge_gameobject_actions.cpp`, `wm_bridge_creature_actions.cpp`); hot via action bus | exists, varying readiness [TV per-verb] |
| Placement (persistent) | `world_spawn_create/update/delete` — persistent world editing verbs; capability map (WM-T01) records source **pending build/deploy** | near — one deploy away |
| Server collision | **dynamic GOs are NOT in server collision/LOS** — `MapCollisionData.cpp` has no gameobject path; vmaps cover static map geometry only. Placed mesh = client-side model collision only | quantified: "a placed mesh is not a functioning wall" |
| Navigation | mmaps precomputed offline (Repack `mmaps/`); no runtime mesh update — creatures/playerbots do **not** path around placed pieces | quantified |
| Missing hooks | (1) server collision for dynamic GOs (vmap extension — core, restart-class) or explicit product decision "decorative-only for v1"; (2) runtime/local mmap update — none exists; (3) structural-support system — new owner (later work package) | named |
| Restart needs today | spawn verbs: hot. GO collision / mmap regen: offline tooling + data deploy (maintenance-class, not per-change) | medium |

## 3. Generated spaces

| Aspect | Owner / evidence | Status |
|---|---|---|
| Instance identity | `MapInstanced::CreateInstanceForPlayer` (`MapMgr.cpp:118`) | exists |
| Layout on existing maps | spawn verbs + Python bundle runner (`wm-project/src/wm/world/bundle.py`, ordered publications/actions/waits) — **hot today**, deterministic encounter layouts on a known navigation surface are reachable with zero server changes | exists (synchronous only) |
| Lifecycle | persistent triggered/resumable event scheduler = documented gap (capability map WM-T11) — WM-side implementation, hot | named gap |
| New geometry | client ADT/WMO assets (fork-side, ticket 05 lineage) + server `maps/vmaps/mmaps` regeneration (offline extractor) + Map.dbc entries → maintenance-window deploy; stock clients excluded from custom geometry (fork-only) | restart-class by nature |
| Missing hooks | runtime "instance template" (spawns+triggers+lifecycle bound to an instance id) on existing maps — the WM-T11 lane; no core hook required for it | WM-side |

## Cross-cutting verdict

- **Movement extensibility: effectively open now** (trusted opcodes + granted-mechanism inventory + hot Spellwright speed).
- **Building: session-level placement open now; server collision & navigation are the two real walls**, both maintenance-class rather than per-change.
- **Generated spaces: existing-map layouts open now (hot); new geometry is a deliberate later product with offline pipeline costs.**
- The seam structure confirms the plan's staging: nothing in movement/building/spaces requires touching AzerothCore core to start.
