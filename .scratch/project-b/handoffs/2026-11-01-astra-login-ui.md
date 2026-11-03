# Astra: interactive Wrath session integration

Baseline: `wotlk`, `b77afde931558f2e0b3b7f815f3a5cdf94d3b9b6`.
Owner requested pull/rebuild during integration: HEAD is now origin `511369d7`.
Local edits were stashed, restored and combined. Only the ownership document conflicted;
the new unified per-ticket workflow was retained. Safety stash remains until integration lands.
Owner clarified that “forking workflow” meant the working stock 3.3.5a workflow; no character cloning.
Multiclassing is deferred until login/roster/create/delete/settings work.

## Ownership and source

Astra owns this task's `wrath.rs`, glue session/VM bridge, protocol auth/roster seams,
action dispatch repairs and launcher. OC continues combat; preserve these pending edits.
No server, DB, installation or identity-file changes. No character created/deleted by the agent.

- Review repairs: nil prefix/suffix attributes, absent refund data returns no values,
  explicit Wrath action GUID carried through spell/item/attack dispatch.
- Incoming bag-helper compilation repairs: QuestLogState.entries, Model.bag, and
  separate template lookup from the mutable miss queue.
- `WOW_WRATH_LOGIN_UI=1` opens Login without environment credentials. Existing direct
  identity/named-character probes retain their own path.
- Existing LoginSubmit/RealmChoice/CharPick requests now reach the Wrath worker.
  Generation-fenced cancellation, world AUTH_OK admission, queue updates, realm list
  refresh/switch, and bounded sockets precede roster entry.
- Character create/delete stay on the parked connection, use server result codes translated
  into the existing glue manager, and refresh the roster after confirmed success.
  Stock confirmation remains the UI's responsibility; no direct destructive probe.
- Wrath GetCharacterInfo returns ten values, including class filename; vanilla keeps eight.
  Client expansion is 3; account expansion comes from the authenticated world response.
- `Launch RustWrath.cmd` / `tools/launch-client.ps1` open the interactive player path.
  No probe credentials, commands, automatic combat or probe timeout are inherited.
  Launcher log retention is three generations. Instructions: `docs/launching.md`.

## Reference

Read-only reference `D:/Projects/wowdecomp-reference`, pin
`e67b6ffef23655dc7da061cb2d36b7963f6ef71a`:
CSimpleFrame_Scripts::Script_GetAttribute `0x0049F2D0`, ActionBarFrame::UseAction
`0x005ABBC0`, GlueScriptEvents_Scripts::DefaultServerLogin `0x004DC260`,
GetClientExpansionLevel `0x004DCBF0`, GetAccountExpansionLevel `0x004DDC20`,
CharacterSelection_Scripts::DeleteCharacter `0x004E33A0`.
CharacterCreation_Scripts contains HoT modifications; use the local matching AzerothCore
CharacterHandler.cpp and SharedDefines.h for actual 12340 packets/results. No decompilation copied.
Installed locale-enus GlueXML corroborates the ten-value roster tuple and client expansion 3.

## Verification so far

One focused UI review-contract test passed (`astra-review-contracts.log`).
Launcher PowerShell parser and git diff whitespace check passed.
Intermediate hotpatch builds passed. After pulling origin and repairing its compilation
mismatches, `cargo build -p benilla --features hotpatch` passed in 10m42s on the integrated
`511369d7` plus local changes (`astra-origin-511369d7-integrated-build.log`).
The build slot is released; no client was launched. Source changes remain uncommitted.
Executable SHA-256: `C37DA31FBA98EB0607849DAD28949A0B956035C5F335F41B99EB64E40C90CD1E`.
Rendered login and human account/create/delete/settings acceptance are not yet demonstrated.
Do not call this the completed playable alpha or a verified complete login workflow.

## Origin integration compile repairs

`511369d7` did not compile as pulled. Corrected its nonexistent gameplay decoder call to
the existing `parse` Option<Result> interface, public MonsterMove/Facing type paths,
NameCache/GameObjectTemplates resource paths, and the missing wow_to_bevy import.
The incoming CreateSpline construction used fields/arguments absent from the donor type.
Translated Wrath flags into the actual flying/cyclic/run-mode booleans (AC MoveSplineFlag.h:
Flying 0x2000, Cyclic 0x80000; movement WalkMode 0x100), and made the existing spline
constructor crate-visible for the Wrath adapter. No movement repair is claimed live-verified.
Current rebuild log: `astra-origin-511369d7-integrated-build.log` in the existing alpha evidence folder.

## Resumed after OC stopped: reported glue screens

