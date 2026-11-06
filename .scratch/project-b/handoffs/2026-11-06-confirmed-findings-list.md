# Confirmed findings and model assignments — 2026-11-06

Baseline: `7e05392f` plus the uncommitted Beam/DeepSeek/Big Pickle work in
`D:\Projects\rustwrath-main-integration`. These are source findings; no build, client or receiver
acceptance is implied.

## Final integration — 2026-11-07

Integrated in `D:\Projects\rustwrath-main-integration` on `integration/autoswarm-waves-2026-11-05`
and published to `origin/main` as `2084c232` (fast-forward from `7e05392f`); the primary checkout
`D:\Projects\project-b-client` fast-forwarded to the same commit. All 20 targeted cases pass;
`cargo check --workspace --all-targets` passes; `cargo build -p benilla --features hotpatch`
passes; the 25-test `characters::sections::tests` module passes against the installed 3.3.5a HD
data. No live client, addon, visual or receiver acceptance has been performed.

| # | Priority | Final status | Evidence |
|---|---|---|---|
| 1 | P1 | **Fixed.** An unsettled attach records `Equipment::default()` as a pending sentinel; the settled set differs, so redress runs. | `an_unsettled_attach_stamp_is_a_sentinel_a_settled_snapshot_never_equals`, `a_pending_attach_stamp_does_not_suppress_the_settled_redress` |
| 2 | P1 | **Fixed/verified.** The decoder carries 12340's six collect-item pairs into the donor objective rows; the selected row's own record answers its detail text. | `a_selected_quest_answers_its_own_detail_text_not_a_placeholder_or_another_row`, `a_pending_record_lands_with_its_detail_text`, `read_quest_template` tests |
| 3 | P1 | **Fixed.** Wrath watch natives, the 25-watch cap, `QUEST_ACCEPTED` row index and the deferred `QUEST_LOG_UPDATE`; the stock tracker auto-arms. `AUTO_QUEST_WATCH` was not changed. | `a_quest_entering_the_log_fires_quest_accepted_before_the_deferred_log_update`, `a_watch_special_item_answers_the_held_starting_item_only`, `the_wrath_watch_list_adds_removes_and_holds_twenty_five` |
| 4 | P1 | **Fixed/verified.** Offer choices and mandatory rewards keep packet order and are indexed once each; the reward window sends the zero-based choice. | `quest_offer_reads_each_choice_and_reward_once_in_packet_order`, `reward_offer_snapshot_keeps_each_choice_and_reward_once`, `the_reward_window_offers_each_choice_once_and_sends_its_index` |
| 5 | P1 | **Fixed (fidelity).** The reported world-origin cull was not reproduced: the old path used the unit root, not the origin. The shake now uses the first effect node's post-attachment point, with the root fallback and 80 yd cull kept. | `a_bone_attached_kit_shakes_at_its_post_attachment_point`, `a_kit_without_an_effect_node_shakes_at_the_unit_root`, `a_fixed_attachment_point_is_still_culled_past_eighty_yards` |
| 6 | P2 | **Fixed.** The additive `Nameplate-Glow` region is no longer skipped; the extract carries its ADD blend to `ui_quad.wgsl` (EGxBlend 3). | `a_hovered_plate_emits_its_additive_glow_quad` |
| 7 | P2 | **Fixed.** The real deviation was a 256-subsegment cap, not the shipped 12-segment peak; it now uses the reference's 1000-point ceiling. | `a_long_hop_is_not_truncated_at_twelve_or_256` |
| 8 | P2 | **Fixed.** Dynamic minimap dots hold their projected coordinate for the reference's ~1 s window; landmarks still re-project each frame. | `a_moved_blip_holds_its_projection_for_the_stale_window`, `a_zoom_change_reprojects_and_the_cache_stays_bounded` |
| 9 | P2 | **Fixed.** Mounted nameplates follow the full seat oscillation; the 0.7 damping and slow mean are removed. | `a_mounted_plate_follows_the_full_seat` |
| 10 | P2 | **Fixed.** A timed-out endpoint resolves visibly as `UNKNOWNOBJECT` once, with the other endpoint's real name and no replay. | `a_never_resolving_name_falls_back_visibly_once` |
| 11 | P2 | **Fixed.** The 3.3.5 selector is `bag + 1`, keyring `KEYRING_FIRST..=KEYRING_LAST` 86..=117, everything else bag 255 as the backpack; the bank-bag and buyback ranges are covered. | `push_container_matrix_covers_backpack_equipped_bags_bank_and_keyring` |
| 12 | P3 | **Fixed.** `Hold` re-arms each pass with a `variation -1` weighted roll instead of one repeating play. | `hold_rearms_each_pass_with_a_variation_roll` |
| 13 | P3 | **Fixed.** A parked rig keeps its slope without a ray; a wake eases from the held vector, and only first sight seeds. | `a_parked_rig_holds_its_slope_and_wakes_without_reseeding`, `a_first_sight_while_parked_seeds_then_holds` |
| 14 | P3 | **Fixed.** World text above 32 px shapes a 32 px raster and stretches the quads; at or below 32 px the true size is used. | `world_text_raster_caps_at_32_and_stretches_above_it` |
| 15 | P3 | **Fixed.** A body whose scene root is hidden is not a tab candidate; the draw election's root verdict is read through `InheritedVisibility`. | `a_scene_hidden_body_does_not_shift_the_draw_election` |
| 16 | P3 | **Fixed (glow).** The glue/death glow is the reference's packed byte, not the unquantized pin. The liquid part is a sound-path test gap; no source change was made. | `the_glue_screens_pin_three_verified_glow_weights` |

