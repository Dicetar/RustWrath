# Live-change proof preparation (queue 02, D)

Updated: 2026-10-25. Status: **PREPARATION COMPLETE, RUNTIME PROOF PENDING**. This document is
source-observed preparation for B-001B-2 / ticket 10. No server, database, WM, client, or game was
started or mutated, and no proof field below is populated.

## Evidence boundary

- Source-observed here: Spellwright v3 command handling, expiry units, publication semantics,
  instance lifetime, and the validation rule used for an invalid B.
- Proposed here: the live-change sequence, observation method, evidence schema, and injected
  interfaces for a future proof runner.
- Not established here: that the authored graphs pass runtime `Parse`/`Validate`; that the lab
  character has the required ingredients, maker access, money, spell slot, or carriers; that B can
  be purchased or selected; or that any of the A/B pulse behaviour occurs on the running server.
- The source copies at `D:/WOW/wm-project/native_modules/mod-multiclass/src/` and
  `D:/WOW/WM_BridgeLab/src/modules/mod-multiclass/src/` were byte-identical for the inspected
  files: `SpellmakingV3.cpp` SHA-256
  `ac46b9babb4905f1ce9ccc8e0123d45b08a4449eee1c586237692713af3add80` (56,502 bytes) and
  `MulticlassBonusXp.cpp` SHA-256
  `2bd36cd6337a96563558a8f6b4cbf4e12568469082832f2ed035c10b3f9a561` (20,803 bytes). Identity of the
  deployed `worldserver.exe` to that source remains unproven.

## Correction: upload expiry is 30 seconds, not 300

The ticket-03 contract (`docs/live-rules/B-001B-1-contract.md` §3) says "300 s upload expiry". The
source says **30 s**. This is a unit error in the contract, not a runtime observation.

Source chain:

- `Multiclass::BonusXpNow()` returns
  `duration_cast<milliseconds>(steady_clock::now().time_since_epoch()).count()`
  (`MulticlassBonusXp.cpp:317-320`).
- `SpellmakingV3::Now()` delegates to `Multiclass::BonusXpNow()` (`SpellmakingV3.cpp:48`).
- `BEGIN` sets `uploadExpiry = Now() + 30000` (`SpellmakingV3.cpp:346`).
- Every accepted `CHUNK` resets `uploadExpiry = Now() + 30000` (`SpellmakingV3.cpp:347`).
- `FINISH` rejects when `Now() > uploadExpiry` (`SpellmakingV3.cpp:348`).

Therefore `30000` is 30,000 ms = **30 seconds**, and it is an inactivity deadline refreshed by each
accepted chunk, not a fixed total upload lifetime. A slow but continuously progressing upload can
outlive 30 seconds.

`quoteExpiry = Now() + 120000` (`SpellmakingV3.cpp:350`) is a fixed **120 seconds** from the quote
response. The contract's 120 s statement is correct.

Proposed contract correction, not applied because `docs/live-rules/` is outside this queue's edit
allowlist:

```diff
-  (S:114-115), 300 s upload expiry, 120 s quote expiry (S:346-350). Purchase flow is **two-step by design**:
+  (S:114-115), 30 s inactivity upload expiry refreshed per accepted CHUNK, 120 s quote expiry
+  (S:346-350). Purchase flow is **two-step by design**:
```

## What the source actually publishes

### Library save is separate from purchase

`SaveModule` inserts a module row, inserts a new revision, advances `current_revision`, then calls
`Book` (`SpellmakingV3.cpp:331-337`). It does **not** create a spell, allocate a cast slot/carrier,
call `Configure`, or compile a new design. The `SAVE` path validates/parses/resolves the graph first
(`:348-349`) but only persists the revision.

Consequence for the proof: saving B proves revision persistence and listing, not activation of B.
Do not describe a library save as a hot patch of an existing purchased spell.

### Purchase is a new immutable identity, applied immediately

