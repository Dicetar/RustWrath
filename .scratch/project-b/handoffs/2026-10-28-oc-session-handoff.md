# Project B — OpenCode session handoff (2026-10-28)

Status: handoff preparation only. After this handoff task began, no build, test, lint, extractor,
game, socket, WM, server or DB operation was run. The client tree is clean at
`wotlk` / `36cf8042131191de0a42073ebc29032b48e4191d`; the only untracked paths are the
verification logs (Astra's runs and the earlier OC smoke runs) and the preserved procgen patch
helper. If this file is read after its own commit, that handoff commit is the new HEAD; the
source identity described here is the last code tree at `6e84aaa1` (all later commits are docs). This file records what is
actually in source, what was actually verified, what is claimed by others, and the exact next
source actions. It is not a completion record and not a playable-alpha claim.

Read this file top to bottom before editing. The decisive facts are inline; the linked
checkpoints contain supporting detail.

---

## 1. Mission and priorities

The owner-approved objective for this lane is a **faithful replacement of the current
AzerothCore 3.3.5a (12340) client, including the installed mods**. The full stock-client
replacement remains the **release gate**. A togglable Elden Ring-style combat mode and suitable
UI changes are approved platform goals, but they need real server authority; the client must not
invent stamina, dodge, i-frames, telegraphs or mode acceptance. Procgen is deferred and is not
this lane's work.

Repo context matters: the repository is documented as a 1.12.1 client (`docs/METHOD.md`,
`docs/CONTRIBUTING.md`). The `wotlk` branch is the Project B replacement lane built on that
baseline. Do not copy Wrath behavior into the vanilla default paths, and do not apply 1.12-only
assumptions to the explicit Wrath target. The stock 3.3.5 FrameXML is loaded from the player's
install; benilla's layer stays small and after stock, per METHOD. The install is read-only.

Evidence levels are separate and must never be collapsed:

| Level | Meaning here |
|---|---|
| Source | The implementation exists at a named commit; no compiler has necessarily seen it |
| Compilation | A compiler accepted the exact tree/pin named |
| Focused tests | Named tests/probes passed at a named tree |
| Receiver acceptance | The intended consumer (WM receiver, server, addon host) accepted the data/flow |
| Production consumption | The real app path (not a fixture) uses the source in a running client |
| Rendered acceptance | The scene/widget is visually inspected under a proven run |
| Demonstrated gameplay | The server-authoritative player loop is observed end to end |

No percentage and no playable-alpha date follows from this handoff. Full replacement, installed
mod acceptance, rendered Northshire and WM production consumption are all still open.

---

## 2. Repository and ownership state

### 2.1 Worktrees and checkouts

| Path | Branch / HEAD | State | Purpose / owner |
|---|---|---|---|
| `D:/Projects/project-b-client` | `wotlk` / `36cf8042131191de0a42073ebc29032b48e4191d` | Tracked tree clean; index empty; untracked owner artifacts | The authoritative client checkout. OC + Astra share this branch |
| `D:/Projects/project-b-opencode` | `opencode/project-b-profile-reload` / `abe317810e9a94f1f6d50a23cebcca6a32db64e7` | 2 tracked files modified, several untracked | Separate agent/workspace repo, not the client. Its commits include a 3.3.5 auth layer and a live logon against the lab authserver; that is **not** client replacement acceptance. Do not edit or commit here for client work |
| `D:/Projects/project-b-opencode-assets` | detached / `1f8d33b84ace0243ee7f6c01c09ff725fd2787a9` | 15 modified asset/format/profile files, 1 untracked | Separate asset experiment; owner not established in this session. Preserve; do not reset/stash/commit |
| `D:/WOW/wm-project` | `main` / `282d8f9eb1461dc6c847cd8ccd51f064e8f7351c` (ahead 5) | Many tracked/untracked WM files modified; `src/wm_native` itself is tracked and clean | WM platform repo. The native receiver package is now committed (`496c562`, `ca83bb3`, `c3e01fd`, `336aab2`, `282d8f9`), including observation schema v2.3. Preserve the rest; do not touch without the WM owner |
| `D:/WOW/ACRB/source/azerothcore` | `5e741c37364f4ec5e3bc2860bcf21f13812fb66d` | Read-only reference | AzerothCore server source used for wire/handler evidence. Not edited |
| `D:/WOW/world of warcraft 3.3.5a hd` | n/a | Read-only install | 12340 binary/assets and `Interface/AddOns`; never write into it |

`project-b-client` also has a `main` branch at `359eff41` (upstream/main). The work branch is
`wotlk`; do not switch/merge/reset.

Observed dirty paths in the other worktrees (re-check before touching; these are not the client
handoff's changes):

- `project-b-opencode`: modified `crates/benilla-profile/src/lib.rs`, `docs/METHOD.md`; untracked
  `.opencode/`, `crates/benilla-combat-core/`, `crates/benilla-wm-native/`, `docs/opencode/`,
  `integrations/`, `opencode.json`.
- `project-b-opencode-assets`: modified `Cargo.lock`; `crates/benilla-adt/src/lib.rs`;
  `crates/benilla-assets/{Cargo.toml,src/lib.rs,src/m2.rs}`;
  `crates/benilla-formats/{src/lib.rs,src/liquid.rs,src/models/anim.rs,src/models/bounds.rs,
  src/models/m2_batches.rs,src/terrain.rs}`; `crates/benilla-m2/{src/error.rs,src/lib.rs,
  src/skin.rs}`; `crates/benilla-profile/src/lib.rs`; untracked
  `crates/benilla-assets/src/profile_source.rs`.
- `D:/WOW/wm-project` tracked modifications: `bootstrap/sources.lock.json`,
  `control/runtime/spell_shell_bank.json`, `data/specs/custom_id_registry.json`,
  `docs/README_OPERATIONS_INDEX.md`, `docs/WM_PLATFORM_HANDOFF.md`,
  `native_modules/mod-wm-bridge/src/wm_bridge_bone_lure_item.cpp`,
  `native_modules/mod-wm-spells/src/wm_spell_runtime.cpp`, `scripts/bridge_lab/
  Configure-BridgeLabRuntime.ps1`, `scripts/bridge_lab/Start-BridgeLabAll.ps1`,
  `scripts/bridge_lab/Start-BridgeLabMySql.ps1`, `src/wm/autoplay/service.py`,
  `src/wm/autoplay/world_context.py`, `src/wm/content/release.py`, `src/wm/context/pack.py`,
  `src/wm/spells/client_patch.py`, `src/wm/spells/server_dbc.py`, `start-bridge-lab-all.bat`.
  Untracked: `client_extensions/`, `control/content_releases/`,
  `control/runtime/{custom_item_icons.json,spellmaking.json,spellmaking_advanced.json,
  spellmaking_v3.json}`, `core_patches/`, `docs/plans/multiclass-2026-10-02/`,
  `docs/plans/spellwright-v2.md`, `docs/plans/spellwright-v3.md`, `native_modules/mod-multiclass/`,
  the `scripts/bridge_lab/Apply-BridgeLab*.ps1`/`Compact-BridgeLabResourceIncludes.ps1`/
  `Deploy-BridgeLab*.ps1`/`Install-BridgeLabSpellmaking.py`/`Show-BridgeLabConsole.ps1` family,
  `scripts/client/Install-CustomItemIcons.py`, `src/wm/spells/item_icons.py`,
  `src/wm/spells/spellmaking.py`, `src/wm/spells/spellmaking_advanced.py`,
  `src/wm/spells/spellmaking_v3.py`, `state/`.

### 2.2 Current uncommitted paths in the client

- Staged: none.
- Unstaged tracked: none.
- Untracked, preserved and **not** part of any commit:
  - `.scratch/project-b/verification/2026-10-26-astra-once/`
  - `.scratch/project-b/verification/2026-10-27-astra-once/`
  - `.scratch/project-b/verification/2026-10-27-astra-ui2/`
  - `.scratch/project-b/verification/2026-10-27-astra-ui3/`
  - `.scratch/project-b/verification/2026-10-27-astra-ui4/`
  - `.scratch/project-b/verification/2026-10-27-astra-wrath-smoke/`
  - `.scratch/project-b/verification/2026-10-27-oc-wrath-smoke/` (59 `run*.log` stock-smoke runs)
  - `tools/patch_procgen_dungeon.py` (GLM/procgen helper; **never run**, never commit)

`git status` has no knowledge of another worker's index state; always re-check immediately
before committing and use explicit paths.

### 2.3 OC commits that define the current surface

Representative commits on the current surface, not a complete log; all are ancestors of HEAD.
"Evidence" is the evidence actually recorded for that slice, not a claim that later work is
covered.

| Commit | Behavior | Evidence recorded |
|---|---|---|
| `cc65287b` | WM observation v2.2 types and bounded action queues | Source; compile/test reported in the WM checkpoint |
| `f8a1993b` | Native cast/resource/aura read model and command contracts | Source; targeted checks reported |
| `adc8a631` | Aura observation state and receipt edges | Source; checked in Astra's bounded pass |
| `a8a51af9` | parry binned-BVH panic repair (vendored `third_party/parry3d`) | Standalone reproducer + check/build at the named tree; graphical half unproven |
| `08cfd64d` | Wrath `Cooldown` widget, sweep/edge/spark art, no timer authority | Source; later compiled by the app smoke; not rendered |
| `c3c27a6f` | Owner-backed world/difficulty/instance/threat/opt-out answers; removed false constants | Source; later compiled; not live |
| `2b04b6de` | `Map.dbc` instance name + party cap feed | Source; later compiled |
| `03ed4d1f` | `CMSG_OPT_OUT_OF_LOOT` send path | Source; later compiled |
| `b89af3a2` | `MSG_SET_DUNGEON_DIFFICULTY` / `MSG_SET_RAID_DIFFICULTY` send path | Source; server echo still absent |
| `6feb5d4d` | Native aura receipts + `SMSG_INITIAL_SPELLS` cooldown tail | Source + targeted checks (see native-state checkpoint) |
| `f2cee289` | Wrath animation group timeline (`order`, `startDelay`, signals) | Source; focused tests |
| `41dc768d` | Alpha animations on region owners | Source; focused tests |
| `e36601a3` | `SetStartDelay` | Source; focused tests |
| `a0471263` | Translation via frame/region offset maps | Source; focused tests |
| `c56040eb` | Timeline verification/tests; unused cleanups | Focused tests |
| `8666d6b1` | Test-only trade helper target argument (unblocks app lib tests) | Compilation/test only; no behavior |
| `7ca701cd` | `CreateAnimationGroup` / `CreateAnimation` / setters / `Pause`/`Resume` | Source; focused tests |
| `316f9897` | Stock smoke plays `TutorialFrameCallOutPulser` | Smoke test |
| `15163814` | Frame-owned Rotation, `SetDegrees`, `SetLooping`, orbit post-pass | Focused tests |
| `f7fe32eb` | `BOUNCE` reverse leg + per-leg base capture | Focused tests |
| `6e84aaa1` | `GetDuration`/`GetElapsed`/`GetProgress`/`SetToFinalAlpha` | Focused tests + smoke |
| `36cf8042` | Close animation-parity goal (checkpoint only) | None beyond docs |

The authoritative records are `.scratch/project-b/checkpoints/2026-10-28-oc-wrath-cooldown-widget.md`,
`.../2026-10-27-oc-native-player-state.md`, `.../2026-10-27-oc-wrath-smoke-bootstrap.md`,
`.../2026-10-28-oc-wrath-animation-timeline.md`, `.../2026-10-28-oc-wrath-animation-parity.md`,
`.../2026-10-26-oc-connected-gameplay-campaign.md`, `.../2026-10-26-oc-native-gameplay-contract.md`,
`.../2026-10-26-oc-wm-system.md` and `.../2026-10-26-oc-route-assets.md`.

### 2.4 Astra's recent shared-source work (not ours)

Astra's active checkpoint is `.scratch/project-b/checkpoints/2026-10-26-astra-wrath-ui-host.md`
(596 lines, updated through 2026-10-28). Her handoff is
`.scratch/project-b/handoffs/2026-10-27-astra-wrath-item-query-consumer.md`. Astra's latest
**code** commit before our animation work is `ed4e3702` (uniquely cached item names); `acb18abc`
is the last edit to her item handoff. The branch HEAD at preparation is `36cf8042` (the close of
our animation work); this handoff is later and is OC's. Do not claim her work as OC's and do not
repeat its implementation.

Recent Astra behavior in the tree: target-aware item-link producers (quest/craft/trade/loot/
auction/inspect/vendor/buyback/group-loot), Wrath `GetItemInfo`/`GetItemIcon`/`GetBuybackItemLink`/
`GetItemFamily`/`GetContainerItemDurability`/`GetItemCount`/`GetItemCooldown`, class/race
tooltip restrictions, cached-name resolution, negative-suffix name catalog, property/enchant DBC
layouts, keyring consumer repair (`c095756b`), container link/lootability, and tooltip subject
handling. Treat all of it as **source-present unless the named check says otherwise**; much of it
is documented as source-only, uncompiled or untested by Astra. Our app-lib smoke at `6e84aaa1`
compiled the crate graph containing `ed4e3702` and the earlier item code, but did not exercise
those behaviors.

Two reconciliation cautions:

- Astra's checkpoint sometimes names a **record/claim commit** (e.g. `18096c15`, `3104740a`,
  `960e78fd`, `d1dc10d6`) next to a behavior whose **code commit** is different. Example: the
  group-loot factor code is `8fd90bea` (`ui_loot_roll.rs`, +9/-1); `d1dc10d6` is only
  "Record buyback item link support". Verify behavior against source, never against the claim
  hash alone.
- The installed-addon discovery claim at checkpoint line 99 is stale: Wrath discovery now reads
  the selected data install's `Interface/AddOns` (`crates/benilla-app/src/ui_script/addons.rs:268-312,
  416`) and chain manifests. This is source-present, not addon-load acceptance.

### 2.5 Ownership, claims and collision risks

- **OC claims now:** no dirty client path. Completed claims are released. The next source claim
  should be only `crates/benilla-ui/src/script/animation.rs` for the Questie timer repair (WP1).
  If it needs `events_regions.rs`, check Astra's current claim first; it is an Astra-claimed
  file even though our `7ca701cd` added `CreateAnimationGroup` there.
- **Astra's live lane:** item/container/addon compatibility under `benilla-ui/src/script/**`
  (notably `item_stats/wrath.rs`, `container.rs`, `host.rs`, `char_stats.rs`, tooltip files) and
  app `ui_items/**`, `ui_loot/mod.rs`, `ui_loot_roll.rs`, `ui_trade/**`, `ui_merchant/**`,
  `ui_auction/**`, `ui_inspect.rs`, `ui_tooltip/**`, `ui_quest/**`, `script_calls.rs`,
  `ui_script/wrath_queries.rs`. Her current checkpoint is the claim list; the absence of a
  dirty file does not release a claim.
- **Collision risks for the planned work:**
  - Group loot: protocol/app translation is OC's lane; the UI owner and link code are Astra's.
    Do not edit `ui_loot_roll.rs` without coordinating.
  - Loot suffix factor: shared `benilla-protocol` struct touches all producers; Astra's link
    formatter is the consumer. Agree on the field shape before editing.
  - Bank capacity: `wrath.rs` mapping is OC's; `script/container.rs`, `script/char_stats.rs`,
    `ui_items/**` are Astra's. This is one connected package and must be coordinated.
  - Mail: `ui_mail/**` is donor/OC-adjacent; no current Astra claim, but `script/mail.rs` and the
    installed Postal/aux consumers need a typed contract before any binding.
  - Animation: `animation.rs` is ours; `events_regions.rs` and `wrath_ui_slice_tests.rs` were
    touched by us and are also Astra-owned/claimed. Coordinate any further edit.
- **Known shared-index incidents:** Astra's 2026-10-26 architecture-review commit attempt hit a
  transient shared-index race; the files stayed intact and the final commit used explicit
  document paths, with no history rewrite (`2026-10-26-astra-architecture-review.md:24`). The WM
  checkpoint records the same disclosure. No known accidental tracked path in current history;
  still, the shared index is a live hazard. Stage explicit paths only, inspect `git diff --cached`
  before every commit, and never `git add -A`.
- **Accidental inclusion check:** our `7ca701cd` intentionally touched Astra's
  `events_regions.rs`; `316f9897` touched her `wrath_ui_slice_tests.rs`; `8666d6b1` touched only
  `ui_trade/tests.rs` test helpers. None included unrelated dirty files. Preserve this discipline.

### 2.6 Uncommitted implementation and exact next edit

No client implementation is uncommitted. The next edit is **not yet started**:

- `crates/benilla-ui/src/script/animation.rs`:
  - `CreateAnimation` currently requires a type string (`:155-168`); Questie calls
    `anim:CreateAnimation()` with no type at
    `D:/WOW/world of warcraft 3.3.5a hd/Interface/AddOns/Questie-335/Compat/Compat.lua:166`.
    Accept a nil/missing type and default to `"Alpha"` (the reference's default animation type),
    preserving the existing named/typed path.
  - Animations have no `GetParent`; Questie calls `timer:GetParent():Stop()` at `Compat.lua:136`.
    Publish `GetParent` returning the stored `ANIMATION_PARENT` (the group for a child; the owner
    frame for a top-level group, if that reference fact is confirmed — see WP1).
  - Add one regression test in `animation::tests` that mirrors the Questie construction:
    `g = f:CreateAnimationGroup(); a = g:CreateAnimation(); a:SetDuration(0.05);
    a:SetScript("OnFinished", ...); p = a:GetParent(); p:SetLooping("REPEAT"); p:Play()`.
- Do not commit anything else in that commit. The test/build is a new execution grant; this
  session's grant is consumed (section 5).

---

## 3. Subsystem map

### 3.1 Session lifecycle, character selection, reconnect and scope invalidation

Authoritative files:

- `crates/benilla-app/src/wrath.rs`: `WrathNetPlugin::build` (`:1239`), `WrathControl`,
  `WrathSessionConfig`, `WrathFeedEvent`, `WrathLatest`, `WrathEvents`, `WrathOutbound`;
  `wrath_session_ownership` (`:1341`), `wrath_pick_admission` (`:1424`),
  `wrath_shutdown_on_exit` (`:1954`), `wrath_net_drain` (`:3614`), `wrath_apply_world`
  (`:4228`), `wrath_stream_movement` (`:4686`).
- `crates/benilla-protocol-wrath/src/session.rs`: `SessionState`, `Phase`, worker/phase edges.
- `crates/benilla-app/src/wrath_native.rs`: `WrathScope`, `WrathNativeState::invalidate_scope`
  (`:411`), cast/channel scope rules.
- `crates/benilla-app/src/ui_script/addons.rs`, `roster`/`CharPick` seam.

Current source behavior (as documented by the campaign): the select screen's stock `CharPick`
channel is rebound to a Wrath-owned queue; one worker at a time; replacement is admitted only
after the old worker joined and its terminal feed was drained; a clean logout returns to
character select; connection loss is a separate policy; the restart budget is bounded; terminal
events cannot tear down a newer session; scope changes invalidate native casts/auras/action state.
The stale-pick/reply and publisher-retirement rules are in `2026-10-26-opencode-client.md` O1 and
the native contract.

Evidence: source and targeted tests only. The stock smoke loads the host, not a session. No
character-select → enter-world → transfer → logout/re-entry run has been performed. The Northshire
run card is ungranted. Gaps: live acceptance of every lifecycle edge; reconnect policy; WM pending
actions across scope loss; any remaining scope bleed in UI aliases.

### 3.2 Gameplay commands and reply translation

Authoritative files:

- `crates/benilla-app/src/wrath.rs`: `translate_gameplay` (`:2018`) maps
  `benilla_protocol_wrath::gameplay::ServerEvent` to the shared `SessionEvent`; `wrath_tap_commands`
  (`:2870`) maps `ClientCommand` to 12340 opcodes/encoders; `wrath_dispatch_events` (`:3479`),
  `route_wm_action_outcomes` (`:3513`).
- `crates/benilla-app/src/net.rs` (`ClientCommand`, `SessionEvent` seam) and `net/io.rs` (the 1.12
  writer; it must keep ignoring Wrath-only commands).
- `crates/benilla-protocol-wrath/src/{opcode.rs,gameplay.rs}`: 12340 layouts and encoders.
- `crates/benilla-app/src/script_calls.rs`: host-side application of UI calls.

Covered by source and slice checkpoints: attack/cast/cancel, vendor list/buy/sell/buyback/repair,
trainer list/buy, bank open/purchase/move, chat and addon messages, area triggers, quest log/share,
item use/equip/swap/split/destroy, death/corpse/resurrect, one-shot services and queries, instance
difficulty selection, loot opt-out, and the donor loot/cooldown/aura owners.

Known gaps: Wrath group-loot inbound/roll-vote (section 4), Wrath mail, the difficulty server echo
and `SMSG_INSTANCE_DIFFICULTY`, `SMSG_THREAT_*`, and any later O7 systems. No live command/reply
path has been accepted in this session.

### 3.3 Cast/channel authority, cooldowns, auras and item replication

Authoritative files:

- `crates/benilla-app/src/wrath_native.rs`: `WrathCastEvent` (`:302`), `WrathCastEventKind`,
  `WrathActiveCast` (`:114`), `WrathActiveChannel` (`:194`), `WrathUnitResources` (`:102`),
  `WrathItemText`, `WrathInventoryFailure`, scope loss.
- `crates/benilla-app/src/wrath.rs`: `WrathWorldView` (`:1036`), `WrathAuraAnchors` (`:1174`),
  `last_cast_end_in_scope`, `WrathResolvedCast`, the net drain and native feed.
- UI consumers (Astra's lane): `ui_script/wrath_cast.rs` (native `UNIT_SPELLCAST_*` edges),
  `ui_script/wrath_aura.rs`, `ui_script/wrath_aura_feed.rs`, `ui_script/wrath_queries.rs`,
  `script/wrath_spell.rs`, `script/wrath_unit.rs`, `script/wrath_aura.rs`, tooltip aura files.
- Cooldown owner: `crates/benilla-app/src/spell/cooldowns.rs` (`SpellHistory`, `ui_triple`,
  `UiClock`); initial tail seeded by `6feb5d4d`; the UI widget is
  `crates/benilla-ui/src/script/cooldown.rs` + `widget/kinds/mod.rs` + `loader/widgets.rs` +
  `extract.rs` + `ui_pass.rs`/`shaders/ui_quad.wgsl` (sweep branch, edge/star4 art).
- Item replication: Astra's target-aware item links; `wrath.rs::push_item_fields` (21 of the
  Wrath enchant dwords; 7 of 12 slots) and `push_player_inventory_fields` (see 4c).

Semantics already recorded: attempt serial vs wire `cast_count`; START remaining time vs total;
`observed_at_unix_ms` vs server game time vs UI `GetTime`; unavailable vs observed zero; aura
receipt revisions and UI anchors; cooldown widget stores `(start,duration)` and the extract
computes the sweep from `Model::now` (no second timer authority). Auras use the native slots
because the vanilla descriptor projection drops high slots and changes flag meaning.

Evidence: source; focused tests for the animation/cooldown-adjacent pieces; targeted native-state
checks were run earlier (`cargo check` protocol/protocol-wrath/app/driver; cooldown 71, wrath 12,
ui_spellbook 6 at that tree). No cast/aura/cooldown rendering or live server observation exists.
Open: exact native-start reconstruction, interruptibility, full aura replacement policy, item
descriptor losses, suffix stats, enchant slots 7..11, and the widget's rendered acceptance.

### 3.4 Stock UI, Lua compatibility, installed-addon hosting and animations

Authoritative files:

- `crates/benilla-lua/**`, `crates/benilla-mlua/**`: Lua 5.0 donor default plus explicit Lua 5.1
  Wrath path; coroutines, libraries and string/table fixes are Astra's campaign.
- `crates/benilla-ui/src/script/**` (target, client, object model, events, loader), with the
  explicit `UiTarget::Wrath335` boundary. Production Wrath construction is selected by
  `WOW_WRATH`; vanilla stays the default.
- `crates/benilla-app/src/ui_script/addons.rs`: Wrath addon discovery from the selected data
  install's `Interface/AddOns` (`root()` `:268`, `chain_addons()` `:301`, `discover_folder()`
  `:416`), manifest parsing, enable-state, enable/disable writes only under `local_state`.
- `crates/benilla-app/src/ui_script/reference_ui.rs`: the stock `Interface\FrameXML\FrameXML.toc`
  loader; `WRATH_SKIPPED_FILES` currently only `VoiceChat.xml`; the smoke loads the whole TOC
  prefix through `PlayerFrame.xml`.
- Animations: `crates/benilla-ui/src/script/animation.rs`; loader
  `crates/benilla-ui/src/loader/animations.rs`; `frame:CreateAnimationGroup` in
  `script/object/events_regions.rs:180-189`; extract orbit post-pass in `script/extract.rs`;
  model offsets/rotations in `script/model.rs`; app carry in `ui_script/extract/mod.rs`.

Animation integration status (source): a group `Play` schedules children by `order` (parallel
within an order), honours `startDelay`, captures per-leg bases, fires `OnPlay`/`OnFinished`/`OnStop`,
supports `Pause`/`Resume`, `SetDuration`/`SetOrder`/`SetChange`/`SetOffset`/`SetStartDelay`/
`SetDegrees`/`SetLooping` (`REPEAT`, `BOUNCE`, `NONE`), `GetDuration`/`GetElapsed`/`GetProgress`/
`SetToFinalAlpha`; Alpha targets a frame or region, Translation moves frame/region rect readers,
Rotation orbits a frame's subtree and carries `ExtractedQuad.rotation`. The stock smoke plays
`TutorialFrameCallOutPulser`; seven focused animation tests pass at the `6e84aaa1` tree.

Actual limits: hit tests use unrotated rects; region-owned rotation is not applied; Scale/Color/
TextureCoord/VertexColor have no caller and are not implemented; BOUNCE is unit-tested only, not
visually proven; smoothing is ignored; the schedule assumes durations do not change mid-run;
repeating Alpha groups drift; group order is static. Installed Details spinners are source-only.
The install's Questie timer is a real dependency and has two concrete remaining call gaps
(`CreateAnimation()` and `GetParent()`), so **addon animation parity is not complete**.

Evidence: stock smoke and focused tests; no installed addon has been loaded in a running client;
no rendered animation inspection. The smoke's data gate is real (WM_ASSET_PROFILE=wm-hd,
interface 30300); a no-data skip is not evidence.

### 3.5 Assets, terrain, collision, models, WMO, graphical blockers

Authoritative records: `2026-10-26-oc-route-assets.md` (profile hashes, map tile, WMO/M2 pins),
`2026-10-27-oc-playable-alpha-campaign.md` (parry repair and blockers),
`third_party/parry3d/BENILLA.md`, `2026-10-27-northshire-loop.md` (run card).

Source state: profile/manifest reader; `.mdx`→`.m2` + external skin; M2 v264; ADT 12340 `MCNK`/
`MH2O`; WDT gating; WMO group loading and terrain/WMO collision accumulation exist. The parry
binned-BVH panic was reproduced and repaired in the vendored crate; the streamed graphical half
is still unproven. Profile identities: `wm-hd.mpqlist` SHA-256
`439bdc21d09f5509073cd88cc341adb2eb05854b425f4d9dec1d2bcee5083d59`; base
`00c7357f2a5599b05ea7b97e4b3529198deb17a458b47199f6567259a1bde1cf`; data root
`D:/WOW/world of warcraft 3.3.5a hd/data`.

Gaps: no rendered terrain/WMO/model/camera/input acceptance; no proven grounding on the abbey
path or bridge; route identity and character eligibility are live facts. The `benilla` binary has
not been rebuilt at HEAD.

### 3.6 WM observation/action integration and production consumers

Authoritative files: `crates/benilla-wm-native/**` (observation/action types),
`crates/benilla-wm-driver/**` (renderless publisher), `wrath_wm_action_intake` (`wrath.rs:1463`),
`route_wm_action_outcomes` (`:3513`), `WrathWmActions`; WM repo `src/wm_native/**` (schema v2.3,
receiver package committed through `282d8f9`).

Current source: observation carries self identity/position, combat/resources/death and freshness/
scope; actions are typed intents mapped onto existing `ClientCommand` owners, with admission,
deadline, dedup, cancellation and authoritative-result matching; remote movement is an explicit
`Unsupported` refusal; non-idempotent actions are not replayed.

Evidence: source and compile at earlier pins; the WM repo has the v2.3 receiver models and native
receiver package committed. No receiver run, ACK/store acceptance, production context read, action
round-trip, stale/disconnect test or human-override test has been demonstrated. Cross-host
settings reload is absent on both sides. Treat WM production consumption as **unaccepted**.

---

## 4. Latest shared-source reconciliation

Astra's handoff and checkpoint are the source of her recent work. All hashes named below are
ancestors of `36cf8042`; the tree is clean, so there is no uncommitted Astra code in the client
checkout. Her own evidence level is often source-only; do not upgrade it.

### 4.1 Recent item/buyback/bag/durability/tooltip work

Present in source and **not ours**: target-aware links for quest/craft/trade/loot/auction/
inspect/vendor/buyback/group-loot; Wrath `GetItemInfo`/`GetItemIcon`/`GetItemSpell`/
`GetInventoryItemID`/`GetBuybackItemLink`/`GetItemFamily`/`GetContainerItemDurability`/
`GetItemCount`/`GetItemCooldown`; cached-name query; negative-suffix name catalog and scaled
allocation data; 24/38-field random-property/enchant loaders; class/race/heirloom tooltip
restrictions; tooltip subject (GetUnit/GetItem/GetSpell) and `OnTooltipSetUnit`; container link/
lootability; keyring consumer span (all 32 seats); Bagnon bag family/free seats; aux/Postal
support bindings where present.

Caveats: source-present does not mean behavior-tested. Astra's handoff documents missing negative
suffix stat text, native property-slot losses, incomplete tooltip line law, no live addon run, and
no Wrath packet path for several UI surfaces. Our app-lib smoke compiled this crate graph at
`6e84aaa1` but did not exercise it.

### 4.2 The five source findings: verdicts and evidence

**(a) Wrath group-loot inbound events and vote submission appear absent despite stock UI
owners — CONFIRMED.**

- `crates/benilla-app/src/ui_loot_roll.rs` is the stock owner: it consumes
  `SessionEvent::{LootStartRoll,LootRoll,LootRollWon,LootAllPassed}` (`:274-286`) and sends
  `CMSG_LOOT_ROLL` from `LootRollSteps` (`:711-719`); `net.rs:1577` defines
  `ClientCommand::LootRoll`.
- The Wrath path has no corresponding wire pieces:
  `benilla-protocol-wrath/src/opcode.rs` defines only `CMSG_LOOT`/`CMSG_LOOT_MONEY`/
  `CMSG_LOOT_RELEASE`/`SMSG_LOOT_RESPONSE`/`SMSG_LOOT_RELEASE_RESPONSE`/`SMSG_LOOT_REMOVED`/
  `SMSG_LOOT_MONEY_NOTIFY`/`SMSG_LOOT_CLEAR_MONEY`/`CMSG_AUTOSTORE_LOOT_ITEM` (no
  `SMSG_LOOT_START_ROLL`, `SMSG_LOOT_ROLL`, `SMSG_LOOT_ROLL_WON`, `SMSG_LOOT_ALL_PASSED`,
  `CMSG_LOOT_ROLL`).
- `gameplay.rs` has no roll readers/variants; `wrath.rs::translate_gameplay` has no roll arms;
  `wrath_tap_commands` has no `ClientCommand::LootRoll` arm, so even the donor UI's vote command
  would be dropped under Wrath.
- Server evidence: AC `Opcodes.h:700-704` (0x29E..0x2A2), `GroupHandler.cpp:494-499` reads
  `guid, slot, rollType`; `Group.cpp:862-961` writes the start/roll/won/all-passed layouts. The
  start packet is guid, map ID, slot, item, suffix, property, **item count**, **countdown**, and
  the vote mask; the roll packet adds the roller guid, roll number, roll type and **auto-pass**;
  the won packet carries the winner and the vote pairs. These target-only fields must be parsed
  from the source, not guessed from the donor shape.
- Qualification: the stock UI owner exists and works on the donor path; the gap is the Wrath
  protocol/adapter/outbound tap, not a missing UI frame.

**(b) Loot-response suffix-factor loss can produce links rejected by AzerothCore — CONFIRMED.**

- `gameplay.rs:2894-2917` (`read_loot_response`) reads `random_property_id` and discards the
  preceding `_random_suffix` word (`:2910-2913`). `protocol-wrath::LootItem` has no factor;
  `wrath.rs:2820-2840` builds `ProtocolLootItem` without one; the shared
  `benilla-protocol/src/messages/loot.rs:11-20` has no factor field.
- `ui_loot/mod.rs:844` still says the link's factor is zero because the wire suffix always is;
  that is the 1.12 assumption and is false for the Wrath loot response.
- AC `Chat/HyperlinkTags.cpp:184-185` rejects a random suffix with zero base amount and a nonzero
  base amount with no suffix; `:138-141` requires the full nine-field item link form. The server
  has the separate suffix word, so a Wrath-shaped link cannot be made acceptable until the factor
  is carried through the decoder and shared contract.
- `read_item_push` (`gameplay.rs:2925-2939`) already retains `suffix_factor`, and the group-loot
  `ActiveRoll` retains it (`8fd90bea`); the loot-response path and `ItemInfo`'s
  `_random_suffix` (`:2331`) are the losses. Do not substitute a guessed factor.
- Ownership: protocol/app translation is OC's; the UI link consumer is Astra's. Coordinate the
  shared struct change.

**(c) The seventh bank bag requires coordinated projection, mapping, UI and purchase-gate work —
CONFIRMED, and the four missing vault slots are part of the same gap.**

- AC `Entities/Player/Player.h:697-704`: bank items 39..66 (28 slots), bank bags 67..73 (seven).
- `wrath.rs:4072-4089` copies `(402,564,48)` = 24 of 28 bank slots and `(458,612,12)` = six of
  seven bags; `wrath.rs:1903-1918` maps the six stock bags 63..68 to 67..72, not 73.
- UI/app constants: `ui_items/mod.rs:53-60` (`BANK_SLOTS=24`, `BANK_BAGS=6`,
  `BANK_BAG_SLOT_FIRST=63`); `script/char_stats.rs:262-265` (`BANK_BAG_SLOT_COUNT=6`) and
  `:555-557` (`BANK_INV_SLOTS=40..=63`, `BANK_BAG_INV_SLOTS=64..=69`);
  `script/container.rs:284-285` (`BANK_BAGS=5..=10`);
  `script/bank.rs:4-7,52-65` (six bags, `GetNumBankSlots` full at six);
  `ui_bank/mod.rs:211-225` (purchase refuses at `bought >= 6`).
- The stock bank path must be checked against the actual 12340 `BankFrame`/`GetNumBankSlots`
  usage; changing only a UI constant would walk the wrong descriptor band. Change the native
  projection, band mapping, UI arrays/slots, purchase gate and stock-facing full predicate
  together. Astra owns the container/keyring consumers; coordinate before touching
  `script/container.rs` or `ui_items/**`.
- Keyring is a separate, already-repaired case: native retains 32 seats; the code repair is
  `c095756b`, and `3104740a` is its record commit. Do not conflate it with the still-open bank
  capacity gap.

**(d) Wrath mail attachments and installed Postal/aux API expectations exceed the donor mail
contract — CONFIRMED.**

- The donor UI mail owner is single-attachment: `script/mail.rs:71-80` has one item per message,
  `GetInboxItem` returns five values (`:615-660`), `GetSendMailItem` four (`:921+`); the app
  `ui_mail` state carries one `MailAttachment`.
- `wrath.rs` has no mail list/attachment translation; the only mail mention is the legacy
  `(text_id, mail_id)` item-text refusal at `:3196`. `protocol-wrath/src/gameplay.rs` has no mail
  decoder/event.
- Installed consumers: `aux-addon/core/tooltip.lua:226,288` calls `GetInboxItemLink`/
  `GetSendMailItemLink`; Postal enumerates multiple inbox rows/attachments
  (`postal/modules/openall.lua:207,235`, `select.lua:156-300`, `express.lua:50-55`) and uses
  `GetInboxHeaderInfo`. A real Wrath mail contract needs list/attachment events and target
  bindings; a synthetic list would be fabrication.
- Ownership: no current Astra claim on `ui_mail/**`, but the work crosses OC protocol/app and the
  UI mail owner; establish the contract before editing.

**(e) Questie's local timers depend on animation groups and `OnFinished` — CONFIRMED, now mostly
source-connected but still blocked by two exact calls.**

- Corrected attribution: the animation timer is in
  `Questie-335/Compat/Compat.lua:158-188` (`QuestieCompat.C_Timer`), not in `WMQuestieCompat`
  (which is only an error-suppression patch). The selected `Questie-335.toc` (`Interface: 30300`)
  loads `Modules\QuestieCompat.lua` and `Compat\embeds.xml` (`:22-23`); the embeds loads the
  Compat-local `Libs\AceTimer-3.0` (OnUpdate implementation, `Compat/Libs/.../AceTimer-3.0.lua:467`)
  and then `Compat.lua` (`embeds.xml:6,30`). The top-level `Questie-335/Libs/AceTimer-3.0` copy
  is not loaded by this TOC and its global `C_Timer.After` dependency is not evidence for this
  path. `Compat.lua:45` creates `QuestieCompat.frame`; core modules alias
  `QuestieCompat.C_Timer` (`Modules/QuestieInit.lua:87`, `Modules/QuestieEventHandler.lua:50`,
  `Modules/QuestieCoordinates.lua:8`, `Modules/Libs/QuestieCombatQueue.lua:8`,
  `Modules/Tracker/TrackerFadeTicker.lua:12`).
- The engine now supplies `CreateAnimationGroup`, `SetDuration`, `SetScript("OnFinished")`,
  `SetLooping("REPEAT")`, `Play`, group `Stop`, and child `OnFinished`. Two calls still fail:
  `anim:CreateAnimation()` with no type (`Compat.lua:166`; our binding requires a string) and
  `timer:GetParent()` (`Compat.lua:136`; animations have no `GetParent`). Until both are fixed
  and addon loading is exercised, Questie timers are not accepted.
- The exact next source edit is WP1 in section 7.

### 4.3 Claim-hash and status cautions

- `d1dc10d6` is not the group-loot factor code; it is "Record buyback item link support"
  (handoff-only). The code is `8fd90bea` ("Retain suffix factor in Wrath group loot links").
- `18096c15`, `3104740a`, `960e78fd` are record/claim commits; the behavior they discuss is in
  other commits (e.g. keyring consumer `c095756b`, item count `0d2029f7`, item links `535ac85f`/
  `98e00eed`). A claim hash is never proof of behavior.
- The claim that "current production discovery sees only local_state/AddOns" is stale; Wrath
  `addons.rs` now reads the install's `Interface/AddOns` and the chain manifests.

---

## 5. Verification ledger and permissions

### 5.1 What this handoff personally inspected

- `git status`, `git log`, `git show --stat`, `git rev-parse` in `project-b-client` at
  `36cf8042`; the worktree HEADs/hashes and dirty states in section 2.
- The source files and line ranges cited in this document, the local 12340 FrameXML/addons,
  the local AC source at `5e741c37`, and the existing checkpoint/handoff documents.
- The latest logged OC stock-smoke run `2026-10-27-oc-wrath-smoke/run59.log` (passing).
- The Astra UI logs' failure lists and counts (not rerun).

### 5.2 Runs actually executed (this session or earlier sessions, with ownership)

| Check / run | Command and tree | Result and evidence |
|---|---|---|
| Animation focused tests (this session) | `cargo test --locked --offline -p benilla-ui --lib 'animation::tests'` at the tree committed as `6e84aaa1` | 7 passed, 0 failed, 3333 filtered. Terminal output inspected; no log file written |
| Stock Wrath UI smoke (this session) | `cargo test --locked --offline -p benilla-app --lib ui_script::wrath_ui_slice_tests::stock_player_frame_host_smoke_test -- --exact --nocapture` with absolute `WM_ASSET_MANIFEST=D:/Projects/project-b-client/docs/profiles/wm-hd.mpqlist`, `WM_ASSET_PROFILE=wm-hd`, `WOW_DATA=D:/WOW/world of warcraft 3.3.5a hd/data`, `BENILLA_REQUIRE_DATA=1`; tree `6e84aaa1` | 1 passed, 0 failed, 6601 filtered; target Wrath335/interface 30300/profile wm-hd; 0.50 s. Terminal output inspected; earlier identical runs are logged under `2026-10-27-oc-wrath-smoke/run59.log` |
| Earlier OC stock smoke (earlier session) | run9..run59 logs | `run59.log` passes. The checkpoint records the failure progression; these are earlier-session evidence, not re-run now |
| Native-state checks (earlier session) | `cargo check` protocol-wrath/protocol/app-lib/driver; `cargo test -p benilla-app --lib cooldown` 71; `wrath` 12; `ui_spellbook` 6; reducer/learned/initial-cooldown tests | Recorded in `2026-10-27-oc-native-player-state.md`; owner-reported by OC. No dedicated aura-receipt/initial-cooldown case |
| Parry repair (earlier session) | standalone reproducer + `cargo check`/`cargo build -p benilla` at `d8250c5c` | Recorded in `2026-10-27-oc-playable-alpha-campaign.md`; the graphical half is not run |
| Cooldown widget (earlier session) | No build/test initially (source-only claim) | Superseded only for compilation by the `6e84aaa1` app-lib smoke; the widget was not rendered |
| Astra bounded pass 1 (external) | `cargo check --locked --offline -p benilla-app`; `cargo test -p benilla-lua -p benilla-mlua -p benilla-ui` at `e3c83d90` | Check pass; UI 3321 passed / 3 failed / 1 ignored. Logs `2026-10-27-astra-once/`; owner-reported |
| Astra UI pass 2 (external) | same UI test invocation at `29ff13f9` | UI 3322 passed / 2 failed / 1 ignored; logs `2026-10-27-astra-ui2/`; owner-reported |
| Astra UI pass 3 (external) | UI suite at a later pin (see log) | UI 3322 passed / 2 failed / 1 ignored; logs `2026-10-27-astra-ui3/`; owner-reported |
| Astra UI pass 4 (external) | 3333 tests at `b52ad4d3`/`be001728` | `b52ad4d3`: 3329 passed / 3 failed (SetShown + two timezone); `be001728`: 3329 passed / 3 failed (layout clamp + two timezone). Logs `2026-10-27-astra-ui4/`; owner-reported. Astra fixed both source failures; no rerun |
| Astra focused smoke (external) | One app-lib smoke at `404cea29` | Failed at `Constants.lua:87` (`FillLocalizedClassList` nil); logs `2026-10-27-astra-wrath-smoke/`; owner-reported. Later OC smoke passes |

Known environmental failures: `script::tests::stdlib::date_and_time_read_the_local_zone` and
`script::tests::stdlib::time_converts_a_date_table` (host timezone). The 2026-10-26 and 2026-10-27
Astra logs show the same two; the third failure differs by pin.

**Important discrepancy:** the session context claims a full UI suite of "3333 passed + 2 known
environmental failures", but no such log exists in `.scratch/project-b/verification/`. The logged
runs are 3321/2, 3322/2 and 3329-pass results as above, and the current tree has 3340 UI tests
(our 7 animation tests were added after the last logged suite). Do not repeat the "3333 passed"
claim as current-tree evidence. A fresh full-suite run is required after any grant.

### 5.3 Uncompiled / untested / unexecuted

**Compiled at `6e84aaa1`:** the `benilla-app --lib` test binary and its dependency graph,
including `benilla-ui` (the seven animation tests compile), `benilla-lua`, `benilla-mlua`,
`benilla-formats` and `benilla-wm-native`; all code at HEAD is in that graph except the notes
below. `6e84aaa1..36cf8042` differs only by the close checkpoint (verified: `git diff
6e84aaa1..36cf8042 -- crates` is empty); this handoff adds only docs.

**Not compiled at HEAD:** the `benilla` player binary (the run card's `cargo build -p benilla` is
at `d8250c5c`, before the cooldown/animation/Astra item work), `benilla-wm-driver`, other
workspace targets/benches not in the app-lib graph, and any doc/test target outside that run.
Do not call the player build green.

**Untested behavior (source only):** every live session/character path; the cooldown widget's
rendered sweep; Rotation/Translation/BOUNCE visually; installed addon loading (including Questie);
all Astra item/tooltip/container behaviors; group loot; mail; bank capacity; difficulty echo/
threat; WM receiver/action; the Northshire route; parry streamed attach.

**Unexecuted proposed commands:** the Northshire run card (`cargo run -p benilla --release` with
the owner's environment), the WM receiver/runtime, any server/DB command, any addon load, any
network/socket trace, and any `cargo fmt`/`clippy` (not installed on this machine).

**Unauthorized runtime acceptance:** no game/graphical/socket/WM/server/DB run was performed in
this session. The live logon recorded in `project-b-opencode` belongs to that separate worktree's
auth work; it does not close client G/L.

### 5.4 Permissions and credentials

- The standing Project B work-package rule is **source work and commits only** unless the owner
  gives an explicit execution grant (`2026-10-26-system-goals.md:27-31`,
  `2026-10-26-opencode-client.md:87-99`). This handoff is under that restriction.
- This OC session had a user grant for builds/tests (the user's earlier "autorized"). It was
  consumed by the animation test run and the app-lib smoke; it is **not** transferred to a fresh
  session.
- Astra's grants are hers and are not OC authorization. The recorded consumed grants are the
  2026-10-27 bounded check/test, the second `29ff13f9` pass, the focused smoke at `404cea29`, the
  eight-case UI grant at `b52ad4d3`, and the later bounded runs. The owner has also reported a
  newly authorized bounded item/UI verification pass for Astra; its exact pin/scope must come from
  the owner, it is **not** an OC grant, and OC must not run checks under it.
- Credentials are owner-held. Record only safe variable names: `WOW_USER`, `WOW_PASS`, `WOW_CHAR`
  (or `.probe-identity` at the repo root), `WOW_HOST`, `WOW_WRATH_AUTH`, `WOW_WRATH_WORLD`,
  `WM_NATIVE_OBSERVATION_TOKEN`. Never copy values, tokens or the contents of `.probe-identity`.
  The install/data paths and profile names are safe; the lab server process/account details are
  owner-held.
- Do not transfer an earlier session's grant to Astra or to a fresh OC session. A pending request
  is not authorization.

---

## 6. Decisions, traps and rejected approaches

### 6.1 Approved boundaries

- Server authority for gameplay outcomes, combat mode, health/resources/death, loot, quests,
  cooldowns and item state. Client-side animation/prediction may present, never grant.
- The stock UI is the core; benilla's layer is small, loads after stock and before addons, and does
  not grow without a named test. FrameXML/addon files come from the player's install; never commit
  them.
- One owner per resource/state store. Do not add a second cooldown timer, aura store, item cache,
  inventory walker, event registry, WM publisher or action bus when an existing owner can be
  extended.
- Target-aware behavior: a Wrath-only binding or layout must be selected by the actual `UiTarget`
  (or the actual session target), not by payload length, an ambient env var in a generic helper,
  or a guess. Preserve the donor path and its tests.
- Explicit-path commits only; preserve other workers' dirty/untracked files; never reset/stash/
  clean/rebase/rewrite shared history. No broad `git add`.
- No install writes, no credentials/secrets in the repo, no environment copy.

### 6.2 Proposed or unaccepted decisions (do not silently promote these)

- `SMSG_AURA_UPDATE_ALL` full-snapshot **replacement vs merge** is not settled. The current code
  merges with coverage and marks edges; the server writer does not prove the client's replacement
  policy. Do not guess.
- Group-loot packet fields beyond the donor set (map ID, item count, countdown and vote mask in
  the start; auto-pass in the roll) need a complete target mapping before implementation.
- Bank capacity layout: how to widen the donor player-array band without overlapping buyback/
  keyring, and whether to change shared `BANK_SLOTS`/`BANK_BAGS` constants or add a target view.
  This is a design decision, not a constant bump.
- Mail attachment contract: list vs per-index events, multi-attachment send/receive, and which
  target bindings to expose. No synthetic attachment list.
- Rotation-aware hit tests/region-owned rotation: no current caller; deferred.
- WM schema v2.4 for aura receipts/clock or settings reload: client-internal today; a coordinated
  WM contract would be required.

### 6.3 Lossy projections and target-only facts (do not report as lossless)

- Bank: 24 of 28 vault slots; six of seven bank bags; one bag GUID and four vault GUIDs are
  dropped in `push_player_inventory_fields`.
- Item descriptors: 21 of the Wrath enchant dwords are copied (7 of 12 enchant slots); property
  slots 7..11 are absent. The permanent/socket slots used by current links survive; that is not
  full item replication.
- Loot response: the random suffix word is discarded before the shared `ProtocolLootItem`; links
  are zero-factor until repaired.
- Aura/descriptor projection: high slots and target flag semantics differ; use the native aura
  owner, not the vanilla descriptor.
- Item/quest/craft/trade/etc. link producers were migrated progressively; group loot, some mail
  and inspect/trade/auction paths still have target-specific gaps documented by Astra.
- The 12340 target uses Lua 5.1/30300 and Wrath-only APIs; the donor is 1.12.1/Lua 5.0. Never
  infer a Wrath API from a 1.12 name or vice versa.

### 6.4 Known defects vs hypotheses

Confirmed source defects/open gaps: the Questie timer calls (WP1); the loot factor drop; the
missing Wrath group-roll path and vote tap; the bank capacity mismatch chain; the absent Wrath
mail contract; the difficulty echo/threat producers; the native bank/enchant projections; and the
absence of rendered/live acceptance. Confirmed source behavior, not defects: animation group
semantics are unit-tested; the smoke host loads the stock TOC prefix; Astra's item source is
committed.

Hypotheses/open questions: actual AC server acceptance of rolled links; whether some installed
addons are enabled/disabled by the user's SavedVariables; whether the user's pinned character
exists/eligibility; whether the parry graphical path is now clean; the exact 12340 `GetParent`
behavior for a top-level group; whether `AURA_UPDATE_ALL` replaces or merges; exact target layout
for group-loot countdown/vote mask.

### 6.5 Failed approaches to remember

- Do not reintroduce `CooldownFrame_SetTimer` as a Lua fallback: stock `Cooldown.lua` owns the
  wrapper, and the native was removed so the widget's `SetCooldown` is the single sink.
- Do not answer world/difficulty/threat queries with open-world constants. The false bootstrap
  answers were removed; missing producers stay nil/unavailable.
- Do not clamp/skip geometry to hide the parry BVH panic; the vendored binning repair was made at
  the mechanism, and a repeat panic disproves it.
- Do not add a fake `C_Timer`, success stub, fake addon frame, synthetic item/attachment or
  invented resource to make a load pass. Addons must load against real owners or fail visibly.
- Do not treat a claim hash, a commit message, a fixture pass or an app-lib compile as receiver/
  production/rendered evidence.
- Do not use a global `WOW_WRATH`/payload-length heuristic inside a generic helper to select the
  item-link layout; carry the actual target.

### 6.6 Reference sources and scope

- `benilla-re` (1.12.1) is the baseline for vanilla behavior; local clone lookup per
  `docs/CONTRIBUTING.md`. It does not cover the 12340 target.
- 12340 evidence used here: the player's install at
  `D:/WOW/world of warcraft 3.3.5a hd` (binary strings, `Interface/FrameXML`, installed
  `Interface/AddOns`), local AC source `D:/WOW/ACRB/source/azerothcore` at `5e741c37`, the
  wowgaming 3.3.5 FrameXML pin `d0339b17b0221db76e6acd2dc2915d224a5b62ca`, and the previously
  cited ClassicAPI/Wowless references. These are read-only; no binary/executable inspection ran
  here.
- The local binary/FrameXML/addon evidence establishes existence/field facts; it does not by
  itself prove runtime timing or server acceptance.

### 6.7 Shared-index hazards and stop conditions

- The checkout is shared with Astra. Race hazards are real: check `git status`, `git diff`,
  `git diff --cached` and `git log -1` immediately before each commit; stage only the exact
  handoff/code paths. Never commit another worker's logged/untracked artifacts.
- Stop and report (do not repair live) if: a run needs credentials/DB/server changes; a panic
  occurs (preserve log/screenshot; the parry repair is then disproven); a roster/character is
  ineligible; a client/source identity does not match the run card; a change would require
  overwriting another worker's claim; or the answer depends on a live fact you cannot access.
- Do not start a build/test/run just because this handoff exists. The next session's first action
  is source-only WP1 and a claim, not execution.

---

## 7. Restart plan — substantial work packages

Order is dependency-driven. "Source-only" means no build/test/run until a new grant. "Grant"
means the named check/run needs explicit owner authorization. "Coordinate" means Astra/WM owner
must agree on the shared interface before edits.

### WP1 (current in-flight) — finish Questie's animation timer contract

- **Why:** Questie-335 is the largest installed compatibility target and its `Compat.C_Timer`
  is used by core modules at load; the animation engine is already most of the way there.
- **Files:** `crates/benilla-ui/src/script/animation.rs` only (plus a test case there).
  Coordinate only if `events_regions.rs` is unavoidable.
- **Edits:** optional default type `"Alpha"` in `CreateAnimation`; `GetParent` (child→group,
  group→owner with the reference behavior confirmed/recorded); one regression test mirroring
  `Questie-335/Compat/Compat.lua:158-188`.
- **Completion:** the test passes; a Wrath host load of `Compat.lua` no longer raises at
  `CreateAnimation`/`GetParent`; then the next addon load failure is captured. Source-only until a
  grant; test creation is also gated.
- **Owner:** OC. No Astra collision on `animation.rs`; `events_regions.rs` is hers.

### WP2 — Wrath group-loot inbound + vote submission

- **Why:** the stock `GroupLootFrame` and roll chat lines already exist; without the Wrath wire
  path, group loot is dead under 3.3.5, and the stock UI's votes are dropped.
- **Files:** `crates/benilla-protocol-wrath/src/{opcode.rs,gameplay.rs}`;
  `crates/benilla-app/src/wrath.rs` (`translate_gameplay`, `wrath_tap_commands`); optionally
  shared message types if a field is missing. Read-only/coordinate on `ui_loot_roll.rs`.
- **Contract references:** AC `Opcodes.h:700-704`, `Handlers/GroupHandler.cpp:494-499`,
  `Groups/Group.cpp:862-961`. Start packet: guid, map ID, slot, item, suffix, property, item count,
  countdown, vote mask; roll: guid, slot, roller, item, suffix, property, roll number, roll type,
  auto-pass; won: target, roll number, roll type plus votes; all-passed: guid, slot, item,
  property, suffix. Verify every field/byte against the local source before coding; do not guess
  countdown semantics.
- **Completion:** source typechecks (grant); decode/translate tests (grant); the existing donor
  loot-roll tests remain unchanged; a live group roll and vote round-trip (grant) match the
  server. No local confirmation is fabricated.

### WP3 — preserve the loot-response suffix factor end to end

- **Why:** Wrath rolled loot links are rejected by AC (`HyperlinkTags.cpp:184-185`) because the
  factor is dropped before the shared item event; the UI formatter already knows how to place it.
- **Files:** `benilla-protocol-wrath/src/gameplay.rs` (`LootItem`, `read_loot_response`);
  `benilla-protocol/src/messages/loot.rs` (field/default and all constructors);
  `crates/benilla-app/src/wrath.rs` (translation); `crates/benilla-app/src/ui_loot/mod.rs`
  (row/link) and the shared target-aware formatter. Coordinate with Astra on the link consumer.
- **Completion:** factor survives decoder → shared event → row/link; the 1.12 path stays zero and
  its tests unchanged; server acceptance of a rolled link is tested live (grant). Do not invent a
  factor for item pushes or group-loot events that already carry one.

### WP4 — bank capacity: 28 vault slots and seven bank bags

- **Why:** the replacement client cannot present or purchase the target bank; the loss is in the
  native projection and consumer constants, not just one slot.
- **Files:** `crates/benilla-app/src/wrath.rs` (`BANDS`, `wrath_bank_bag`, `wrath_item_pos`);
  `crates/benilla-app/src/ui_items/mod.rs`; `crates/benilla-ui/src/script/{char_stats.rs,
  container.rs,bank.rs}`; `crates/benilla-app/src/ui_bank/mod.rs`. Coordinate Astra on
  container/keyring consumers; do not touch her keyring repair.
- **Design first:** choose the lossless band mapping that does not collide with buyback/keyring,
  or add a target-aware view. AC `Player.h:697-704` is the authority. Do not simply raise
  `BANK_BAG_SLOT_COUNT` and walk the wrong band.
- **Completion:** all 28 vault slots and seven bag GUIDs project; stock 12340 `BankFrame` sees
  seven; purchase the seventh bag once; existing vanilla tests unchanged; live bank purchase/move
  (grant).

### WP5 — Wrath mail list/attachments

- **Why:** installed Postal/aux expect multi-attachment inbox/send and links; the donor owner is
  single-attachment and the Wrath path has no mail decode.
- **Files:** `benilla-protocol-wrath/**`, `wrath.rs`, `ui_mail/**`, `script/mail.rs`, target item
  link helper. Establish the contract from AC `MailHandler`/`MailPackets` before coding.
- **Completion:** inbox header/list, attachment enumeration and links for the selected addons;
  send/return/take paths; live mail (grant). No synthetic list.

### WP6 — difficulty echo, `SMSG_INSTANCE_DIFFICULTY` and threat producers

- **Why:** the cooldown slice sends the difficulty selection but cannot confirm it; threat is nil
  with no producer. These are visible world-state gaps.
- **Files:** `benilla-protocol-wrath/src/opcode.rs`/`gameplay.rs`; `wrath.rs`;
  `ui_instance.rs`; `script/wrath_world.rs` and the CVar/`MapDifficulty` owners.
- **Contract:** the server echoes `MSG_SET_DUNGEON/RAID_DIFFICULTY` with
  `(difficulty, unk=1, isInGroup)`, `SMSG_INSTANCE_DIFFICULTY` 0x33B, `SMSG_(HIGHEST_)THREAT_*`
  0x482-0x485 (`MiscHandler.cpp:1264`; local AC source). Verify exact bodies.
- **Completion:** getters reflect the server echo; `UnitThreatSituation` uses the real producer;
  tests/check (grant); live group/instance (grant).

### WP7 — installed-addon hosting acceptance (Questie first)

- **Why:** discovery/loading source exists, but no installed addon has been exercised; Questie is
  the concrete next dependency after WP1.
- **Work:** use the Wrath host/addon loader with the real install read-only; load
  `Questie-335.toc`, capture the first failure, fix only demonstrated API/loader gaps; repeat for
  the other requested addons in the owner's priority order.
- **Gates:** a harness run and an addon-load run need a grant; do not claim success from source or
  a fixture. Completion: the selected addon initializes and its tracker/timers run without
  errors, evidenced by a log; this is addon acceptance, not full replacement.

### WP8 — Northshire rendered/live acceptance

- **Why:** the G/L gate is the only way to close terrain/collision/model/camera/quest/loot/reward
  and re-entry; source progress cannot substitute.
- **Work:** rebuild `benilla` at the selected source identity (grant), use the run card
  `2026-10-27-northshire-loop.md` exactly (profile/manifest, character eligibility, stop
  conditions), capture logs/screenshots, and repair only the owning seam after a failure.
- **Dependencies:** WP6 (world-state truth) and the asset/collision source are useful but not
  blockers; the run may reveal the next asset defect. Keep G distinct from L.
- **Completion:** the documented loop passes with retained server state and no stale scope; one
  repeat of the affected path after any repair.

### WP9 — WM production consumption

- **Why:** the WM repo now has the receiver/native package, but production consumption and actions
  are unaccepted.
- **Work:** coordinate with the WM owner; run the receiver/runtime with a token, confirm hello/
  schema freshness, receive a production observation, submit a permitted action and observe the
  authoritative result; test stale/disconnect/cancel. The client-side source is OC's; the WM repo
  is the owner's.
- **Completion:** receiver ACK/store + production context + one action result + stale/disconnect
  behavior; logs retained. No client-only test substitutes.

### Source-only vs grant vs coordination

- Source-only now: WP1, WP2 (code), WP3 (code), WP4 (design/implementation after agreement),
  WP5 (contract/code), WP6 (code), and source parts of WP7.
- Grant required: any `cargo check`/test/new case, the player build, the G/L run, the addon load,
  the WM receiver/action, any server/DB/socket operation.
- Coordinate: Astra for WP2 UI link, WP3 shared struct/link, WP4 container/keyring, WP5 mail UI;
  WM owner for WP9. Do not edit her claimed files or the WM repo without agreement.

### Next concrete source action

Open `crates/benilla-ui/src/script/animation.rs`, add the nil/missing-type default to
`CreateAnimation` and the `GetParent` publication, and write the Questie-shaped regression case
in the existing `animation::tests` module. Claim only that file, commit it explicitly after the
granted test passes, and update the animation parity checkpoint with the actual result. Then move
to WP2 (group loot) or WP3 (loot factor), whichever the owner prioritizes.

---

## 8. Fresh-session bootstrap prompt

Paste this into a new OC session:

```text
You are continuing Project B as the client/gameplay integration lead. Work only in
D:/Projects/project-b-client, branch wotlk. Do not touch D:/Projects/project-b-opencode,
D:/Projects/project-b-opencode-assets or D:/WOW/wm-project unless the owner explicitly
assigns that repo to you.

Read in this order before editing:
1. D:/Projects/project-b-client/AGENTS.md
2. docs/METHOD.md and docs/CONTRIBUTING.md (standing rules; source-only unless granted)
3. .scratch/project-b/handoffs/2026-10-28-oc-session-handoff.md (this handoff)
4. .scratch/project-b/checkpoints/2026-10-28-oc-wrath-animation-parity.md and
   .scratch/project-b/checkpoints/2026-10-28-oc-wrath-cooldown-widget.md
5. .scratch/project-b/checkpoints/2026-10-27-oc-native-player-state.md,
   2026-10-27-oc-wrath-smoke-bootstrap.md and 2026-10-26-oc-connected-gameplay-campaign.md
6. Astra's .scratch/project-b/handoffs/2026-10-27-astra-wrath-item-query-consumer.md
   and .scratch/project-b/checkpoints/2026-10-26-astra-wrath-ui-host.md (skim the claim tail)

Before any edit:
- Revalidate: git -C D:/Projects/project-b-client rev-parse HEAD; git status --short;
  git diff --cached --stat; git log --oneline -10. HEAD should be at or after 36cf8042.
- Inspect untracked paths and never stage/commit/delete another worker's verification logs or
  tools/patch_procgen_dungeon.py. Never reset/stash/clean/rebase or use git add -A.
- Claim files explicitly in a checkpoint before editing. Preserve Astra's claimed files
  (item/container/addon/UI link lanes) and all WM repo changes.

Resume the in-flight source slice, WP1:
- In crates/benilla-ui/src/script/animation.rs, let CreateAnimation() default to "Alpha" and
  add Animation:GetParent (child -> group; group -> owner with the reference behavior recorded).
- Add one regression test in animation::tests that mirrors Questie-335/Compat/Compat.lua:158-188
  (frame:CreateAnimationGroup; group:CreateAnimation(); child:SetDuration; child:SetScript
  "OnFinished"; child:GetParent(); group:SetLooping("REPEAT"); group:Play).
- Do not build or run anything until the owner grants it. The previous session's grant is
  consumed, and Astra's bounded item/UI grant is not yours.

Then continue in the order of section 7 of the handoff: Wrath group-loot inbound/vote (WP2),
loot suffix-factor preservation (WP3), bank capacity (WP4), Wrath mail (WP5), difficulty/threat
(WP6), installed-addon load acceptance (WP7), Northshire G/L (WP8), WM production consumption
(WP9). Source-only until granted; coordinate with Astra on shared UI/link/container files and
with the WM owner on the WM repo.

Evidence rules: report source, compilation, focused tests, receiver acceptance, production
consumption, rendered acceptance and demonstrated gameplay separately. Never close a live gate
from source or unit evidence. No playable-alpha date or completion percentage. Record exact
commands and logs; do not copy credentials or tokens.
```