Honorable mentions: cinematic letterbox and loading-screen fit verified with aspect tests; chat
bubble moved to the unbounded gx diagonal basis; aura alpha uses the reference's linear 8-bit
ramp; the death screen is an instant ghost-flag switch. The world-map arrow pair, 3.3.5 POI icon
gate and action-cooldown `UiClock` handoff are included.

## Original assignment table (historical)

| # | Priority | Finding | Location | Status / evidence | Assigned model |
|---|---|---|---|---|---|
| 1 | P1 | Self-avatar attaches naked or with stale gear; the 1 s attach fallback stamps the unsettled equipment as `AppliedEquipment`, so the redress gate never re-runs. | `crates/benilla-app/src/entities/attach/mod.rs:309-320`, `:779-782`; `redress.rs:106-108` | **Confirmed.** Big Pickle trial failed immediately on `provider.free-limit`; no files changed. Needs a DeepSeek fallback (or a later Big Pickle retry). | **Big Pickle blocked → DeepSeek fallback** |
| 2 | P1 | Some quests open with no journal text. | `crates/benilla-app/src/ui_quest_log.rs`, `crates/benilla-ui/src/script/quest_log.rs` | **Implemented by DeepSeek, unverified.** Retains the 12340 collect-item block and resolves the row’s own quest id. | **DeepSeek** |
| 3 | P1 | No quest-tracking UI; the stock watch frame/objective rows never appear. | `crates/benilla-app/src/ui_quest*`, `crates/benilla-ui/src/script/quest_log.rs`, quest UI assets | **Implemented by DeepSeek, unverified.** Adds Wrath watch natives, 25-watch cap and `QUEST_ACCEPTED` row firing. | **DeepSeek** |
| 4 | P1 | "Skirmish at Echo Ridge" reward-choice window has no rewards, so the quest cannot be completed. | `crates/benilla-app/src/wrath.rs` quest reward translation; `crates/benilla-ui/src/script/quest_log.rs` | **Implemented/tests added by DeepSeek, unverified.** The full offer/choice shape is preserved; no production decoder defect was proven. | **DeepSeek** |
| 5 | P1 | Bone-attached combat-shake kits are culled before they shake: the shake origin is read before the attachment fills it, so bone-attached first nodes shake at the world origin and are culled past 80 yd. 26 of 58 shipped shake kits are dead. | `crates/benilla-app/src/camera_shake.rs:290` and `fire_kit_shakes` | **Confirmed.** The code comment itself documents the pre-attachment read and the 26/58 loss. | **DeepSeek** |
| 6 | P2 | Lit nameplate glow is never drawn; the plate brightens the bar instead. The additive `Nameplate-Glow` rim is skipped in the linear pipeline. | `crates/benilla-ui/src/script/extract.rs:221` | **Confirmed source finding.** | **Beam** |
| 7 | P2 | Long visual chain beams are truncated: a ~30 yd hop peaks at 12 sub-segments against the reference's unbounded subdivision. | `crates/benilla-app/src/entities/chain_beam.rs:54-56`, `subdivide` | **Confirmed src comment/deviation; exact cap needs tracing (the constant is 256, the shipped rows peak at 12).** | **DeepSeek** |
| 8 | P2 | Minimap blips re-project every frame instead of drawing ~1 s stale coordinates as the reference does. | `crates/benilla-app/src/minimap/blips/mod.rs:28` | **Confirmed as a documented deviation.** | **Beam** |
| 9 | P2 | Mounted nameplates damp the seat oscillation to 0.7 instead of following the full seat motion. | `crates/benilla-app/src/nameplates.rs:289` | **Confirmed source finding.** | **Beam** |
| 10 | P2 | A kill with an unstreamed endpoint name is silently dropped after the 5 s retry; no combat line, toast or feedback. | `crates/benilla-app/src/ui_script/wrath_combat_log_feed.rs:77-92` | **Confirmed.** Only `party_kill_unresolved`/debug records the loss. | **DeepSeek** |
| 11 | P2 | Loot push receive-line key matrix has gaps; a mis-keyed push means the player never hears "You loot / receive". | `crates/benilla-app/src/ui_loot/mod.rs:77-85` (`push_container`), receive selectors below | **Needs matrix audit against 3.3.5 `OnItemPush`/`GetItemInfo`.** | **Beam** |
| 12 | P3 | Spell Hold does not re-arm with per-pass variation; one repeating play instead of the reference's re-arm (`variation −1`). | `crates/benilla-app/src/entities/spell_fx/lifecycle.rs:11` | **Confirmed source finding.** | **DeepSeek** |
| 13 | P3 | Entity conform tracking is limited to flagged-visual units; parked rigs skip the ray and re-seed at the slope on wake. | `crates/benilla-app/src/entities/conform.rs:67` | **Confirmed source finding.** | **DeepSeek** |
| 14 | P3 | Crit-pop combat text rasterizes at true size; the reference stretches the ≤32 px raster. | `crates/benilla-app/src/combat_text/mod.rs:264` | **Confirmed source finding.** | **Beam** |
| 15 | P3 | Exterior cull is missing the scene-attach gate; draw election shifts exterior scene content. | `crates/benilla-app/src/target/scan.rs:368` | **Confirmed source finding.** | **DeepSeek** |
| 16 | P3 | Ambient/glow and liquid visual deviations: `ffx_glow` quantize/behavior and terrain/liquid sampled-position flags. | `crates/benilla-world/src/ffx_glow.rs:176`; terrain/liquid sampled-position path | **Confirmed source finding.** | **Beam** |