`Purchase` is gated by a fresh nonce, maker proximity, ingredients, and money
(`SpellmakingV3.cpp:314-317`). It allocates a new cast slot and carriers (`:320-323`), inserts a new
design/binding transaction (`:326-328`), and on commit calls `Configure(d)` immediately, inserts the
design into the in-process compiled map, learns the spell, and emits `CREATED|id` (`:329`).

`Configure` rewires live `SpellInfo` and inserts the design under `designsMutex`
(`SpellmakingV3.cpp:168-191`). This is why a purchased definition is live without a restart.

Consequence for the proof: A and B must have **different purchased spell IDs**. B is a new
definition, not an update of A.

### Instances pin their design

`Context` holds `std::shared_ptr<Design> design` (`SpellmakingV3.cpp:57-64`), and `Instance` contains
a `Context` (`:69-79`). A running A instance therefore keeps A's graph while B is purchased and
compiled. This is the mechanism the proof must observe, not merely infer.

### No database-plus-reload route

Definitions are loaded from `wm_spellforge_design` only in `V3Library::OnStartup`
(`SpellmakingV3.cpp:397-399`). No runtime reload verb exists in this translation unit. The addon
dispatcher (`:339-354`) exposes `LIST`, `REVISION`, `BEGIN/FINISH`, `BUY`, and `DELETE`; it does not
expose a reload or same-ID update.

The DB-plus-reload route is therefore rejected unless a separate owner adds and proves a reload
path. An external DB write followed by a restart is not a live change.

## Route and selection gates

The existing command entry is file-static, and the only caller is addon chat from the same player
(`SpellmakingV3.cpp:339-354,392`). The two required WM adapters remain missing:

1. **Server-side publication entry.** Proposed `SpellmakingV3::SubmitCommand(Player*,
   std::string)` plus a `mod-wm-bridge` typed verb wrapping it. It must preserve the player online
   state, `AtMaker`, ingredient, money, upload, and two-step quote checks. A direct DB insert is not
   an acceptable substitute.
2. **WM-owned logical-action binding.** Proposed mapping `logical_action_id -> purchased spell_id`
   consulted by the existing grant/refresh path. Rollback would repoint the mapping to A. This does
   not exist; therefore WM automation cannot claim the "selected logical action" evidence.

The tier used in a proof must be recorded explicitly:

- **Tier (i), actor-casts-B:** the actor casts A or B directly. B is a normal purchased spell;
  rollback is `DELETE B`. This needs no new WM binding code.
- **Tier (ii), WM-owned binding:** blocked on gate 2. Not provable in this checkout.

Existing addon authoring is **not** proof of a WM publication adapter. A manual addon demo is
tier (i) at best, and only if the evidence labels it as operator-driven.

## Scenario and pinned artifacts

| Artifact | Identity |
|---|---|
| A | `Sentinels Vigil`, `docs/live-rules/graph-A.json` |
| A SHA-256 | `e4df64d4eb6b124ee270e28a78f973df9fcb700e01ebfa1b1d27e0b055e53a22` |
| B | `Sentinels Alacrity`, `docs/live-rules/graph-B.json` |
| B SHA-256 | `dcf6956d901e7c62e640f53a624d6ba7ad881acb8e8d89c3601317f7825af7b1` |
| Invalid B | B with effect-18 magnitude changed from `30` to `96`; expected rejection by `SpellmakingV3Graph.cpp:65` |

A and B share Ward, but the every-six-second reaction invokes a different operation/package
relation: Mending (effect 7) for A, Swiftness (effect 18) for B. This is a behavioural relation
change, not a display-value edit.

Runtime preconditions to verify before starting the proof:

- A already purchased and known by the actor.
- The actor knows ingredients for effects 12 and 7 (A) and 12, 7, and 18 (B)
  (`SpellmakingV3.cpp:123-124`).
- Naram is present and interactable; the actor is alive and out of combat at quote/purchase
  (`SpellmakingV3.cpp:125-126,317`).
- A free slot in the `mode=0,target=0` family and enough carriers (`SpellmakingV3.cpp:320-323`).
- Enough money for the quoted fee.
- A is castable before any invalid-B attempt and remains castable after it.

