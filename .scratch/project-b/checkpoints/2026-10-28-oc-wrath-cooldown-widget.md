# OC — Wrath cooldown widget and false gameplay-state answers

Date: 2026-10-28
Owner: OpenCode (client/gameplay integration lead)
Base HEAD: `7ec2be19` (Astra's bag query review over `8e919cec`)
Work: complete the 12340 `Cooldown` widget, drive its sweep through the existing UI
extract/pass, integrate every existing cooldown consumer, and replace the bootstrap
gameplay-state constants with their existing owners.

## File claim

Editing:

- `crates/benilla-ui/src/widget/kinds/mod.rs`
- `crates/benilla-ui/src/script/cooldown.rs` (new)
- `crates/benilla-ui/src/script/mod.rs`
- `crates/benilla-ui/src/script/object.rs`
- `crates/benilla-ui/src/script/types.rs`
- `crates/benilla-ui/src/script/extract.rs`
- `crates/benilla-ui/src/loader/widgets.rs`
- `crates/benilla-ui/src/loader/mod.rs`
- `crates/benilla-ui/src/script/wrath_natives.rs` (bootstrap answers leave it)
- `crates/benilla-ui/src/script/wrath_world.rs` (new; owner-backed unit/world queries)
- `crates/benilla-app/src/ui_script/extract/mod.rs`
- `crates/benilla-app/src/ui_pass.rs`
- `crates/benilla-app/src/shaders/ui_quad.wgsl`
- `crates/benilla-app/src/ui_script/wrath_ui_slice_tests.rs` (fixture rows only)
- this checkpoint

Coordinating: Astra owns item/container/addon compatibility; her committed `8e919cec` and
`7ec2be19` are not touched. No procgen files.

## Recovered 12340 Cooldown contract (local evidence)

The installed client binary (`D:\WOW\world of warcraft 3.3.5a hd\wow.exe`, ASCII strings) proves
the widget surface exactly:

- widget type `Cooldown`; methods `SetCooldown`, `SetReverse`, `GetReverse`, `SetDrawEdge`,
  `GetDrawEdge`; no `GetCooldownTimes`, no color/countdown-font/text setters (all absent).
- `SetCooldown` failure text: `Usage: %s:SetCooldown(start, duration)` — two numeric arguments,
  receiver named.
- XML attributes `reverse` and `drawEdge`.
- default art: `interface\cooldown\edge.blp` (16x16; a gold `255,211,0` one-pixel radius from the
  top edge to the centre, alpha 255 -> 0 toward the centre) and `interface\cooldown\star4.blp`
  (128x128; an opaque-alpha, greyscale four-point spark at the centre, black elsewhere, so it is
  an ADD blend).

Stock callers (`Interface/FrameXML` through the TOC, the `Script file` includes walked):

- `Cooldown.lua` defines `CooldownFrame_SetTimer(self, start, duration, enable)`, the only
  wrapper: `start > 0 and duration > 0 and enable > 0` calls `self:SetCooldown(start, duration)`
  and shows, else hides.
- Every stock consumer calls that wrapper: `ActionButton.lua:348`, `BankFrame.lua:80`,
  `BonusActionBarFrame.lua:172`, `ContainerFrame.lua:352`, `FriendsFrame.lua:127` (inverted
  enable), `MultiCastActionBarFrame.lua:892`, `PaperDollFrame.lua:1273`/`:1984`,
  `PetActionBarFrame.lua:177`, `PetPaperDollFrame.lua:431`, `RuneFrame.lua:53` (display flag),
  `SpellBookFrame.lua:466`, `TargetFrame.lua:383`/`:444`, `TotemFrame.lua:122`,
  `UIParent.lua:3281`/`:3337` (auras), `WatchFrame.lua:1194`.
- XML instances: `ActionButtonTemplate.xml:63`, `BankFrame.xml:22`/`:52`, `ContainerFrame.xml:29`,
  `FriendsFrame.xml:176`/`:346`, `PaperDollFrame.xml:17`/`:73`/`:1109`, `PetPaperDollFrame.xml:14`,
  `SpellBookFrame.xml:141`, `TargetFrame.xml:30`/`:93`/`:153` (reverse + drawEdge),
  `TotemFrame.xml:42` (reverse), `WatchFrame.xml:61`, `Cooldown.xml:4` (template).
- Installed addons only go through the stock wrapper: bartender4 `petbutton.lua:207`,
  `stancebar.lua:93`; Bagnon `components/item.lua:429` (clear with 0,0,0). No installed addon
  calls a Cooldown method directly or creates its own Cooldown frame.

Presentation recovered from the art and the binary defaults:

- the dark wipe is engine-drawn (no art file): the remaining-time sector of the frame's rect,
  clockwise from the top, `reverse` mirroring it;
- when `drawEdge`, the gold hand (`edge.blp`) is drawn as the frame-sized square rotated to the
  sweep boundary (the art is a radius from the top edge to the centre);
- the spark (`star4.blp`) is the frame-sized square centred on the boundary rim, ADD blended.

Not independently recoverable from the install: the dark fill's exact colour/alpha (no data; the
widget family's documented default black at 0.8 alpha is used and named as such), and whether the
spark is drawn when `drawEdge` is off (the art is a widget default, so it is always drawn).

## Existing cooldown owners (no second timer authority)

- Player `SpellHistory` — `crates/benilla-app/src/spell/cooldowns.rs`, fed by the 12340 handlers
  wired in the O2c campaign; `CooldownInfo::ui_triple` projects onto the VM `GetTime` clock through
  `UiClock`.
- `GetActionCooldown` / `GetSpellCooldown` (ID and book forms) / `GetContainerItemCooldown` /
  `GetInventoryItemCooldown` / `GetPetActionCooldown` / `GetShapeshiftFormCooldown` /
  `GetCompanionCooldown` / `GetTradeSkillCooldown` read those owners; every stock cooldown
  consumer routes through `CooldownFrame_SetTimer` and therefore `Cooldown:SetCooldown`.
- Aura timers ride `UIParent.lua:3281`/`:3337`, already called with `(expirationTime - duration,
  duration, 1)` from the native aura projection.

The widget keeps no timers: `SetCooldown` stores `(start, duration)` on the VM clock and the
extract computes the sweep from `Model::now`, the same `GetTime` every producer stamps.

## Progress (2026-10-28)

Base moved under the work: Astra landed `e5d76281`/`c095756b`/`3104740a`/`73ab3fc3`
(keyring/merchant/trade/action); this campaign's changes still apply cleanly on top and do not
touch her lanes.

Implemented in source, uncommitted:

- `FrameKind::Cooldown` and `KindState::Cooldown(CooldownState)` (`start`, `duration`, `reverse`,
  `draw_edge`; `progress(now)`), created by the game-layer factory for the Wrath in-game VM; the
  kind's method tables are reached by `guard_receivers` (`every_frame_kind` now 21) and the type
  chain is `Cooldown → Frame → Region`.
- `script/cooldown.rs`: `SetCooldown(start, duration)` (two numeric args, 12340 usage line),
  `SetReverse`/`GetReverse`/`SetDrawEdge`/`GetDrawEdge` (truthiness coercion, 1/nil reads), built
  only for `UiTarget::Wrath335`.
- XML loader: `apply_cooldown` reads `reverse`/`drawEdge` per level through the setters, last
  level winning.
- Extract: a running timer (`0 <= progress < 1`) emits `QuadContent::Cooldown`; `drawEdge` emits
  the frame-sized `edge.blp` hand rotated to the boundary (the rotation is compensated for a
  non-square rect), and the `star4.blp` spark is emitted frame-sized at the rim (ADD).
- App: the cooldown quad becomes a texture-less `UiQuad` with `sweep: true`; its corner colours
  encode `(u, v, progress, ±alpha)`; the run and material key split on `sweep`; the shader's
  binding 13 draws the remaining-time sector as black at `0.8 × frame alpha`, mirrored when the
  w sign is negative.
- `CooldownFrame_SetTimer` fallback removed from `wrath_natives.rs`: stock `Cooldown.lua` owns it;
  removing the native is what makes the widget's own `SetCooldown` the single sink.

Known deviations / gaps to verify:

- Not compiled, tested, or rendered (campaign allows source and explicit-path commits only).
- Dark fill colour/alpha (black, 0.8) is a named approximation; the spark always draws; the hand
  is stretched to a non-square rect rather than confined to its short side.
- The frame alpha rides the w channel's magnitude, so clipping cannot alter it; reverse is its
  sign. A rotated (frame-level `SetRotation`) cooldown would misplace the hand/spark.
- No timer countdown text is drawn: the 12340 binary registers no such setter, and this campaign
  found no stock caller drawing a text overlay on a `Cooldown` frame.
- Step 4 is not implemented yet: the bootstrap `wrath_natives.rs` answers for
  `GetDungeonDifficulty`, `GetRaidDifficulty`, `GetOptOutOfLoot`, `UnitThreatSituation`,
  `GetInstanceInfo` and `UnitInBattleground` still stand. The plan is `wrath_world.rs` over
  `Model::instance_type` (+ a map view pushed by `ui_instance.rs`), the `dungeonDifficulty`/
  `playerDifficulty` CVar rows (nil when absent), a per-VM opt-out flag with a
  `CMSG_OPT_OUT_OF_LOOT` (0x409) send, `nil` threat until an `SMSG_THREAT_UPDATE` producer exists,
  and nil difficulty-name/player-difficulty until a `MapDifficulty`/difficulty producer exists.
