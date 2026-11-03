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
