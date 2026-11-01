# Gemini / Antigravity continuation prompt — RustWrath

Copy the block below into Antigravity. Work in `D:\Projects\project-b-client`, branch `wotlk`.

---

You are continuing the RustWrath 3.3.5a client in `D:\Projects\project-b-client` (branch
`wotlk`). Read `AGENTS.md`, `docs/METHOD.md`, `docs/agents/active-ownership.md`, and this
handoff before touching code. Astra is out of use; take over her pending login/roster work.
Preserve the uncommitted work of other owners and allow only one Cargo build at a time.

## Current verified state

- HEAD: `6890d9d9` "Checkpoint the Wrath login, realm and character-select integration".
- Previous local commit: `13d5f2e4` "Keep Wrath realm categories visible without tournament
  support".
- `origin/wotlk` is one commit ahead with `0fcb1bea` "Add web preview applet configuration and
  structure", an unrelated web-preview commit. Do not blind-pull: it adds web files that are
  currently untracked in this checkout. Integrate deliberately or leave the branch local.
- Build passed: `cargo build -p benilla`, finished 2026-11-01 21:55:59, binary SHA-256
  `E10CF3C1A94EEE35D57F9BC78B5FB0F2551C01172FC99B1DCE945629931DE403`.
- Launcher: `Launch RustWrath.cmd` / `tools/launch-client.ps1`; latest verified launch log
  `benilla-config/Logs/Launcher/20261101-215612-105`.
- The stock 3.3.5a login screen is now clean: account/password fields and Login button visible,
  no modal Lua error. The old `Sound_OutputDriverIndex` modal is fixed by registering that CVar.
- `IsTournamentRealmCategory`, `IsStreamingTrial`, and `RealmListUpdateRate` were added; the
  realm wizard now loads and reaches character select.
- Earlier in-world run (binary `D6E7B8B3…`) reached Northshire with MainMenuBar/PaperDoll loaded,
  proving login and character selection work. That run still showed missing
  `showKeyring`, `GetCurrentTitle`, `GetSelectBackgroundModel`, `GetCVarMin`, and several
  3.3.5 DBC schema mismatches plus Wrath M2 skin parse errors.

## Immediate tasks

1. Launch with `Launch RustWrath.cmd`, log in manually with the dedicated account, and verify
   the realm list and character-select roster. Do not print or change credentials.
2. Fix remaining glue/build warnings one at a time, each with a focused test:
   - `GetSelectBackgroundModel` (`CharacterSelect.lua:520`) — return a valid background model or
     an explicit nil the caller tolerates; verify the character-select background.
   - `GetCVarMin` / `GetCVarMax` (`OptionsPanelTemplates.lua`) — expose the reference minimums
     for the CVars the options panels read.
   - Missing 3.3.5 CVars: `extShadowQuality`, `gxStereoConvergence`, `Sound_EnableSFX`,
     `showKeyring`, and any `GetCurrentTitle` dependency in `PaperDollFrame.lua`.
3. Repair the 3.3.5 DBC loader schemas now that `Cfg_Categories`, `ChrRaces`, `Factions`, and
   `LoadingScreens` fixes are in the working tree:
   - `LoadingScreens.dbc` schema has 3 fields, the 3.3.5 file has 4.
   - `TaxiNodes.dbc` schema has 16 fields, the file has 24.
   - `Exhaustion.dbc` schema has 15 fields, the file has 23.
   Use the pinned 12340 reference (`D:/Projects/wowdecomp-reference`, commit
   `e67b6ffef23655dc7da061cb2d36b7963f6ef71a`) and the install's real DBC headers.
4. Wrath M2 skin parse failures (`ui-autocastbutton.m2` and world `*_smoke.m2`) still load
   through the 1.12 skin path. Diagnose the 3.3.5 `.skin` header/version before changing the
   loader; preserve the vanilla path.

## Then verify playable combat

After character select, confirm in a real human session:
- action-bar click and `1`/`2` keys cast/attack;
- the Wrath `SMSG_SPELL_GO` success path clears the in-flight cast (no repeated "Another action
  is in progress");
- bags open by `B` and by the backpack button; loot rows accept clicks;
- target frame, unit tooltips, and roaming-creature positions match the attackable position.

Do not claim any of these from tests alone; record the rendered/connected evidence.

## Constraints

- Human-driven combat only. No bots, GM shortcuts, quest resets, automated attacks/kills,
  server/DB/WM/install changes, or credential changes.
- One Cargo build at a time; check `Get-Process cargo,rustc,benilla` first.
- Commit only files you own or have explicitly taken over; preserve other owners' dirty work.
- Keep focused tests (at most 20 essential tests per task), and record source, build, focused
  tests, and in-game evidence separately.
