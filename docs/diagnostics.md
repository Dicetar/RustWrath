# Client diagnostics

Run from the repository after reproducing a problem:

```powershell
powershell -ExecutionPolicy Bypass -File .\tools\collect-diagnostics.ps1
```

The collector reads a snapshot of the latest launcher's stdout/stderr without stopping
the client. It writes `benilla-config/Logs/Launcher/latest-diagnostics.md` and `.json`.
Use `-RunDir` to select a specific run and `-OutFile` to keep a separate report.
Each subsystem finding includes occurrence count and first/last log file and line.
One finding can belong to multiple subsystems; category totals are not additive.

It recognizes logged missing Lua functions/methods, CVars, UI load errors, asset and
geometry diagnostics, quest/gossip failures, loot/inventory failures, cast/action
failures, movement and connection problems. Logged rejections can be normal gameplay
(out of range, insufficient resources); guarded unsupported features and recovered
asset warnings need separate triage. No matching diagnostic means **unverified**.

For silent or visible bugs, create a local JSON observation file, for example
`benilla-config/Logs/Launcher/observations.json`:

```json
[
  {
    "category": "Quests",
    "description": "Firna: NPC name and quest ID; opened dialogue, clicked Accept, quest absent from log. Include time and reproduction steps.",
    "evidence": "path to screenshot"
  },
  {
    "category": "Geometry",
    "description": "Character stretches after talking; include location, camera angle and whether movement restores it.",
    "evidence": "path to screenshot"
  }
]
```

```powershell
powershell -ExecutionPolicy Bypass -File .\tools\collect-diagnostics.ps1 -ObservationsFile .\benilla-config\Logs\Launcher\observations.json
```

Observations are retained as player reports, not machine-verified failures. Evidence
paths are references; the collector does not inspect or copy screenshots. Do not put
credentials in observations. Review logs before sharing; reports retain log messages.

For gameplay acceptance, record the entire attempted route: quest open/accept/objective/
reward/turn-in; kill/loot/item appearing in bag; ability input/cast/outcome/cooldown;
UI opening/content/click response; and relog persistence when relevant. Include item,
quest, spell or NPC IDs where available. The collector does not currently correlate
requests to responses or detect silent timeouts, missing loot, deformed meshes or wrong
textures from pixels. Those require runtime instrumentation or human evidence; this
report must never be used as proof that the gameplay route passed.

## Resident interaction diagnostics

The resident recorder exports an allowlisted JSONL bundle from the debug panel. Coverage
is derived from the compiled vector registry, not an optimistic list: `hotpatch` is
listed as instrumented only when that feature is compiled, and vectors that are not
produced by source are listed under `not_yet_instrumented`. Every vector reports
`absence_not_proof: true`; zero retained observations is not a healthy reading.

Hotpatch rollback evidence is instrumented: a frame failure of the active module is
followed by either a bounded `RestoredPrevious` outcome (failed attempt, the restored
attempt when it is still known, and the restored module's verified content digest or
`unknown`) or an explicit `RestoreUnavailable` outcome meaning no retained predecessor
existed and the baseline pose is active. A restored module that fails again produces a
new bounded failure/restore outcome rather than one collapsed terminal. Free-form loader
error text, module paths and revision labels stay in the local log and are not exported;
a lost restore event is reported as an unverified gap, not a claimed success. Trust
policy and memory isolation remain out of scope: hotpatch modules are explicitly trusted
local native code, so coverage lists `hotpatch_trust_policy`, not rollback, as not yet
instrumented.

Per-vector producer drops are attributed through `vector_for_stage` at ingress time.
Retained-ring evictions are attributed through the same registry at the actual `pop_front`
eviction point; an event that cannot be classified is counted under `unclassified`, so the
per-vector retained evictions sum to the global `evicted` counter. The export and inspector
expose `producer_drops`, `retained_evictions`, `sampled_out`, `sampling_interval`,
`sampling_denominator`, `retained_observations`, `latest_retained_sequence`, and
`limiting_factors` (`producer_drop`, `retained_eviction`, `source_specific_loss`,
`sampling`, `uninstrumented`, plus `event_driven_reporting`). `retained_eviction` appears
only for a vector that actually lost retained observations, never merely because the global
eviction counter is nonzero. Coverage lists those vectors under
`paths_with_retained_evictions`.

Periodic summaries are bounded and suppressed when they carry no evidence. Asset windows are
emitted for a real failure or missed-event burst (coalesced to at most one window per second)
and otherwise only as a 30-second zero-value heartbeat, so a healthy asset vector still has a
denominator without flooding the ring. Frame intervals are aggregated over five seconds and
carry the measured `window_ms`; replica revision changes are coalesced to the latest revision
at most once per five seconds.
Scene, movement, audio and script observations remain change-driven. The retention summary
reports the retained sequence range, capacity, global loss counters, nonzero per-vector
evictions, and the cadence of these producers; zero retained observations is never proof
that no event occurred.

Object-update sampling reports seen/sampled/sampled-out counts with interval 64;
`sampling_denominator` is the total packets seen, so the rate is sampled/denominator.
movement reports coalesced/refused counts. An absent sampled packet is not evidence
that the server sent no update.

