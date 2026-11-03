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

Export is user-initiated, runs off the frame thread, writes to a temporary file, and
renames with `create_new(true)` so an existing report is never overwritten. Unknown
schema versions are rejected by `validate_snapshot_header` before row decoding. Raw
GUIDs, credentials, chat, free-form strings, paths, packet bodies and coordinates are
not exported; NPC identity is a run-local numeric alias. Run ID and export/inspector
support are compiled only with the `dev` feature; coverage states this explicitly.
