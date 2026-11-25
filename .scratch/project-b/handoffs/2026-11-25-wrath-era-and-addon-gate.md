# Wrath is the client: session default, 3.3.5a addon gate, stock UI load

Status: implementation slice on `wrath/addon-era`; needs the owner's launch
Owner: DeepSeek V4.1 Flash (opencode-go)
Branch: wrath/addon-era (from main `4601fd6a1`)
Next: capture the 3.3.5a `_G`/event oracle, then run the addon corpus

## Why

The library's era switch still defaulted to the donor 1.12 client; only `WOW_WRATH=1` selected the
3.3.5a session. The addon gate was also the 1.12 `IsAddOnLoadable` law even when the VM target was
no `INCOMPATIBLE` floor, the demand rule inverted, and dependencies checked with the
caller's demand flag.

## What changed

- **The boot selects Wrath.** `benilla-client` sets `WOW_WRATH=1` unless `WOW_DONOR_112=1`, so
  the client is the 3.3.5a session with no launcher switch. The library keeps the donor default so
  its fixtures do not silently change era.
- **The gate is the 3.3.5a law** (`UIUtil/AddOns.cpp` `IsAddOnLoadable` `0x005F7E90`): an
  interface below 20000 is `INCOMPATIBLE` (hard, `checkAddonVersion` does not override), 30300 is
  exact, required dependencies recurse with the demand flag set, and a LoadOnDemand addon refuses a
  normal load with `DEMAND_LOADED` and loads through `LoadAddOn`. The donor law survives only for a
  11200 client (fixtures).
- **The API shapes split by target.** `GetAddOnInfo` returns the Wrath `enabled, loaded, reason`
  (the donor's fifth is loadable); `IsAddOnLoaded` returns `loading, loaded` on Wrath and one flag
  on the donor.
- **The layer is 30300.** `assets/ui/layer.toc` and the Wrath stock UI now agree on the interface.
- **The stock 3.3.5a UI loads clean.** Three real gaps surfaced and are fixed:
  - handler chunks get the `self` and `event` globals (the Wrath FrameXML still reads them);
  - `FillLocalizedClassList` is seated before `Constants.lua`, so the file no longer aborts at
    its class block and every later constant exists;
  - `GetScreenResolutions` has the current entry, so `UIParent.lua`'s menu-bar offset math is
    numeric.
  The test VM also seeds a race (`DressUpFrame`), the Wrath CVars and the Constants stand-ins.
- **Addon discovery is live on the install.** With Wrath active, `roots()` scans
  `<install>/Interface/AddOns` and `benilla-config/AddOns`; `WOW_ADDONS_PATH` is now an
  authoritative corpus override (only that folder, and the chain's third-party rows are filtered
  out; the `Blizzard_*` rows stay).
- **Test triage.** Donor-only addon/layer tests are gated on `donor_data_or_skip!` or
  `donor_layer_off()`; the layer-order test forces `WOW_WRATH=1` and exercises the Wrath layer
  (the bottle) on the 3.3.5a chain.

## Evidence

- `cargo test -p benilla-ui --lib`: 3,557 passed.
- `cargo test -p benilla-app --lib addons`: 76 passed.
- `cargo test -p benilla-app --lib ui_script::layer_tests`: 16 passed.
- `cargo test -p benilla-souls`: 33 passed.
- `cargo check -p benilla-client`: passes.
- The full `benilla-app` lib suite still shows the pre-existing donor-data failures on a 3.3.5a
  `WOW_DATA` install (tooltip/spell/world-map tests that need a 1.12 chain); they are not from
  this slice and need the `donor_data_or_skip!` sweep.

## Next

1. Capture the 3.3.5a `_G` and event args (the harness still compares against
   `reference/lua_globals.tsv`, a 1.12 capture).
2. Run `addon_harness` with `WOW_WRATH=1` on the install's corpus and rank the gaps.
3. 3.3.5a addon messages (`RegisterAddonMessagePrefix` + the addon opcode), secure templates,
   and the `GetAddOnMetadata`/SavedVariables edge cases the corpus hits.

## Addon corpus survey (2026-11-25, after the era slice)

The harness now hosts the 3.3.5a contract: `harness_target()`, the stock 3.3.5a UI, the
localized class catalog before `Constants.lua`, and the Wrath CVar list. `--surface` dumps
31,909 names and refuses to apply the 1.12 oracle; `tools/wrath-globals-capture` (a stock-client
addon) plus `tools/import-globals-capture.ps1` produce `reference/lua_globals_30300.tsv` for
the real diff. `WOW_ADDONS_PATH` remains the authoritative corpus override.

Survey of the install's `Interface\AddOns` (49 addons with manifests):

- loaded without a single load error: **39/49**
- first session-start errors: 37 `Couldn't find CVar named ...` + 1 invalid inventory slot
  → after the fixes below, **zero** first session-start errors
- drew something: 7/49; survived being used: 4/49
- missing-global counts: 29 addons at 0, 7 at 1–2, 6 at 3–5, 5 at 6–15, 2 at 16+

Fixes this run forced:

- the stock options panel's combat-text/unit-name CVars (`fctComboPoints`, `fctCombatState`,
  `fctDamageTaken`, `fctDodgeParryMiss`, `fctEnergyGains`, `fctFriendlyHealing`,
  `fctHonorGains`, `fctPeriodicEnergyGains`, `fctReputationGains`, `fctXP`, the six
  `UnitName*Name` toggles) registered in `WRATH_UI_CVARS`;
- `GetCurrentRegion` (1) and `GetMinimapShape` ("ROUND") as 3.3.5a natives;
- `GetAddOnMetadata` answers nil for a missing/non-string key on a Wrath VM instead of raising;
- the shared inventory-slot reader and `GetInventoryItemCount` read a non-number as Lua's
  `tonumber` 0 (Bagnon's keyring pass is `GetInventoryItem(..., false)`), so it resolves to
  the ammo slot rather than raising.

Remaining corpus blockers, in order: the stock templates `ActionBarButtonSpellActivationAlert`,
`InsetFrameTemplate`, `DialogBorderOpaqueTemplate` and the `BackdropTemplate` shim; the widget
methods `Texture:SetHorizTile/SetVertTile`, `FontString:SetWordWrap`, `ColorSelect:GetColorAlpha`
and `EditBox:SetCursorPosition`; `GetAuctionItemClasses` needs the app's twelve-class push in the
harness (the production client installs it); Questie's HBD `GetCurrentPlayerPosition`; Bagnon's
`GetDummyBag` (its format line reads `select(12, GetAuctionItemClasses())`, empty in the harness).