The inspector uses a cached, bounded snapshot refreshed at most once per 250 ms or
material ring change; it does not rescan the retained ring on every egui pass.
The snapshot and export also carry bounded cast/channel chains, loot/vendor/equipment
correlations, quest/UI dispatch correlations, login/world/reload transitions, movement
pressure and corrections, audio output health, asset failure windows, and hotpatch
attempts. These summaries are evidence pointers; they do not claim visible pixels,
audibility, movement feel, or a successful server outcome without the corresponding
authoritative observation.

Cast chains name the local action a native cast edge was attributed to. An exact or strong
match prints that action's non-zero local id; the native attempt serial and the wire cast
count are printed separately, so neither is confused with the action. Two rapid same-spell
casts stay `ambiguous` unless one authoritative wire count uniquely matches a local write;
newest-event adjacency is never used. A native edge with no retained local write is
`unverified` and prints `action unknown (local action not retained)`, never an exact action
and never interaction 0.

A rejected gameplay decode records the opcode, a closed decoder family and a closed coarse
reason (`Truncated`, `TrailingBytes`, `UnterminatedString`, `InvalidCount`, `InvalidValue`,
`Other`), plus an optional bounded undecoded-byte count that is a length only. The decoder's
free-form message and the packet body are not recorded, so `SMSG_QUESTGIVER_QUEST_DETAILS`
(`0x188`) identifies the decoder as `QuestDetails` and the reason as `TrailingBytes` without
exporting quest text, title text or any body bytes. The fields live in the incident's
`terminal_stage`, so they still identify the decoder and reason after the evidence rows are
evicted.

Export is user-initiated, runs off the frame thread, writes to a temporary file, and
renames with `create_new(true)` so an existing report is never overwritten. Unknown
schema versions are rejected by `validate_snapshot_header` before row decoding. Raw
GUIDs, credentials, chat, free-form strings, paths, packet bodies and coordinates are
not exported; NPC identity is a run-local numeric alias. Run ID and export/inspector
support are compiled only with the `dev` feature; coverage states this explicitly.

### Evidence classes and class-aware retention

Every retained row belongs to exactly one closed `EvidenceClass`, independent of the
diagnostic vector that owns it. The mapping is explicit in
`quest_trace::evidence_class_for_stage`:

| Class | Stages | Intent |
|---|---|---|
| action | intent, guard, admission, encoded, outbox, delivery, outbound; cast; quest panel/slot/journal/turn-in; inventory, loot, vendor and equipment; session, world, login and hotpatch transitions; loss annotations; script errors, load errors and warnings; reducer/gameplay rejections; socket failures | protected action evidence |
| health | frame window, asset failure/window, scene summary, replica revision, movement pressure, audio mixer/health, UI frame visibility | periodic, reclaimed first |
| protocol | packet receipt, reducer acceptance, gameplay decode | protected decode-adjacent evidence |
| ui_input | `UiPress`/`UiClick` pointer traffic | bounded UI input; reclaimed before core evidence is touched |
| unclassified | fallback only | counted, never inferred from a vector |

`ScriptError`, `ScriptLoadError` and `ScriptWarning` are action evidence: a script failure
can explain a UI action, and the Lua poll is change-driven rather than periodic, so these
rows are not treated as disposable churn. A reducer or gameplay rejection is action
evidence too, because it explains why an action did not land; successful decode steps stay
protocol. The mapping is total over the closed `Stage` enum, so current producers cannot
emit unclassified rows. The class and its counters remain so a future stage must choose a
class explicitly instead of silently becoming disposable.

Retention keeps at most 256 `health` rows and 256 `ui_input` rows and reclaims the oldest
row of the incoming class first while the 1024-row ring is under pressure. 64 health rows
and 32 UI input rows are protected. Health rows above the reserve are evicted first,
within their own budget, and counted; an incoming health row reclaims an older health
row rather than an action row. UI input rows above the reserve are evicted before core
evidence; an incoming UI input row reclaims an older UI input row, then (above the
health reserve) an older health row, and is refused with an explicit counter rather than
displacing protected core action/protocol evidence. An incoming action/protocol/unclassified
row reclaims health above its reserve, then UI input above its reserve, then the oldest
core action/protocol row. Rows keep their global sequence order for every reader. The
store is a sequence-keyed ordered map plus one per-class sequence index; each insert and
eviction is bounded (`O(log 1024)`), with no per-event scan of the ring and exact
per-class membership.

Per-class `retained`, `evicted` and `dropped` counts are exported in
`coverage.evidence_classes`, `coverage.evidence_class_totals`, and the top-level
`evidence_class_retention` / `evidence_class_totals` header keys. `dropped` is the sum of
the classified ingress losses, so it always equals the per-class sum including
`unclassified`; the totals also report whether `retained + evicted == received` and
whether the class sums equal the global counters. Health-only eviction is visible loss but
does not by itself mark an action conclusion as a telemetry gap; eviction of action,
protocol or unclassified rows does, and `unclassified` loss is never hidden.

### Incident evidence notes after eviction

