# OpenCode checkpoint - WM observation and player-action system (claim)

Date: 2026-10-26
Owner: OpenCode (client/protocol/bridge integration and WM consumer/action implementation)
Base HEAD (Project B): `5a6eaaa4`
Base HEAD (WM repo): `011a7b3` with the owner's existing dirty set preserved; this session edits
only the files listed below and commits them with explicit paths.
Work: `.scratch/project-b/work-packages/2026-10-26-system-goals.md`, OpenCode section 1-7.

## File claim (Project B)

- `crates/benilla-wm-native/src/observation.rs`
- `crates/benilla-wm-native/src/producer.rs`
- `crates/benilla-wm-native/src/frame.rs`
- `crates/benilla-wm-native/src/client.rs`
- `crates/benilla-wm-native/src/publisher.rs`
- `crates/benilla-wm-native/src/lib.rs`
- `crates/benilla-wm-native/src/error.rs` (only if a new typed error is required)
- `crates/benilla-protocol-wrath/src/session.rs`
- `crates/benilla-app/src/wrath.rs`
- `crates/benilla-wm-driver/src/main.rs`
- `.scratch/project-b/checkpoints/2026-10-26-oc-wm-system.md` (final system checkpoint)

## File claim (WM repo)

- `src/wm_native/constants.py`
- `src/wm_native/models.py`
- `src/wm_native/context_adapter.py`
- `src/wm_native/protocol.py`
- `src/wm_native/receiver.py`
- `src/wm_native/runtime.py`
- `src/wm_native/store.py`
- `src/wm_native/actions.py` (new; the client-seam action broker)
- `src/wm_native/__init__.py`
- `src/wm_native/__main__.py`

Not claimed: Astra's `crates/benilla-procgen/**`, `docs/procgen/**`,
`.scratch/project-b/compatibility/**`; the WM repo's existing dirty files
(`bootstrap/sources.lock.json`, `control/`, `data/`, `docs/`, `native_modules/`, `scripts/`,
`src/wm/autoplay`, `src/wm/content`, `src/wm/context`, `src/wm/spells`), and
`tools/patch_procgen_dungeon.py`.

## Plan of record

1. Observation completeness: schema v2.2 adds sourced `combat`, `resources` and `death` to the
   existing v2.1 transport and WM context, computed from the session's replicated self entry.
2. Action path: extend the same loopback transport with typed WM intent frames and typed result
   frames; the client adapter maps intents onto the existing `ClientCommand` owners and tracks
   authoritative outcomes from the existing session-event dispatch; the WM receiver hosts a
   bounded, idempotent, deadline/scope-checked action broker and feeds results to status/context.
   Move/stop is reported unsupported if no command owner exists for remote movement (the existing
   owner is the keyboard/camera input controller); this is an explicit refusal, not silence.
3. Lifecycle/diagnostics: retire pending actions on scope/echelon changes, surface observation
   age/scope, ACK, queue pressure and action status through the existing health owners, and write
   the single system checkpoint.

No builds, tests, test files, linters, formatters, imports, or processes are authorized by this
claim. Everything below is source integration only.