## Resolved / closed this session

- Party-kill burst no longer loses all but the last kill: bounded `WrathWorldView.party_kills`,
  ordered drain, retry window and drop counters (already at HEAD).
- Quest-marker status table is clean/test-pinned; no open marker-table change.
- Weapon-trail `fade_step` resolved and test-pinned.
- Beam appearance resolved during the sweep.
- Re-dress readiness/retry fixed.
- Skin-composite missing-layer reporting fixed (the atlas now reports every layer that fails all
  candidates).

## Not a bug / claim corrected

- **`AUTO_QUEST_WATCH` is not the tracker bug.** 3.3.5a
  `QuestLogFrame.lua:261` compares `AUTO_QUEST_WATCH == "1"` (string), and the host seeds `"1"` in
  `crates/benilla-app/assets/ui/OptionsFrame.xml:33`. The actual missing tracker remains item 3,
  not the flag type.

## Honorable mentions — not in the top 10

- Cinematic letterbox — `crates/benilla-app/src/cinematic.rs:117`
- Loading-screen aspect — `crates/benilla-app/src/loading_screen.rs:64`
- Chat-bubble text basis — `crates/benilla-app/src/chat_bubble.rs:23`
- Aura alpha ramp — `crates/benilla-app/src/aura_visual.rs`
- Corpse/death fade — `crates/benilla-app/src/death/mod.rs`
- FFX glow quantize — `crates/benilla-world/src/ffx_glow.rs:176` (same area as item 16)
- Terrain/water liquid tests (sound path)

## Worker results (source only; nothing compiled or accepted)

- **DeepSeek quest**: completed. Changed `benilla-protocol-wrath/src/gameplay.rs`,
  `ui_quest_log.rs`, `ui_quest/mod.rs`, `ui_quest/verb_tests.rs`,
  `benilla-ui/src/script/quest_log.rs`, `quest_log_detail_tests.rs`. Added nine focused tests.
  Left unbacked: the fifth query string/completed-log text, intermediate drop-item ids, and
  POI-proximity sorting in `SortQuestWatches`.
- **DeepSeek map**: completed. Added the world-map arrow-effect frame pair, the 3.3.5 POI icon
  gate and tests; changed `worldmap.rs`, `worldmap_arrow.rs`, `tests/worldmap.rs`,
  `ui_world_map.rs`, plus one `mod.rs` re-export.
- **Beam cooldown**: completed. Changed `ui_action/state.rs` to use the `UiClock` anchor instead of
  `Instant::now()` for action cooldowns; added regression tests in `action.rs`/`cooldown.rs`.
- **Beam skin**: superseded by DeepSeek Batch B. The real cause was the HD 512² base skin with
  128×64 armour sheets; `blit_over` now follows the reference's scale/crop branch. The real-data
  `characters::sections::tests` module passes 25/25 against the installed HD data.
- **DeepSeek batches A/B/C**: completed source-only. Batch A fixed the shake attachment, beam
  bound, stale dots, mounted seat, parked conform, scene gate and Hold re-arm. Batch B fixed the
  attach sentinel, HD front-armour blit, glow extract, kill fallback, loot key matrix, crit raster
  and glow quantize. Batch C finished the quest/watch/reward path and the cinematic, loading,
  bubble, aura and death checks.
- **Final integration verification**: the 20-case targeted batch passes (17 after the first pass,
  20 after fixing the three harness/logic defects), the optional loading/death cases pass, and
  the section-compositor module passes 25/25 with the HD install. No live acceptance.
- **Big Pickle attach trial**: failed at startup with `provider.free-limit` (no model call); no
  files changed.
- **Flint quest/map**: both failed at startup with `provider.free-limit`; no files changed.

## Assignment shape

- **Big Pickle**: item 1 (explicit trial).
- **DeepSeek**: items 2-5, 7, 10, 12, 13, 15 (engine/state, retry and lifecycle reasoning).
- **Beam**: items 6, 8, 9, 11, 14, 16 (contained extract/minimap/nameplate/loot/raster/visual
  pipeline changes).
- Do not launch a second worker on the same files while an owner is active; Big Pickle owns item 1
  and DeepSeek owns the active quest files.