Incidents keep the existing bounded `evidence` sequence references. They also carry up to
eight self-contained `evidence_notes`, each `{seq, elapsed_ms, class, stage}`, resolved
from the live row when the incident is created or updated; no packet body, text, path or
GUID is copied. When a chain overflows, the opening row and the most recent rows are kept.
`evidence_notes_truncated` marks that older notes were dropped, and
`evidence_truncated` still marks a bounded sequence chain. `missing_event_sequences` still
lists references no longer in the ring, but the opening and terminal notes remain readable
because they were captured before eviction.

The inspector's incident line shows the opening and terminal note for the latest incident
and says `opening note unavailable`, `terminal note unavailable`, or `notes truncated`
rather than implying a row is present. An opening note may be `recovered from start metadata`
when the original row was already evicted but bounded start metadata survived; it then uses
the stored local boundary time and says so. The saved report uses the same incident record, so
these fields survive export unchanged. All additions are additive to schema
`rustwrath.interaction-diagnostics.v1` version 1: existing readers can ignore the new
header and incident fields, and `validate_snapshot_header` is unchanged.

### UI click outcomes

Every retained press or click resolves to one of five closed verdicts. The verdict is derived
only from the bounded witness captured at the input edge, never from a later action, a button
animation, a toast, an empty panel or the absence of anything.

| Verdict | What it establishes |
|---|---|
| `no hit` | No frame took the point. When a rect-containing frame was excluded, the line names why: `its scroll clip excluded the point` or `it has no resolvable clip owner (layout gap)`, and the printed owner and bounds then describe that excluded candidate, not a hit. Hit insets, a disabled mouse bit and a nameplate veto are by design or by veto and are never reported as a clipped button. A fully hidden frame is absent from the hit walk, so it is indistinguishable from empty space. |
| `a frame took the point but has no recognised category` | The hit is real but its coarse frame category is unknown; the row cannot name what was clicked further than that. |
| `no handler bound for this button edge` | A named frame took the point and no Lua handler, script hook or engine click action exists for the edge that fires `OnClick`. For a `Button`, that edge follows `RegisterForClicks`; a release on an `…ButtonUp` registration is not the same edge as a press on an `…ButtonDown` one. This verdict needs the slot's own `HandlerMissing`; a slot that ran with no recorded gate and no run prints `click not run on this edge: no gate recorded` instead, because a target without a run-local frame id cannot answer the presence question. The hyperlink route fires `OnHyperlinkClick` instead of `OnClick` and also reads as no `OnClick` handler, which is a known engine path rather than a fault. |
| `handler present, not run: <gate>` | A handler exists but the click slot stopped at the first named gate: title region, not pushed, not registered for this button edge, cursor outside the pressed frame, drag, capture lost, frame destroyed, layout drag, world hook, or no press target. `not pushed` means the button was not PUSHED at the release (disabled, hidden, never pressed, or the press was cleared) and is not the same claim as `disabled`. The same gate on an edge that has no handler of its own prints `click not run on this edge: <gate>` instead, so a release row of a `…ButtonDown` registration says the press edge fires the click rather than claiming no handler is bound. |
| `handler ran` | A Lua handler ran, or `engine click action ran (no Lua handler)` for the LootButton take. A `handler error caught locally (exact)` records an mlua error raised inside that edge's own script fire; it is the only exact handler-fault evidence this recorder keeps. |

Attachments are listed after the verdict and never upgrade it. `host-attributed link` names the
action whose `Subject.ui_dispatch_id` is this click's id, with its last retained milestone. That
id is one eligible candidate chosen when the intent was minted, so the link is a heuristic
attribution, not an identity carried from the UI; a keybound cast or a world-interact open can
inherit an unrelated nearby click's id, and the line still claims only the attribution. A typed
server refusal keeps the weaker of the link confidence and the incident confidence, so a strong
refusal can never make a weak click link look exact. A script-error row is polled from the VM's
diagnostic log once per second and keyed by the error message, so a row is message identity, not
handler identity: a row first retained after a click is a possible association, and a row already
retained is a repeat of a message.
Nothing printed here establishes an error or an action as the click's effect, that a panel or
button was rendered, that the user intended that control, or that the server received what the
client sent.

Missing evidence is never a negative result. `absence, not proof` means the window expired with
nothing retained. `unverified ... pointer rows were lost` means the bounded UI-input class evicted
the pointer row (see `ui_input_rows_lost`), while the click-to-action link can still survive on
the action's carried id. A fully hidden frame, a release outside the window (which produces no
row at all), and an engine click action with no Lua handler are all reported as what they are,
not as a nil-handler fault.

The inspector prints up to eight click-outcome lines. The saved report exports the same strings
as `ui_click_outcomes`, beside the existing `ui_click_correlations`, and adds the boolean
`ui_input_rows_lost`. Both are additive to `rustwrath.interaction-diagnostics.v1` version 1:
existing keys and types are unchanged, `validate_snapshot_header` is unchanged, and readers that
ignore unknown keys are unaffected. The existing row fields keep their meaning; `visible` remains
a capture-time hit fact (`null` on a miss), and the post-handler observation is the witness's own
`visible_after`/`alive_after`.