Owner screenshots: missing IsStreamingTrial / GetSelectBackgroundModel / RealmListUpdateRate,
nil SetPreferredInfo input, black scene and green panel borders. OC's pending two bindings
are preserved; five-second refresh matches legacy SRP (12340 RealmList_Scripts, 0x004DE450).
Streaming-trial mode is false because this host only mounts full local MPQs, independently
of the account expansion. Account expansion getter no longer overwrites the server-fed value.

Read-only profile probe `benilla-assets/examples/glue_profile.rs` established:
- Selected patch-enus-k Glue-Tooltip-Border has valid 128x16 base pixels but bad mip 5.
  UI base-image consumers now use the same valid-prefix recovery as world art; strict
  blp_to_rgba still reports corruption. No archive or texture was modified.
- Cfg_Categories has 21 fields, not 11. Reader now uses the file's own layout, including
  16 localized names at column 4. 12340 CRealmList::Initialize keeps all rows; LocaleMask
  is not the donor Region column. The real profile now resolves named categories.
- Login Legion and Human scene cameras both decoded zero before the repair. 12340
  M2Data.h places cameras/lookup at 0x110/0x118 with 0x64-byte cameras, versus donor
  0x124/0x12c and 0x7c. Both selected models now decode one nonzero authored camera.
- GetSelectBackgroundModel uses the resolved DBC scene token: DK class, Dwarf for gnomes,
  Orc for trolls and invalid selection, otherwise the race. Native 0x004E3620.

Realm refresh with no live parked transport now answers failure instead of leaving the UI
waiting forever. Remember the selected realm for a clean logout's parked-roster reconnect;
the lifecycle coordinator admits that reconnect only after old-session retirement.

Focused checks: three camera cases and one Wrath glue-contract case passed. Selected-profile
decoder/provenance probe passed with the expected strict tail error retained. Source build
and same-screen rendered acceptance are pending (`astra-glue-screen-build.log`).
Cargo verbose output confirms the installed C:/Users/Qraag/.cargo/bin/sccache.exe wrapper
is invoked; no cache-hit or speedup claim follows from the zero-hit counters observed so far.
No automated credentials, combat, character creation/deletion, server or install writes.

## Screenshot follow-up, 2026-11-01

Owner screenshots show the repaired backdrop/borders now render, but login options,
roster selection, creation, and world reveal remain broken. Runtime baseline:
`benilla-config/Logs/Launcher/20261101-204232-720/stderr.log`.

Confirmed causes and source repairs in progress:
- Glue loaded before registration of WRATH_UI_CVARS. Register before its first OnLoad;
  retain real rows/saved values. Add referenced sound/window/stereo defaults and Wrath
  CVar bound getters (shadow maximum remains unavailable, not an invented capability).
- SetLighting stops on absent Model:SetGlow / ModelFFX ResetLights/Add* methods.
  Implement checked methods and six retained light sets plus a real glow override.
  Feed effects to the preview and custom scene lights to its GPU lighting buffer.
  Character/pet still share the existing stage buffer; independent custom rig consumption
  is not complete. Do not claim full ModelFFX lighting parity.
- Creation catalog is empty because Faction.dbc has 57 fields, not donor 37.
  Add its parent modifiers/caps and 16-locale blocks, plus 20-field FactionGroup.
  Fix Wrath race/class triples, six-value selected class, sex enum, faction index and
  global class selection; preserve donor paths. Expansion comes from the real DBC rows.
- Wrath emits EnteredWorld after self creation/position. The loading screen incorrectly
  awaited a future Worldport that is never sent: assets were all resident, player settled,
  warm pass/UI ready, but awaiting_snap stayed true indefinitely. Do not await that extra
  snap on this Wrath edge; retain all actual residency/settling/UI/warm gates.
- LoadingScreens.dbc has four fields (HasWideScreen appended); retain its actual art path.

References: CSimpleModel_Scripts SetGlow, CSimpleModelFFX::SetGlow 0x004E6160,
CSimpleModelFFX_Scripts ResetLights 0x004E6BE0 and Add*; creation scripts 0x004E0830,
0x004E0A60, 0x004E1880, 0x004E19A0; FactionRec 57 fields; LoadingScreensRec::Read
0x008B2B20. Selected GlueParent.lua lines 344-402 and CharacterCreate.lua are in the
ignored read-only profile probe log, not copied into shipped code.

Open: RequestRealmSplitInfo is absent and still errors in roster OnShow; do not replace
the network operation with a success stub. Custom creation XML also references missing
art/templates. A new build and rendered acceptance are pending. The prior check found
four integer-width errors; the concurrent merchant owner mechanically repaired those
and the ModelEffects export. Preserve those fixes. No fresh client launched yet.
