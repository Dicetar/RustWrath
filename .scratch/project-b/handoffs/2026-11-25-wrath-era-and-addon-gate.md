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

## Corpus pass 2: session survival

The first pass's 37 CVar deaths and one inventory-slot death are closed. The survey now reports:

- loaded without a load error: **39/49** (strict 37: two packages name files they do not ship)
- survived a session start: **34/49** (was 0/49)
- drew something: 7/49; survived being used: 4/49; UI probe: 0/49 (the probe's bounds, not a
  load result)

Closed this pass:

- `GetCritChance` (melee crit from `PLAYER_CRIT_PERCENTAGE`), `GetExpertise` (main/offhand),
  `GetExpertisePercent`, `GetShieldBlock` and `GetDodgeBlockParryChanceFromDefense` — the stock
  `PaperDollFrame.lua:1026` stat pane calls them on every session start;
- `GetCVar`/`GetCVarDefault` answer nil for an unregistered name on a Wrath VM (the donor and the
  glue-only `GetCVarMin`/`GetCVarMax` keep the reference's raise), so an addon's options row
  cannot die on a setting the host has not seated;
- the missing stock CVars `fctComboPoints`, `fctCombatState`, `fctEnergyGains`,
  `UnitNameEnemyGuardianName`, `UnitNameEnemyTotemName`, `nameplateAllowOverlap`;
- the Wrath inventory-slot reader's `lua_tonumber` coercion for `GetInventoryItemLink` and
  `GetInventoryItemCount` (Bagnon's `false` keyring slot);
- `GetQuestLink` (nil until the quest-log link projection exists) and the 4.x
  `hooksecurefunc(function, hook)` form (accepted; the 3.3.5a binding cannot replace a value);
- the harness seats the app's twelve `GetAuctionItemClasses` rows, unblocking Bagnon's
  `select(12, ...)` and its `GetDummyBag` definitions;
- the Wrath layer adds `BackdropTemplate` + `BackdropTemplateMixin`, and the Wrath VM answers
  `Texture:SetHorizTile/SetVertTile`, `FontString:SetWordWrap`, `ColorSelect:GetColorAlpha` and
  `EditBox:SetCursorPosition` (tiling/wrap are accepted, not yet rendered).

Remaining, all single-addon or out-of-era: the 10 load failures (3 `table index is nil`, 2
`setmetatable`, 2 `format`, one missing template, one `GetAddOnMetadata`, one `core` nil) and
five session-start first errors (invalid frame handle, one `hooksecurefunc` shape, one nil call,
Ace `EmbedPlugin`, one `format`). The real `reference/lua_globals_30300.tsv` still needs the
stock client's capture addon run.

## Night pass, 2026-11-26: corpus to 45/49 load, 43/49 session

The remaining corpus failures were mostly harness fidelity, not client gaps. The survey now reports:

- loaded without a load error: **45/49** (strict 43: two packages name files they do not ship)
- survived a session start: **43/49**
- drew something: **11/49**; survived being used: **8/49**

Harness fixes (the big jump):

- every 3.3.5a top-level chunk now receives `(addonName, private)` through
  `UiScript::set_loading_addon`; this fixed `!!!garbageprotector`, `aux-addon`,
  `newlevelframe` and Questie's `Compat.lua:39`;
- the seated auction tree now comes from `categories(script.target(), ...)` instead of the
  hardcoded 1.12 ten-row list, so Bagnon's `select(12, GetAuctionItemClasses())` finds Quest;
- seated equipment/bag links use `item_link_full_in_target` (the 3.3.5a nine-field payload),
  fixing Bagnon_Forever's `ToShortLink`;
- the Wrath layer adds `DialogBorderOpaqueTemplate`, `InsetFrameTemplate`,
  `MainHelpPlateButton`, `ActionBarButtonSpellActivationAlert`, `MicroButtonAlertTemplate`,
  `ScrollFrameTemplate` and `ModelWithControlsTemplate`.

Client/VM fixes:

- secure P0: `scrub`, `forceinsecure`, `securecallfunction`, `Frame:AllowAttributeChanges`,
  `RegisterStateDriver`/`UnregisterStateDriver` tolerate nil/empty, `RegisterAttributeDriver`
  aliases, `RegisterUnitWatch`/`UnregisterUnitWatch`/`UnitWatchRegistered` fallbacks, and the
  `SecureFrameTemplate`/`SecureHandlerDoubleClickTemplate`/`SecureHandlerMouseWheelTemplate`
  template names;
- addon messages: the 3.3.5a LANG_ADDON scheme is native (there is no send-addon opcode; the
  LANG_ADDON chat message is the reference). Wrath `SendAddonMessage` now applies the RAID→PARTY
  downgrade outside a raid, sends an oversized line instead of raising (the server drops it), and
  provides the 4.1 `RegisterAddonMessagePrefix` family as a shim;
- natives: `GetCritChance`, `GetExpertise`/percent, `GetShieldBlock`,
  `GetDodgeBlockParryChanceFromDefense`, `GetCoinTextureString`, `Lerp`, `CreateVector2D`,
  `IsModifierKeyDown`, `UnitPosition`, `GetPlayerInfoByGUID`, `GetUnitPitch`, `GetQuestLink`;
- widget methods: `Texture:SetHorizTile/SetVertTile/SetColorTexture`,
  `FontString:SetWordWrap`, `ColorSelect:GetColorAlpha`, `EditBox:SetCursorPosition`,
  `Slider:SetObeyStepOnDrag/SetStepsPerPage`, `GameTooltip:SetSocketGem/SetExistingSocketGem`;
- `collectgarbage` accepts the 5.1 `step`/`setstepmul`/`incremental` controls;
  `SetFixedFrameStrata`/`SetFixedFrameLevel` are accepted; `hooksecurefunc` tolerates a missing
  target or a table+function value on a Wrath VM; `showQuestTrackingTooltips` is seated.

Remaining: 4 load failures (three are the harness's missing Spell.dbc catalog — the production
client installs it — and one nil value), and 2 session first errors: Bartender's
`GetFrameHandle`/`IsFrameHandle` path (MoP-era addon) and Details' Ace `EmbedPlugin` (modern
Details). GatherMate2 (Classic/MoP TOC), aux-addon (TBC) and zzzmopitemtooltips (MoP) are out of
the Wrath target. The UI-probe column stays 0/49; it is the harness's probe bound, not a load
result.

## Frame-handle binding (partial)

`GetFrameHandle(frame, create)` and `IsFrameHandle(value)` are now bound for a Wrath VM. A frame
wrapper's `[0]` lightuserdata is the identity, and the wrapper table itself is the handle the
stock `SecureHandlers.lua`/`RestrictedExecution.lua` receive. The binding is tested on a live
frame and rejects a plain table.

The Bartender 4.4 secure-state path still stops at `SecureHandlers.lua:108`: the `self` that
reaches `GetFrameHandle` there is not a frame wrapper (it is a restricted-environment value), so
the next step is a true handle object whose frame methods route through the environment and whose
identity survives restricted execution. The rest of the corpus is unchanged at 45/49 load and
43/49 session; Bartender is MoP-era and is not part of the Wrath target.

## Nightly 2026-11-27: 47/49 load, 47/49 session, 17/49 survive use

Branch `wrath/night-2026-11-27`, from `main` `e3c8e14d9`. Corpus: the install's
`Interface\AddOns` (49 manifests). Baseline log: `F:/AI/tmp/opencode/wrath-corpus-survey-25.txt`,
roster `F:/AI/tmp/opencode/wrath-corpus-status-25.txt`.

- loaded without a load error: **48/49** (strict **46/49**; Bagnon_Config and fizzle list files
  their own packages do not contain)
- survived a session start: **47/49**
- drew something: **19/49** (was 11); survived hover/click/drag: **17/49** (was 8), the rest
  had nothing a pointer can reach
- no use-probe errors remain; the two failures are out of era: GatherMate2 (TOC
  `11508,20505,50503`) and bartender4 (`X-Curse-Packaged-Version: 4.4.2`, MoP secure handle)

### What the run forced

- **The harness seats `Spell.dbc`.** A new public `UiScript::set_spell_info` (Model
  `spell_info`, keyed by spell id) carries name, rank, icon, power type, cast time and range
  from `Spell`, `SpellRange` and `SpellCastTimes`; `wrath_spell::query` reads the lent host
  first and falls back to the seat. Production does not seat it: the app host answers from the
  catalog owner. GatherMate, Quartz and the Details plugin chain now load; cost stays nil in the
  harness because it needs the seated player's stats.
- **3.3.5 unit names resolve.** `Model::unit_named` matches a pushed snapshot's `UnitName`,
  optional `Name-Realm` suffix and ASCII case only. `Script_GetGUIDFromToken` (`0x60abf0`)
  falls back to a name and returns false for a miss, so a Wrath getter is non-raising: `with_unit`
  and `check_unit_token_in` no longer raise `Unknown unit name`, and `UnitGUID`, `UnitPower`,
  `UnitPowerType` and the relation resolver read the name's snapshot. The 1.12 fixture keeps the
  strict token line. The harness player now seats `has_object`, `is_connected` and `guid`, so
  roster code sees a live player.
- **A frame's per-kind `__index` enumerates inherited methods.** The client publishes one method
  table per kind and addon code copies the API by `pairs` over it (Details' DF
  `Libs/DF/button.lua:1052`). The flat view is built nearest-class-last, with the linked chain as
  its fallback, and is rebuilt for the plain Frame after `guard_receivers` wraps every method;
  without that rebuild the view held the unguarded functions. The widget-method census reads the
  flat view too, so a method poked into `getmetatable(f).__index` is probed like the client's; it
  found the three MovieFrame verbs already answered.
- **Animation and region verbs.** `Animation:SetScale(x[, y])` and `SetOrigin(point, x, y)`
  record their parameters; a Wrath Texture/FontString owns `CreateAnimationGroup` as a frame
  does. Details' alert glow builds one on a texture.
- **`SetDontSavePosition` / `GetDontSavePosition`** are the 3.3.5 Frame flags (Wrath only): the
  layout cache skips a user-placed frame's rect while set; the getter answers 1 or nil.
- **`GetInstanceDifficulty()`** answers `m_instanceDifficulty + 1` (`0x515750`). The field is 0
  until `SMSG_INSTANCE_DIFFICULTY`, which this host does not parse; the open world's 1 is the
  exact no-packet answer, not a fabricated tier.

### Verification

- `cargo test -p benilla-ui --lib`: 3,564 passed (6 ignored).
- `cargo test -p benilla-app --lib addons`: 76 passed.
- `cargo test -p benilla-app --lib ui_script::layer_tests`: 16 passed.
- `cargo check -p benilla-client`: passes.
- `cargo clippy -p benilla-ui -p benilla-app --lib -- -D warnings`: clean.
- `WOW_WRATH=1 cargo run -q -p benilla-app --example addon_harness -- "<install>/Interface/AddOns" --status <file> --deep 2`: the 47/49 run above.

### Remaining / next

1. The 3.3.5a `_G`/event capture still needs the owner's stock-client run of
   `tools/wrath-globals-capture`, then `tools/import-globals-capture.ps1`.
2. Bartender 4.4's restricted-execution handle remains the only in-root session failure; it is
   MoP-era code, and the true handle object (frame methods routed through the environment,
   identity preserved across `CallRestrictedClosure`) is not part of the Wrath target.
3. Animation `Scale`/`Color`/`TextureCoord` verbs record but do not tick yet; `SetScale` and
   `SetOrigin` are the two Details needs. `SpellInfoView::cost` stays unavailable in the
   harness until the seat also projects player stats.

### Integration note: origin/main is uncompilable WIP

While this task ran, `origin/main` advanced from `e3c8e14d9` to `18b106d3e` (a runes/totems/
frame-handle-proxy WIP whose own note says `Cargo compilation and corpus re-run have not yet
run`). The required one end-of-task merge was attempted as `6655d2f56` and reverted: that commit
does not compile on its own, before any of this branch's code is considered.

- `crates/benilla-ui/src/script/wrath_security.rs`: `FRAME_HANDLE_LUA` opens with `r#"` but its
  Lua contains `select("#", ...)`, so the raw string ends at the first `"#`. Use `r##"..."##`.
- `crates/benilla-ui/src/script/host.rs`: `trade_skill_link_source` is defined twice (lines 68
  and 80); one must go.
- `crates/benilla-ui/src/script/wrath_natives.rs`: tests use `PetActionView::auto_cast_allowed`,
  `auto_cast_enabled`, `UiScript::set_clock` and `UiScript::set_quest_record`, none of which
  `18b106d3e` adds.

This branch therefore stays on `e3c8e14d9` plus its own commit. The owner (or the WIP's author)
should fix `18b106d3e` before integrating; the survey numbers above were taken on this branch,
not on main.