## Proof sequence

### Step 0: baseline identity and preflight

Record before touching the scenario:

- `worldserver.exe` path, SHA-256, PID, process start identity/time, and the binary's full path.
- `SpellmakingV3.cpp`, `SpellmakingV3Graph.cpp`, and `MulticlassBonusXp.cpp` SHA-256 values.
- Graph A and B SHA-256 values and B's relation difference.
- Actor GUID, A spell ID, map/zone, and relevant config (at minimum the declared map update
  interval).
- Preconditions above.

The baseline manifest records the 2026-10-07 deployment as
`WM_BridgeLab/run/bin/worldserver.exe`, SHA-256
`9cd24a0cca001bf04e4af7ea4faccf3b3825eabab2d98a824363def2c21ee0f8`. Re-read the artifact at proof
time; do not reuse that historical value, and do not use the stale `WM_BridgeLab/run/worldserver.exe`
duplicate.

A future runner must refuse to claim no-reboot evidence if any identity field is missing or if the
PID/start identity/binary hash changes. Capture the same identity fields again after B is purchased
and after rollback, so the A -> B -> rollback interval has more than an endpoint assertion.

### Step 1: save B while A is active

Use the existing `SAVE` upload path (or a future WM publication adapter that calls the same owning
path). Record:

- module ID, prior revision, new revision, `SAVED` response, and timestamp;
- confirmation that A's purchased design is unchanged and that no B spell ID exists yet.

Expected source-observed result: a new revision only (`SpellmakingV3.cpp:331-337`). Do not claim B
is active.

### Step 2: invalid B rejected with A usable

Submit the invalid-B fixture. Record the exact `ERROR|node|field|message` response, the fixture
hash, and proof that A is still castable. Expected source-observed rejection:
`SpellmakingV3Graph.cpp:65` for effect 18 at magnitude 96.

This step must not delete, replace, or disable A. An invalid-B rejection is not proof of rollback;
it is proof that validation did not publish a bad definition.

### Step 3: quote and purchase B

1. `FINISH` with purpose `QUOTE`, record the returned nonce and quote timestamp/expiry. The quote
   window is 120 s (`SpellmakingV3.cpp:350`).
2. `BUY|nonce`, record `CREATED|spell_id` and the exact purchased graph hash. B's spell ID must
   differ from A's, and the nonce must match the quote.

Record the two steps separately. A purchase without the matching nonce is not this proof. Do not
hide the fee, the maker interaction, or the slot/carrier allocation. Re-read the server process
identity after purchase.

The `CREATED|id` response does not contain the design body. Re-read B's graph through the existing
`LIST`/`SPELL` document path or the stored design row and hash that returned body; do not hash the
submitted JSON and assume the server stored the same bytes.

### Step 4: concurrent active-A / new-B behaviour

Start or keep an A instance running before B is activated, then:

- cast A and record A's instance ID, spell ID, start/end time, and observed pulses;
- cast B and record B's instance ID, spell ID, start/end time, and observed pulses;
- confirm the running A instance completes on A's original graph while B executes B's graph.

Do not derive this from the mere presence of two spell IDs. The evidence must identify the
instance's design/spell and the observed effect relation: A's periodic relation produces the
Mending package; B's produces the Swiftness package.

Because no probe is authorized in this preparation pass, the capture method is proposed, not
implemented: the owner must choose an existing approved server log/observation surface before the
run. The client observation bridge is additive perception and does not prove server-side spell
instances or outcomes.

### Step 5: selection and rollback

Record the tier.

- **Tier (i):** the actor selects/casts B for the logical action, then `DELETE B`, then casts A.
  Record `DELETE` response, confirmation that B's active instances ended with the source's reason
  13, `B` was unlearned, and A remains known/castable (`SpellmakingV3.cpp:352`).
- **Tier (ii):** do not claim. The WM-owned binding is missing. Record it as a gate dependency.

Rollback means **stop new B activation and select A again**. It does not mean deleting A, undoing
already completed damage/healing, or erasing database history. Re-read the server process identity
after rollback.

