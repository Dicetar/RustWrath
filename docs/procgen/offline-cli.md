# Offline procedural content tool

Source entry point: `crates/benilla-procgen/src/bin/procgen-offline.rs`. This tool has not been compiled or invoked. The commands below describe its implemented interface for later authorized use; they are not a report of execution.

Invocation shape: `procgen-offline --workspace-root ROOT COMMAND INPUTS OUTPUT_NAME.json`.

| Command | Input JSON files, in order | Output |
|---|---|---|
| `generate` | request, supported build identity | owned validated candidate |
| `inspect` | candidate, supported build identity | counts/identity and data-consistency summary; runtime UNKNOWN |
| `plan` | candidate, supported build identity, placement request | verified encounter plan |
| `draft` | candidate, supported build identity, encounter plan, draft options | inert WM draft package |
| `receipt` | candidate, supported build identity, encounter plan, WM draft, original WM report | normalized application receipt |
| `cleanup` | candidate, supported build identity, encounter plan, WM draft, original WM report, application receipt | receipt-based inert cleanup package |
| `cleanup-receipt` | candidate, supported build identity, encounter plan, WM draft, original WM report, application receipt, original cleanup report, cleanup-run document | normalized per-object cleanup outcomes |

Request is the strict `artifact::Request` DTO: seed hex, item count, dungeon spec, terrain request with float bits, spawn spec. Build identity is `artifact::BuildIdentity`; it requires retained archive identity rather than inferring provenance from the current checkout. Placement and draft options are documented in [encounter-planning.md](encounter-planning.md) and [wm-drafts.md](wm-drafts.md). Cleanup-run JSON has the sole field `run_key`, distinct from the original spawn run.

Each input is capped at 64 MiB, including a check for growth while reading. At most eight inputs are accepted. The shared JSON structural and typed semantic limits apply. Every input and complete output encoding is processed before output directories/files are created. The CLI performs no WM import, subprocess, socket, SQL, game or server operation; saved reports are ordinary explicit file inputs.

The root must contain the Project B workspace and procgen crate manifests. Output is restricted to `ROOT/benilla-config/procgen/offline/OUTPUT_NAME.json`; there is no arbitrary output-path option. Existing directory components are canonicalized and must remain inside that root. Output names cannot contain path separators, parent traversal, alternate-stream syntax or reserved Windows device names. Files are create-new only, refusing overwrite and existing symlinks. Write failures attempt to remove only that newly created file. It is not a multi-file transaction and a process crash can leave a partial new file; strict decoding rejects truncated JSON. Use a fresh name and retain failed artifacts when diagnosing a crash. The explicit root is a workspace, never the game installation.

Cleanup results distinguish confirmed native removal, unknown, unattempted and preview. They retain unresolved spawn steps. `requires_reconciliation` remains true unless every known object is confirmed removed and no spawn effect remains unknown. Confirmation is a result reported by the existing owner, not an independent live query or permission to reuse a retired content ID. No command executes either nested WM bundle.

Prepared verification scenarios remain pending: argument/count/refusal paths; input growth and output bounds; directory junction/path/device-name refusal; create-new/write-failure behavior; complete generation-to-cleanup artifact chain; exact retained-report reconstruction; partial/ambiguous spawn and cleanup runs. No test files/cases were created and no execution checks were run.