### Step 6: duplicate-pulse and timer check

For each instance, record a pulse key containing at least `(instance_id, reaction_node, event,
scheduled_time)`. Validate that keys are unique for the observation window. Source support for
deduplication is the per-instance `lastReaction` map and its interval guard
(`SpellmakingV3.cpp:78,213-214`).

Do not substitute "no visible duplicate" for timestamped event evidence. If only aggregate counts
are available, record the weaker evidence and keep the duplicate claim incomplete.

### Step 7: final identity and lifecycle limit

Re-read PID, process start identity/time, and binary SHA-256. All must equal Step 0. Record the
logout limit: logout ends all instances and clears the job queue
(`SpellmakingV3.cpp:390`); startup reloads definitions/bindings but does not rehydrate active
effects (`:397-399`).

## Evidence schema

Use one record per proof attempt. The top-level `status` stays `unproven` until the validation rules
pass, and every populated field must cite its observation source in `notes`. Never fill a proof field
from expectation.

```json
{
  "schema": "wm.live_change.proof.v1",
  "attempt_id": null,
  "status": "unproven",
  "source_identity": {
    "spellmaking_v3_sha256": "ac46b9babb4905f1ce9ccc8e0123d45b08a4449eee1c586237692713af3add80",
    "spellmaking_v3_graph_sha256": "dc963e06716009f19b1aaf34057ea4e5f74e23e26f8b7052642b09688596afca",
    "multiclass_bonus_xp_sha256": "2bd36cd6337a96563558a8f6b4cbf4e12568469082832f2ed035c10b3f9a561",
    "graph_a_sha256": "e4df64d4eb6b124ee270e28a78f973df9fcb700e01ebfa1b1d27e0b055e53a22",
    "graph_b_sha256": "dcf6956d901e7c62e640f53a624d6ba7ad881acb8e8d89c3601317f7825af7b1"
  },
  "server_identity": {
    "baseline":      {"binary_path": null, "pid": null, "start_token": null, "binary_sha256": null, "captured_at_unix_ms": null},
    "after_purchase":{"binary_path": null, "pid": null, "start_token": null, "binary_sha256": null, "captured_at_unix_ms": null},
    "after_rollback":{"binary_path": null, "pid": null, "start_token": null, "binary_sha256": null, "captured_at_unix_ms": null},
    "unchanged": null
  },
  "actor": {"player_guid": null, "spell_a": null, "spell_b": null},
  "point_1_2_platform": {
    "save_revision": {"module_id": null, "prior_revision": null, "new_revision": null, "response": null, "at_unix_ms": null},
    "invalid_b": {"fixture_sha256": null, "expected_error": null, "actual_response": null, "a_still_castable": null, "at_unix_ms": null},
    "quote": {"nonce": null, "expires_at_unix_ms": null, "at_unix_ms": null},
    "purchase_b": {"nonce": null, "spell_id": null, "graph_sha256": null, "response": null, "at_unix_ms": null},
    "a_instance": {"instance_id": null, "spell_id": null, "started_at_unix_ms": null, "ended_at_unix_ms": null, "pulse_keys": []},
    "b_instance": {"instance_id": null, "spell_id": null, "started_at_unix_ms": null, "ended_at_unix_ms": null, "pulse_keys": []}
  },
  "point_3_4_wm_selection": {
    "tier": null,
    "logical_action_id": null,
    "selected_spell_id": null,
    "rollback": {"deleted_spell_id": null, "b_instances_ended": null, "a_still_castable": null, "at_unix_ms": null},
    "wm_publication_adapter": "missing",
    "wm_logical_binding": "missing"
  },
  "duplicate_check": {"algorithm": "unique (instance_id,node,event,scheduled_time)", "result": null},
  "limits": {"logout_ends_instances": true, "active_effects_rehydrated_on_startup": false},
  "notes": []
}
```

### Validation rules

A proof is complete only when all of these are true:

- `server_identity.baseline`, `after_purchase`, and `after_rollback` exactly match on binary path,
  PID, start token, and binary hash; all three were captured on the same running process.
- `spell_a != spell_b`, both are non-zero purchased IDs, and B's purchased graph hash equals the
  pinned graph-B hash.
- `save_revision.new_revision > prior_revision` and the save response is `SAVED`.
- `invalid_b.actual_response` is a rejection at the expected graph-validation node/field, and
  `a_still_castable` is true.
- `purchase_b.nonce == quote.nonce`, purchase occurred after quote and before its expiry, and the
  response is `CREATED`.
- `a_instance.spell_id == spell_a`; `b_instance.spell_id == spell_b`; both have source-backed pulse
  evidence. A's instance may end after B is purchased; it must not be rewritten to B.
- Every pulse key in an instance is unique; `duplicate_check.result` is `pass`, or the duplicate
  claim remains incomplete.
- `tier` is one of `actor_casts_b` or `wm_owned_binding`. Tier `wm_owned_binding` is impossible
  until the WM binding exists; if it is claimed without that gate, the proof is invalid.
- Rollback records `DELETE B` and A remaining usable. It must not record deletion of A.
- No field is inferred from a configuration file, a static parse, or the presence of a spell ID.

## Injected interfaces for a later proof runner

No runner was added in this pass. The missing server dispatcher and WM binding mean a runner would
either immediately raise or fake progress; the task forbids a fake-success scaffold. When those
owners exist, the runner should depend on these narrow ports rather than on WM internals:

```python
class PublicationPort(Protocol):
    def save_module(self, graph: GraphArtifact) -> LibraryRevisionReceipt: ...
    def quote(self, graph: GraphArtifact) -> QuoteReceipt: ...
    def purchase(self, quote: QuoteReceipt) -> PurchaseReceipt: ...
    def delete(self, spell_id: int) -> DeleteReceipt: ...

class SelectionPort(Protocol):
    def select(self, logical_action_id: int, spell_id: int) -> SelectionReceipt: ...
    def rollback(self, logical_action_id: int, spell_id: int) -> SelectionReceipt: ...

class ProcessIdentityPort(Protocol):
    def snapshot(self) -> ServerProcessIdentity: ...

class EvidencePort(Protocol):
    def record(self, name: str, value: object, *, source: str) -> None: ...
```

`PublicationPort` must call the owning Spellwright path, not direct SQL. `SelectionPort` must be
the WM-owned binding, not an addon whisper or a bot client. `ProcessIdentityPort` must read the
actual running deployment (`WM_BridgeLab/run/bin/worldserver.exe`), not a stale duplicate.
`EvidencePort` must reject missing or invented fields rather than filling defaults.

## Gates and non-substitutions

| Claim | Required gate | Current status |
|---|---|---|
| B is published through WM's typed path | server dispatcher shim + bridge verb | missing |
| A logical action selects B | WM-owned `logical_action_id -> spell_id` binding | missing |
| B is live without restart | purchased through Spellwright, `Configure` observed | source-observed only |
| Definition changes via DB write | runtime reload verb | absent; rejected |
| No restart | PID/start identity/binary hash unchanged | unproven |
| Invalid B rejected | runtime graph validation response | source-observed only |
| No duplicate pulses | timestamped per-instance evidence | unproven |
| Active A completes on A | instance design/spell + effect evidence | source mechanism known; unproven live |

Do not substitute any of the following for the missing gates:

- an addon-only authoring session;
- a DB insert followed by a restart;
- a bot-client whisper masquerading as the author;
- deleting A to make B appear selected;
- a hidden purchase or a purchase without its matching quote;
- a client observation packet claiming server-side spell outcomes;
- "no visible duplicate" without timestamped event keys.

## Unexecuted validation

No graph was submitted to `Parse`/`Validate`; no quote, purchase, save, delete, cast, or selection
was run. No server process, database, WM control path, client, addon, or game was started. No PID,
binary hash, pulse key, or server response in the schema above has been observed. The proof remains
ticket 10's work under an explicit runtime authorization.
