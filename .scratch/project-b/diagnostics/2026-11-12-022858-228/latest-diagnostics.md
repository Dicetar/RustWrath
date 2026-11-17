# RustWrath In-Game Diagnostic & Triage Report
- **Run Directory:** `D:\Projects\project-b-client\benilla-config\Logs\Launcher\20261112-022858-228`
- **Flight Recorder Snapshot:** `D:\Projects\project-b-client\benilla-config\Logs\InteractionDiagnostics\interaction-1794440675417481200.jsonl` (run_id: `18e721700bffc534-ae78`, received: 3310, dropped: 0)
- **Prepped Tickets File:** `D:\Projects\project-b-client\benilla-config\Logs\Launcher\latest-tickets.md` (12 ready)
- **Generated:** 2026-11-12T02:44:40.9258300+03:00
- **Total Log Lines Scanned:** 3606

## Coverage and limits
This report combines stdout/stderr log scanning with the in-game InteractionDiagnostics flight recorder. A loaded UI is not proof that its controls work. Requests, rejections and guarded features need triage; they are not all client bugs. Missing geometry, wrong textures, invisible loot and unresponsive controls can be silent. Record those as observations with screenshots and reproduction steps.

## Prepped Resolution Tickets Summary
| Ticket | Category | Title | Target Files |
| --- | --- | --- | --- |
| **AUTO-01** | UI / Lua Globals | Implement missing Wrath Lua global functions (18 unique) | `crates/benilla-ui/src/script/`, `crates/benilla-app/src/ui_script/` |
| **AUTO-02** | UI / Widget Methods | Implement missing FrameXML widget methods (5 unique) | `crates/benilla-ui/src/script/widget/` |
| **AUTO-03** | UI / Frame & Table Fields | Resolve nil field/global indexing in stock UI or addons (1 unique) | `crates/benilla-ui/src/script/`, `crates/benilla-app/src/ui_script/` |
| **AUTO-04** | CVars | Register missing 3.3.5a CVars (10 unique) | `crates/benilla-app/src/cvars/` |
| **AUTO-05** | UI / Lua Runtime | Fix Lua / FrameXML runtime errors (147 unique) | `crates/benilla-ui/src/script/`, `crates/benilla-app/src/ui_script/` |
| **AUTO-06** | DBC / Formats | Fix DBC schema / record layout mismatches (2 unique) | `crates/benilla-formats/src/dbc.rs`, `crates/benilla-app/src/` |
| **AUTO-07** | Assets / M2 / BLP | Fix M2 / Skin / BLP / WMO asset load failures (21 unique) | `crates/benilla-formats/src/`, `crates/benilla-assets/src/` |
| **AUTO-08** | 3D Models & Player Visuals | Fix 3D character/entity model attachment failure or settle timeout (7 unique) | `crates/benilla-app/src/entities/attach/`, `crates/benilla-app/src/entities/equipment/` |
| **AUTO-09** | AddOns & Third-Party UI | Resolve third-party AddOn loading, version, or dependency errors (43 unique) | `crates/benilla-app/src/ui_script/addons.rs`, `crates/benilla-ui/src/script/addon.rs` |
| **AUTO-10** | Flight Recorder / Incidents | Triage and resolve 64 recorded interaction incident(s) | `crates/benilla-app/src/quest_trace.rs`, `crates/benilla-app/src/wrath.rs`, `crates/benilla-app/src/ui_quest/` |
| **AUTO-11** | Quests / Correlations | Investigate 4 quest/journal/turn-in correlation anomaly(ies) | `crates/benilla-app/src/ui_quest/`, `crates/benilla-app/src/ui_quest_log.rs`, `crates/benilla-protocol-wrath/src/gameplay.rs` |
| **AUTO-12** | Combat / Cast / Loot / Vendor | Investigate 11 cast or loot/vendor correlation anomaly(ies) | `crates/benilla-app/src/spell/`, `crates/benilla-app/src/ui_loot/`, `crates/benilla-app/src/ui_merchant/`, `crates/benilla-app/src/wrath.rs` |

| Area | Unique diagnostic records | Owning Modules | Acceptance |
| --- | ---: | --- | --- |
| Geometry | 13 | `crates/benilla-world/src/model_render.rs`, `crates/benilla-world/src/rig_anim.rs`, `crates/benilla-formats/src/m2/` | Unverified by log scan |
| Textures | 26 | `crates/benilla-assets/src/`, `crates/benilla-formats/src/blp.rs`, `crates/benilla-app/src/shaders/` | Unverified by log scan |
| Quests | 7 | `crates/benilla-app/src/ui_quest/`, `crates/benilla-app/src/ui_quest_log.rs`, `crates/benilla-app/src/ui_gossip/`, `crates/benilla-protocol-wrath/src/gameplay.rs` | Unverified by log scan |
| LootBags | 2 | `crates/benilla-app/src/ui_loot/`, `crates/benilla-app/src/ui_merchant/`, `crates/benilla-app/src/ui_items/` | Unverified by log scan |
| Abilities | 40 | `crates/benilla-app/src/spell/`, `crates/benilla-app/src/ui_action/`, `crates/benilla-app/src/ui_cast.rs`, `crates/benilla-app/src/wrath.rs` | Unverified by log scan |
| UI | 194 | `crates/benilla-ui/src/script/`, `crates/benilla-app/src/ui_script/`, `crates/benilla-ui/src/xml/` | Unverified by log scan |
| Movement | 14 | `crates/benilla-app/src/player/`, `crates/benilla-app/src/wrath.rs`, `crates/benilla-world/src/` | Unverified by log scan |
| Connection | 11 | `crates/benilla-app/src/wrath.rs`, `crates/benilla-protocol-wrath/src/`, `crates/benilla-app/src/glue_session/` | Unverified by log scan |

## In-Game Flight Recorder Summary (InteractionDiagnostics)
- **Schema:** `rustwrath.interaction-diagnostics.v1` v1 | **Mode:** `wrath_12340` | **Build SHA:** `c5d57f182ceb40921c76b26a18d62bde440ac7c8`
- **Events:** received=3310, evicted=2286, dropped=0, upstream_gaps=0

### Flight Recorder: Recorded Incidents
- `outcome=ServerReportedFailure confidence=Unmatched interaction=0 op=Observed quest= stage={"Cast":{"attempt":null,"edge":{"Interrupted":"@{reason=40}"},"spell_id":75,"wire_cast_count":1}}`
- `outcome=ObservedFailure confidence=Exact interaction=0 op=Observed quest= stage={"ScriptError":{"occurrences":1,"row_seq":42,"vm_session":5}}`
- `outcome=ObservedFailure confidence=Exact interaction=0 op=Observed quest= stage={"ScriptLoadError":{"occurrences":1,"row_seq":4,"vm_session":7}}`
- `outcome=ObservedFailure confidence=Exact interaction=0 op=Observed quest= stage={"ScriptError":{"occurrences":1,"row_seq":15,"vm_session":7}}`
- `outcome=ObservedFailure confidence=Exact interaction=0 op=Observed quest= stage={"ScriptError":{"occurrences":1,"row_seq":16,"vm_session":7}}`
- `outcome=ObservedFailure confidence=Exact interaction=0 op=Observed quest= stage={"ScriptError":{"occurrences":2,"row_seq":18,"vm_session":7}}`
- `outcome=ObservedFailure confidence=Exact interaction=0 op=Observed quest= stage={"ScriptError":{"occurrences":1,"row_seq":19,"vm_session":7}}`
- `outcome=ObservedFailure confidence=Exact interaction=0 op=Observed quest= stage={"ScriptError":{"occurrences":1,"row_seq":20,"vm_session":7}}`
- `outcome=ObservedFailure confidence=Exact interaction=0 op=Observed quest= stage={"ScriptError":{"occurrences":1,"row_seq":21,"vm_session":7}}`
- `outcome=ObservedFailure confidence=Exact interaction=0 op=Observed quest= stage={"ScriptError":{"occurrences":1,"row_seq":22,"vm_session":7}}`
- `outcome=ObservedFailure confidence=Exact interaction=0 op=Observed quest= stage={"ScriptError":{"occurrences":1,"row_seq":23,"vm_session":7}}`
- `outcome=ObservedFailure confidence=Exact interaction=0 op=Observed quest= stage={"ScriptError":{"occurrences":1,"row_seq":24,"vm_session":7}}`
- `outcome=ObservedFailure confidence=Exact interaction=0 op=Observed quest= stage={"ScriptError":{"occurrences":1,"row_seq":25,"vm_session":7}}`
- `outcome=ObservedFailure confidence=Exact interaction=0 op=Observed quest= stage={"ScriptError":{"occurrences":1,"row_seq":26,"vm_session":7}}`
- `outcome=ObservedFailure confidence=Exact interaction=0 op=Observed quest= stage={"ScriptError":{"occurrences":1,"row_seq":27,"vm_session":7}}`
- `outcome=ObservedFailure confidence=Exact interaction=0 op=Observed quest= stage={"ScriptError":{"occurrences":1,"row_seq":28,"vm_session":7}}`
- `outcome=ObservedFailure confidence=Exact interaction=0 op=Observed quest= stage={"ScriptError":{"occurrences":1,"row_seq":29,"vm_session":7}}`
- `outcome=ObservedFailure confidence=Exact interaction=0 op=Observed quest= stage={"ScriptError":{"occurrences":1,"row_seq":30,"vm_session":7}}`
- `outcome=ObservedFailure confidence=Exact interaction=0 op=Observed quest= stage={"ScriptError":{"occurrences":2,"row_seq":31,"vm_session":7}}`
- `outcome=ObservedFailure confidence=Exact interaction=0 op=Observed quest= stage={"ScriptError":{"occurrences":1,"row_seq":32,"vm_session":7}}`
- `outcome=ObservedFailure confidence=Exact interaction=0 op=Observed quest= stage={"ScriptError":{"occurrences":1,"row_seq":33,"vm_session":7}}`
- `outcome=ObservedFailure confidence=Exact interaction=0 op=Observed quest= stage={"ScriptError":{"occurrences":2,"row_seq":34,"vm_session":7}}`
- `outcome=ObservedFailure confidence=Exact interaction=0 op=Observed quest= stage={"ScriptError":{"occurrences":1,"row_seq":35,"vm_session":7}}`
- `outcome=ObservedFailure confidence=Exact interaction=0 op=Observed quest= stage={"ScriptError":{"occurrences":1,"row_seq":36,"vm_session":7}}`
- `outcome=ObservedFailure confidence=Exact interaction=0 op=Observed quest= stage={"ScriptError":{"occurrences":1,"row_seq":37,"vm_session":7}}`
- `outcome=ObservedFailure confidence=Exact interaction=0 op=Observed quest= stage={"AssetFailure":{"cause":"Reader","family":"Texture"}}`
- `outcome=ObservedFailure confidence=Exact interaction=0 op=Observed quest= stage={"AssetFailure":{"cause":"Reader","family":"Texture"}}`
- `outcome=ObservedFailure confidence=Exact interaction=0 op=Observed quest= stage={"AssetFailure":{"cause":"Reader","family":"Texture"}}`
- `outcome=ObservedFailure confidence=Exact interaction=0 op=Observed quest= stage={"ScriptError":{"occurrences":1,"row_seq":38,"vm_session":7}}`
- `outcome=LocalRefusal confidence=Exact interaction=18 op=Observed quest= stage={"Cast":{"attempt":null,"edge":{"LocalRefusal":"@{reason=60}"},"spell_id":32223,"wire_cast_count":null}}`
- `outcome=ObservedFailure confidence=Exact interaction=0 op=Observed quest= stage={"ScriptError":{"occurrences":1,"row_seq":39,"vm_session":7}}`
- `outcome=ObservedFailure confidence=Exact interaction=0 op=Observed quest= stage={"ScriptError":{"occurrences":1,"row_seq":40,"vm_session":7}}`
- `outcome=ObservedFailure confidence=Exact interaction=0 op=Observed quest= stage={"ScriptError":{"occurrences":1,"row_seq":41,"vm_session":7}}`
- `outcome=ObservedFailure confidence=Exact interaction=0 op=Observed quest= stage={"ScriptError":{"occurrences":1,"row_seq":42,"vm_session":7}}`
- `outcome=ObservedFailure confidence=Exact interaction=0 op=Observed quest= stage={"ScriptError":{"occurrences":1,"row_seq":43,"vm_session":7}}`
- `outcome=ObservedFailure confidence=Exact interaction=0 op=Observed quest= stage={"ScriptError":{"occurrences":1,"row_seq":46,"vm_session":7}}`
- `outcome=ObservedFailure confidence=Exact interaction=0 op=Observed quest= stage={"ScriptLoadError":{"occurrences":1,"row_seq":4,"vm_session":9}}`
- `outcome=ObservedFailure confidence=Exact interaction=0 op=Observed quest= stage={"ScriptError":{"occurrences":1,"row_seq":15,"vm_session":9}}`
- `outcome=ObservedFailure confidence=Exact interaction=0 op=Observed quest= stage={"ScriptError":{"occurrences":1,"row_seq":16,"vm_session":9}}`
- `outcome=ObservedFailure confidence=Exact interaction=0 op=Observed quest= stage={"ScriptError":{"occurrences":2,"row_seq":18,"vm_session":9}}`
- `outcome=ObservedFailure confidence=Exact interaction=0 op=Observed quest= stage={"ScriptError":{"occurrences":1,"row_seq":19,"vm_session":9}}`
- `outcome=ObservedFailure confidence=Exact interaction=0 op=Observed quest= stage={"ScriptError":{"occurrences":1,"row_seq":20,"vm_session":9}}`
- `outcome=ObservedFailure confidence=Exact interaction=0 op=Observed quest= stage={"ScriptError":{"occurrences":1,"row_seq":21,"vm_session":9}}`
- `outcome=ObservedFailure confidence=Exact interaction=0 op=Observed quest= stage={"ScriptError":{"occurrences":1,"row_seq":22,"vm_session":9}}`
- `outcome=ObservedFailure confidence=Exact interaction=0 op=Observed quest= stage={"ScriptError":{"occurrences":1,"row_seq":23,"vm_session":9}}`
- `outcome=ObservedFailure confidence=Exact interaction=0 op=Observed quest= stage={"ScriptError":{"occurrences":1,"row_seq":24,"vm_session":9}}`
- `outcome=ObservedFailure confidence=Exact interaction=0 op=Observed quest= stage={"ScriptError":{"occurrences":1,"row_seq":25,"vm_session":9}}`
- `outcome=ObservedFailure confidence=Exact interaction=0 op=Observed quest= stage={"ScriptError":{"occurrences":1,"row_seq":26,"vm_session":9}}`
- `outcome=ObservedFailure confidence=Exact interaction=0 op=Observed quest= stage={"ScriptError":{"occurrences":1,"row_seq":27,"vm_session":9}}`
- `outcome=ObservedFailure confidence=Exact interaction=0 op=Observed quest= stage={"ScriptError":{"occurrences":1,"row_seq":28,"vm_session":9}}`
- `outcome=ServerReportedFailure confidence=Unverified interaction=0 op=Observed quest= stage={"Cast":{"attempt":null,"edge":{"Failed":"@{reason=27}"},"spell_id":12494,"wire_cast_count":0}}`
- `outcome=LocalRefusal confidence=Unverified interaction=23 op=Observed quest= stage={"Cast":{"attempt":null,"edge":{"LocalRefusal":"@{reason=60}"},"spell_id":8407,"wire_cast_count":null}}`
- `outcome=LocalRefusal confidence=Unverified interaction=24 op=Observed quest= stage={"Cast":{"attempt":null,"edge":{"LocalRefusal":"@{reason=60}"},"spell_id":8407,"wire_cast_count":null}}`
- `outcome=ObservedFailure confidence=Exact interaction=0 op=Observed quest= stage={"ScriptError":{"occurrences":1,"row_seq":29,"vm_session":9}}`
- `outcome=ObservedFailure confidence=Exact interaction=0 op=Observed quest= stage={"ScriptError":{"occurrences":1,"row_seq":30,"vm_session":9}}`
- `outcome=ObservedFailure confidence=Exact interaction=0 op=Observed quest= stage={"ScriptError":{"occurrences":3,"row_seq":31,"vm_session":9}}`
- `outcome=LocalRefusal confidence=Unverified interaction=36 op=Observed quest= stage={"Cast":{"attempt":null,"edge":{"LocalRefusal":"@{reason=60}"},"spell_id":12824,"wire_cast_count":null}}`
- `outcome=LocalRefusal confidence=Unverified interaction=37 op=Observed quest= stage={"Cast":{"attempt":null,"edge":{"LocalRefusal":"@{reason=60}"},"spell_id":12824,"wire_cast_count":null}}`
- `outcome=LocalRefusal confidence=Unverified interaction=38 op=Observed quest= stage={"Cast":{"attempt":null,"edge":{"LocalRefusal":"@{reason=10}"},"spell_id":12824,"wire_cast_count":null}}`
- `outcome=LocalRefusal confidence=Unverified interaction=39 op=Observed quest= stage={"Cast":{"attempt":null,"edge":{"LocalRefusal":"@{reason=10}"},"spell_id":12824,"wire_cast_count":null}}`
- `outcome=LocalRefusal confidence=Unverified interaction=40 op=Observed quest= stage={"Cast":{"attempt":null,"edge":{"LocalRefusal":"@{reason=10}"},"spell_id":12824,"wire_cast_count":null}}`
- `outcome=LocalRefusal confidence=Unverified interaction=41 op=Observed quest= stage={"Cast":{"attempt":null,"edge":{"LocalRefusal":"@{reason=10}"},"spell_id":8402,"wire_cast_count":null}}`
- `outcome=ObservedFailure confidence=Exact interaction=0 op=Observed quest= stage={"ScriptError":{"occurrences":1,"row_seq":32,"vm_session":9}}`
- `outcome=WriteUnconfirmed confidence=Unverified interaction=34 op=Sell quest= stage={"SessionEnded":{"kind":"LoggedOut"}}`

### Flight Recorder: Action Summaries
- `Action 46 Observed (npc None, quest None, item None, count None, slot None): sent; server outcome unconfirmed; cast spell 8492 wire Some(8): no native evidence for this action (unverified); trace has gaps`
- `Action 45 Observed (npc None, quest None, item None, count None, slot None): sent; server outcome unconfirmed; cast spell 8422 wire Some(7): no native evidence for this action (unverified); trace has gaps`
- `Action 44 Observed (npc None, quest None, item None, count None, slot None): cast requested; delivery unconfirmed; cast spell 8422 wire None: no native evidence for this action (unverified); trace has gaps`
- `Action 43 Observed (npc None, quest None, item None, count None, slot None): sent; server outcome unconfirmed; cast spell 8422 wire Some(6): no native evidence for this action (unverified); trace has gaps`
- `Action 42 Observed (npc None, quest None, item None, count None, slot None): cast requested; delivery unconfirmed; cast spell 8422 wire None: no native evidence for this action (unverified); trace has gaps`
- `Action 41 Observed (npc None, quest None, item None, count None, slot None): blocked by a local guard (reason 0x0a); trace has gaps`
- `Action 40 Observed (npc None, quest None, item None, count None, slot None): blocked by a local guard (reason 0x0a); trace has gaps`
- `Action 39 Observed (npc None, quest None, item None, count None, slot None): blocked by a local guard (reason 0x0a); trace has gaps`

### Flight Recorder: Quest Panel Correlations
- `Progress panel (quest Some(194)) on npc Some(6): observed after action 33 (possible match); trace has gaps`
- `Gossip panel on npc Some(6): observed after action 33 (possible match); trace has gaps`
- `Progress panel (quest Some(340)) on npc Some(5): observed after action 32 (possible match); trace has gaps`
- `Gossip panel on npc Some(5): observed after action 32 (possible match); trace has gaps`
- `Progress panel (quest Some(186)) on npc Some(4): observed after action 31 (possible match); trace has gaps`
- `Progress panel (quest Some(191)) on npc Some(3): observed after action 30 (possible match); trace has gaps`

### Flight Recorder: Quest UI Correlations
- `Quest action 33 Open (npc Some(6), quest None): last milestone Delivery(Written); outcome in progress/unconfirmed [Unverified]; dispatch None; trace has gaps`
- `Quest action 32 Open (npc Some(5), quest None): last milestone Delivery(Written); outcome in progress/unconfirmed [Unverified]; dispatch None; trace has gaps`
- `Quest action 31 Open (npc Some(4), quest None): last milestone Delivery(Written); outcome in progress/unconfirmed [Unverified]; dispatch None; trace has gaps`
- `Quest action 30 Open (npc Some(3), quest None): last milestone Delivery(Written); outcome in progress/unconfirmed [Unverified]; dispatch None; trace has gaps`
- `UI dispatch 573: button Left target Other edge Release clicks Up (vm9 reload0): established click antecedent for this registration; no retained action or incident carries this click's id; pointer rows were lost (association unavailable); tr`
- `Journal: quest Some(6981) JournalRowInModel in reload generation 0 (current 0)`

### Flight Recorder: UI Error Correlations
- `UI error 32: pointer input within two seconds but no click antecedent for the edge that fires the handler (no causal claim); trace has gaps; pointer rows were lost`
- `UI error 32: pointer input within two seconds but no click antecedent for the edge that fires the handler (no causal claim); trace has gaps; pointer rows were lost`
- `UI error 32: no retained press or click in this VM within two seconds; pointer rows were lost (association unavailable); trace has gaps; pointer rows were lost`
- `UI error 31: one nearby click edge whose handler ran in the same VM; polled row is a repeat of a message already retained; possible association, polled once per second and message-keyed; trace has gaps; pointer rows were lost`
- `UI error 31: one nearby click edge whose handler ran in the same VM; polled row is a repeat of a message already retained; possible association, polled once per second and message-keyed; trace has gaps; pointer rows were lost`
- `UI error 31: pointer input within two seconds but no click antecedent for the edge that fires the handler (no causal claim); trace has gaps; pointer rows were lost`
- `UI error 31: no retained press or click in this VM within two seconds; pointer rows were lost (association unavailable); trace has gaps; pointer rows were lost`
- `UI error 31: no retained press or click in this VM within two seconds; pointer rows were lost (association unavailable); trace has gaps; pointer rows were lost`

### Flight Recorder: UI Click Outcomes
- `UI click 573 (release edge, vm9 reload0): handler ran; no retained action or incident carries this click's id; pointer rows were lost (association unavailable); trace has gaps; pointer rows were lost`
- `UI click 571 (release edge, vm9 reload0): no handler bound for this button edge (no Lua handler, hook or engine click action); trace has gaps; pointer rows were lost`
- `UI click 569 (release edge, vm9 reload0): click not run on this edge: the button was not pushed (disabled, hidden, never pressed, or the press was cleared); trace has gaps; pointer rows were lost`
- `UI click 567 (release edge, vm9 reload0): no handler bound for this button edge (no Lua handler, hook or engine click action); trace has gaps; pointer rows were lost`
- `UI click 565 (release edge, vm9 reload0): no handler bound for this button edge (no Lua handler, hook or engine click action); trace has gaps; pointer rows were lost`
- `UI click 563 (release edge, vm9 reload0): handler ran; no retained action or incident carries this click's id; pointer rows were lost (association unavailable); trace has gaps; pointer rows were lost`
- `UI click 561 (release edge, vm9 reload0): no handler bound for this button edge (no Lua handler, hook or engine click action); trace has gaps; pointer rows were lost`
- `UI click 559 (release edge, vm9 reload0): no handler bound for this button edge (no Lua handler, hook or engine click action); trace has gaps; pointer rows were lost`

### Flight Recorder: Cast Correlations
- `Cast action unknown (local action not retained) spell=8492 wire=8 attempt=9 scope=Some((2, 2)): Launched confidence=unverified; trace has gaps`
- `Cast action 46 spell=8492 wire=8 attempt=unknown scope=Some((2, 2)): Admitted confidence=unverified; no native evidence retained; trace has gaps`
- `Cast action unknown (local action not retained) spell=8422 wire=7 attempt=8 scope=Some((2, 2)): Launched confidence=unverified; trace has gaps`
- `Cast action 45 spell=8422 wire=7 attempt=unknown scope=Some((2, 2)): Admitted confidence=unverified; no native evidence retained; trace has gaps`
- `Cast action 44 spell=8422 wire=unknown attempt=unknown scope=Some((2, 2)): Requested confidence=unverified; no native evidence retained; trace has gaps`
- `Cast action unknown (local action not retained) spell=8422 wire=6 attempt=7 scope=Some((2, 2)): Launched confidence=unverified; trace has gaps`
- `Cast action 43 spell=8422 wire=6 attempt=unknown scope=Some((2, 2)): Admitted confidence=unverified; no native evidence retained; trace has gaps`
- `Cast action 42 spell=8422 wire=unknown attempt=unknown scope=Some((2, 2)): Requested confidence=unverified; no native evidence retained; trace has gaps`

### Flight Recorder: Loot & Vendor Correlations
- `Action 34 Sell (npc Some(7), item Some(4377), count None, slot Some(16)): sent; no matching server evidence in 30s (unverified); trace has gaps`
- `Action 25 Loot (npc Some(2), item None, count None, slot None): loot release observed (possible match); trace has gaps`
- `Action 16 Loot (npc Some(1), item Some(2934), count Some(1), slot Some(0)): loot release observed (ambiguous); trace has gaps`
- `Action 14 Loot (npc Some(1), item Some(769), count Some(1), slot Some(0)): loot release observed (ambiguous); trace has gaps`
- `Action 9 Equip (npc None, item Some(2100), count Some(1), slot Some(24)): equipment slot change observed (possible match); trace has gaps`

### Flight Recorder: Session Transitions
- `login attempt Some(0): last milestone RosterReceived`
- `last session phase: CharacterSelection (attempt Some(0), map None)`
- `most recent scope: epoch 2, world generation 2`
- `last failure: none retained`
- `last session end: LoggedOut`
- `last transfer: none retained`
- `last reload: none retained`
- `telemetry gaps: evicted=2286, dropped=0, tracking_lost=0, source_gaps=0; negative conclusions unverified`

## Player observations (reported, not automatically verified)
- No observations supplied.

## Diagnostic evidence by subsystem
### Geometry
- ``benilla_app::ui_script::input: ui_script(input): runtime error: ...rface\AddOns\Blizzard_TalentUI\Blizzard_TalentUI.lua:303: Couldn't find CVar named 'talentFrameShown'`` (x4; stderr.log:2062 through stderr.log:2855)
- ``benilla_assets::m2: M2 particle data unavailable for mpq://particles/bloodspurts/bloodspurt.m2: Wrath geometry/recursive particles remain guarded`` (x2; stderr.log:687 through stderr.log:3398)
- ``benilla_app::ui_script::extract: ui_script: runtime error: ...rface\AddOns\Blizzard_TalentUI\Blizzard_TalentUI.lua:303: Couldn't find CVar named 'talentFrameShown'`` (x2; stderr.log:2849 through stderr.log:2852)
- ``benilla_app::pipe_warm: pipeline compiled LIVE [154] label=pbr_alpha_blend_mesh_pipeline vs=embedded://benilla_assets/shaders/wow_model.wgsl vs_defs=[IRRADIANCE_VOLUMES_ARE_USABLE+MATERIAL_BIND_GROUP=3+MESH_PIPELINE+MULTIPLE_LIGHTMAPS_IN_ARRAY+MULTIPLE_LIGHT_PROBES_IN_ARRAY+SCREEN_SPACE_SPECULAR_TRANSMISSION_BLUR_TAPS=8+SHADOW_FILTER_METHOD_GAUSSIAN+VERTEX_NORMALS+VERTEX_OUTPUT_INSTANCE_INDEX+VERTEX_POSITIONS+VERTEX_UVS+VERTEX_UVS_A+VIEW_PROJECTION_PERSPECTIVE] bufs=[stride32@[0,1,2]] cull=None bias=0 depth_write=false cmp=GreaterEqual fs=embedded://benilla_assets/shaders/wow_model.wgsl fs_defs=[IRRADIANCE_VOLUMES_ARE_USABLE+MATERIAL_BIND_GROUP=3+MESH_PIPELINE+MULTIPLE_LIGHTMAPS_IN_ARRAY+MULTIPLE_LIGHT_PROBES_IN_ARRAY+SCREEN_SPACE_SPECULAR_TRANSMISSION_BLUR_TAPS=8+SHADOW_FILTER_METHOD_GAUSSIAN+VERTEX_NORMALS+VERTEX_OUTPUT_INSTANCE_INDEX+VERTEX_POSITIONS+VERTEX_UVS+VERTEX_UVS_A+VIEW_PROJECTION_PERSPECTIVE+WOW_WATER_CLIP] blend=Some(BlendState { color: BlendComponent { src_factor: SrcAlpha, dst_factor: OneMinusSrcAlpha, operation: Add }, alpha: BlendComponent { src_factor: One, dst_factor: OneMinusSrcAlpha, operation: Add } }) mask=ColorWrites(15) samples=1`` (x1; stderr.log:1480 through stderr.log:1480)
- ``benilla_app::pipe_warm: pipeline compiled LIVE [153] label=pbr_opaque_mesh_pipeline vs=embedded://benilla_assets/shaders/wow_model.wgsl vs_defs=[IRRADIANCE_VOLUMES_ARE_USABLE+MATERIAL_BIND_GROUP=3+MAY_DISCARD+MESH_PIPELINE+MULTIPLE_LIGHTMAPS_IN_ARRAY+MULTIPLE_LIGHT_PROBES_IN_ARRAY+SCREEN_SPACE_SPECULAR_TRANSMISSION_BLUR_TAPS=8+SHADOW_FILTER_METHOD_GAUSSIAN+VERTEX_NORMALS+VERTEX_OUTPUT_INSTANCE_INDEX+VERTEX_POSITIONS+VERTEX_UVS+VERTEX_UVS_A+VIEW_PROJECTION_NONSTANDARD+WOW_RIG_SKIN] bufs=[stride56@[0,1,2,10,11]] cull=Some(Back) bias=0 depth_write=true cmp=GreaterEqual fs=embedded://benilla_assets/shaders/wow_model.wgsl fs_defs=[IRRADIANCE_VOLUMES_ARE_USABLE+MATERIAL_BIND_GROUP=3+MAY_DISCARD+MESH_PIPELINE+MULTIPLE_LIGHTMAPS_IN_ARRAY+MULTIPLE_LIGHT_PROBES_IN_ARRAY+SCREEN_SPACE_SPECULAR_TRANSMISSION_BLUR_TAPS=8+SHADOW_FILTER_METHOD_GAUSSIAN+VERTEX_NORMALS+VERTEX_OUTPUT_INSTANCE_INDEX+VERTEX_POSITIONS+VERTEX_UVS+VERTEX_UVS_A+VIEW_PROJECTION_NONSTANDARD] blend=None mask=ColorWrites(15) samples=1`` (x1; stderr.log:1469 through stderr.log:1469)
- ``benilla_app::pipe_warm: pipeline compiled LIVE [1401] label=blit pipeline vs=embedded://bevy_core_pipeline\fullscreen_vertex_shader\fullscreen.wgsl vs_defs=[] bufs=[] cull=None bias=0 depth_write=false cmp=none fs=embedded://bevy_core_pipeline\blit\blit.wgsl fs_defs=[] blend=Some(BlendState { color: BlendComponent { src_factor: One, dst_factor: OneMinusSrcAlpha, operation: Add }, alpha: BlendComponent { src_factor: One, dst_factor: OneMinusSrcAlpha, operation: Add } }) mask=ColorWrites(15) samples=1`` (x1; stderr.log:3579 through stderr.log:3579)
- ``benilla_assets::m2: M2 particle data unavailable for mpq://spells/bestowdisease_impact_chest.m2: Wrath geometry/recursive particles remain guarded`` (x1; stderr.log:2048 through stderr.log:2048)
- ``benilla_app::pipe_warm: pipeline compiled LIVE [149] label=pbr_alpha_blend_mesh_pipeline vs=embedded://benilla_assets/shaders/wow_model.wgsl vs_defs=[IRRADIANCE_VOLUMES_ARE_USABLE+MATERIAL_BIND_GROUP=3+MESH_PIPELINE+MULTIPLE_LIGHTMAPS_IN_ARRAY+MULTIPLE_LIGHT_PROBES_IN_ARRAY+SCREEN_SPACE_SPECULAR_TRANSMISSION_BLUR_TAPS=8+SHADOW_FILTER_METHOD_GAUSSIAN+VERTEX_COLORS+VERTEX_NORMALS+VERTEX_OUTPUT_INSTANCE_INDEX+VERTEX_POSITIONS+VERTEX_UVS+VERTEX_UVS_A+VIEW_PROJECTION_PERSPECTIVE] bufs=[stride48@[0,1,2,5]] cull=Some(Back) bias=0 depth_write=false cmp=GreaterEqual fs=embedded://benilla_assets/shaders/wow_model.wgsl fs_defs=[IRRADIANCE_VOLUMES_ARE_USABLE+MATERIAL_BIND_GROUP=3+MESH_PIPELINE+MULTIPLE_LIGHTMAPS_IN_ARRAY+MULTIPLE_LIGHT_PROBES_IN_ARRAY+SCREEN_SPACE_SPECULAR_TRANSMISSION_BLUR_TAPS=8+SHADOW_FILTER_METHOD_GAUSSIAN+VERTEX_COLORS+VERTEX_NORMALS+VERTEX_OUTPUT_INSTANCE_INDEX+VERTEX_POSITIONS+VERTEX_UVS+VERTEX_UVS_A+VIEW_PROJECTION_PERSPECTIVE+WOW_WATER_CLIP] blend=Some(BlendState { color: BlendComponent { src_factor: One, dst_factor: One, operation: Add }, alpha: BlendComponent { src_factor: Zero, dst_factor: One, operation: Add } }) mask=ColorWrites(15) samples=1`` (x1; stderr.log:1425 through stderr.log:1425)
- ``benilla_app::pipe_warm: pipeline compiled LIVE [148] label=pbr_opaque_mesh_pipeline vs=embedded://benilla_assets/shaders/wow_model.wgsl vs_defs=[IRRADIANCE_VOLUMES_ARE_USABLE+MATERIAL_BIND_GROUP=3+MAY_DISCARD+MESH_PIPELINE+MULTIPLE_LIGHTMAPS_IN_ARRAY+MULTIPLE_LIGHT_PROBES_IN_ARRAY+SCREEN_SPACE_SPECULAR_TRANSMISSION_BLUR_TAPS=8+SHADOW_FILTER_METHOD_GAUSSIAN+VERTEX_NORMALS+VERTEX_OUTPUT_INSTANCE_INDEX+VERTEX_POSITIONS+VERTEX_UVS+VERTEX_UVS_A+VIEW_PROJECTION_PERSPECTIVE] bufs=[stride32@[0,1,2]] cull=None bias=0 depth_write=true cmp=GreaterEqual fs=embedded://benilla_assets/shaders/wow_model.wgsl fs_defs=[IRRADIANCE_VOLUMES_ARE_USABLE+MATERIAL_BIND_GROUP=3+MAY_DISCARD+MESH_PIPELINE+MULTIPLE_LIGHTMAPS_IN_ARRAY+MULTIPLE_LIGHT_PROBES_IN_ARRAY+SCREEN_SPACE_SPECULAR_TRANSMISSION_BLUR_TAPS=8+SHADOW_FILTER_METHOD_GAUSSIAN+VERTEX_NORMALS+VERTEX_OUTPUT_INSTANCE_INDEX+VERTEX_POSITIONS+VERTEX_UVS+VERTEX_UVS_A+VIEW_PROJECTION_PERSPECTIVE] blend=None mask=ColorWrites(15) samples=1`` (x1; stderr.log:1419 through stderr.log:1419)
- ``benilla_app::pipe_warm: pipeline compiled LIVE [147] label=pbr_opaque_mesh_pipeline vs=embedded://benilla_assets/shaders/wow_model.wgsl vs_defs=[IRRADIANCE_VOLUMES_ARE_USABLE+MATERIAL_BIND_GROUP=3+MESH_PIPELINE+MULTIPLE_LIGHTMAPS_IN_ARRAY+MULTIPLE_LIGHT_PROBES_IN_ARRAY+SCREEN_SPACE_SPECULAR_TRANSMISSION_BLUR_TAPS=8+SHADOW_FILTER_METHOD_GAUSSIAN+VERTEX_COLORS+VERTEX_NORMALS+VERTEX_OUTPUT_INSTANCE_INDEX+VERTEX_POSITIONS+VERTEX_UVS+VERTEX_UVS_A+VIEW_PROJECTION_PERSPECTIVE+WOW_RIG_SKIN] bufs=[stride72@[0,1,2,5,10,11]] cull=Some(Back) bias=0 depth_write=true cmp=GreaterEqual fs=embedded://benilla_assets/shaders/wow_model.wgsl fs_defs=[IRRADIANCE_VOLUMES_ARE_USABLE+MATERIAL_BIND_GROUP=3+MESH_PIPELINE+MULTIPLE_LIGHTMAPS_IN_ARRAY+MULTIPLE_LIGHT_PROBES_IN_ARRAY+SCREEN_SPACE_SPECULAR_TRANSMISSION_BLUR_TAPS=8+SHADOW_FILTER_METHOD_GAUSSIAN+VERTEX_COLORS+VERTEX_NORMALS+VERTEX_OUTPUT_INSTANCE_INDEX+VERTEX_POSITIONS+VERTEX_UVS+VERTEX_UVS_A+VIEW_PROJECTION_PERSPECTIVE] blend=None mask=ColorWrites(15) samples=1`` (x1; stderr.log:695 through stderr.log:695)
- ``benilla_app::pipe_warm: pipeline compiled LIVE [152] label=pbr_alpha_blend_mesh_pipeline vs=embedded://benilla_assets/shaders/wow_model.wgsl vs_defs=[IRRADIANCE_VOLUMES_ARE_USABLE+MATERIAL_BIND_GROUP=3+MESH_PIPELINE+MULTIPLE_LIGHTMAPS_IN_ARRAY+MULTIPLE_LIGHT_PROBES_IN_ARRAY+SCREEN_SPACE_SPECULAR_TRANSMISSION_BLUR_TAPS=8+SHADOW_FILTER_METHOD_GAUSSIAN+VERTEX_NORMALS+VERTEX_OUTPUT_INSTANCE_INDEX+VERTEX_POSITIONS+VERTEX_UVS+VERTEX_UVS_A+VIEW_PROJECTION_NONSTANDARD+WOW_RIG_SKIN] bufs=[stride56@[0,1,2,10,11]] cull=Some(Back) bias=0 depth_write=false cmp=GreaterEqual fs=embedded://benilla_assets/shaders/wow_model.wgsl fs_defs=[IRRADIANCE_VOLUMES_ARE_USABLE+MATERIAL_BIND_GROUP=3+MESH_PIPELINE+MULTIPLE_LIGHTMAPS_IN_ARRAY+MULTIPLE_LIGHT_PROBES_IN_ARRAY+SCREEN_SPACE_SPECULAR_TRANSMISSION_BLUR_TAPS=8+SHADOW_FILTER_METHOD_GAUSSIAN+VERTEX_NORMALS+VERTEX_OUTPUT_INSTANCE_INDEX+VERTEX_POSITIONS+VERTEX_UVS+VERTEX_UVS_A+VIEW_PROJECTION_NONSTANDARD+WOW_WATER_CLIP] blend=Some(BlendState { color: BlendComponent { src_factor: SrcAlpha, dst_factor: OneMinusSrcAlpha, operation: Add }, alpha: BlendComponent { src_factor: One, dst_factor: OneMinusSrcAlpha, operation: Add } }) mask=ColorWrites(15) samples=1`` (x1; stderr.log:1468 through stderr.log:1468)
- ``benilla_app::pipe_warm: pipeline compiled LIVE [151] label=pbr_alpha_blend_mesh_pipeline vs=embedded://benilla_assets/shaders/wow_model.wgsl vs_defs=[IRRADIANCE_VOLUMES_ARE_USABLE+MATERIAL_BIND_GROUP=3+MESH_PIPELINE+MULTIPLE_LIGHTMAPS_IN_ARRAY+MULTIPLE_LIGHT_PROBES_IN_ARRAY+SCREEN_SPACE_SPECULAR_TRANSMISSION_BLUR_TAPS=8+SHADOW_FILTER_METHOD_GAUSSIAN+VERTEX_NORMALS+VERTEX_OUTPUT_INSTANCE_INDEX+VERTEX_POSITIONS+VERTEX_UVS+VERTEX_UVS_A+VIEW_PROJECTION_NONSTANDARD] bufs=[stride32@[0,1,2]] cull=Some(Back) bias=0 depth_write=false cmp=GreaterEqual fs=embedded://benilla_assets/shaders/wow_model.wgsl fs_defs=[IRRADIANCE_VOLUMES_ARE_USABLE+MATERIAL_BIND_GROUP=3+MESH_PIPELINE+MULTIPLE_LIGHTMAPS_IN_ARRAY+MULTIPLE_LIGHT_PROBES_IN_ARRAY+SCREEN_SPACE_SPECULAR_TRANSMISSION_BLUR_TAPS=8+SHADOW_FILTER_METHOD_GAUSSIAN+VERTEX_NORMALS+VERTEX_OUTPUT_INSTANCE_INDEX+VERTEX_POSITIONS+VERTEX_UVS+VERTEX_UVS_A+VIEW_PROJECTION_NONSTANDARD+WOW_WATER_CLIP] blend=Some(BlendState { color: BlendComponent { src_factor: One, dst_factor: One, operation: Add }, alpha: BlendComponent { src_factor: Zero, dst_factor: One, operation: Add } }) mask=ColorWrites(15) samples=1`` (x1; stderr.log:1451 through stderr.log:1451)
- ``benilla_app::pipe_warm: pipeline compiled LIVE [150] label=pbr_alpha_blend_mesh_pipeline vs=embedded://benilla_assets/shaders/wow_model.wgsl vs_defs=[IRRADIANCE_VOLUMES_ARE_USABLE+MATERIAL_BIND_GROUP=3+MESH_PIPELINE+MULTIPLE_LIGHTMAPS_IN_ARRAY+MULTIPLE_LIGHT_PROBES_IN_ARRAY+SCREEN_SPACE_SPECULAR_TRANSMISSION_BLUR_TAPS=8+SHADOW_FILTER_METHOD_GAUSSIAN+VERTEX_COLORS+VERTEX_NORMALS+VERTEX_OUTPUT_INSTANCE_INDEX+VERTEX_POSITIONS+VERTEX_UVS+VERTEX_UVS_A+VIEW_PROJECTION_PERSPECTIVE] bufs=[stride48@[0,1,2,5]] cull=None bias=0 depth_write=false cmp=GreaterEqual fs=embedded://benilla_assets/shaders/wow_model.wgsl fs_defs=[IRRADIANCE_VOLUMES_ARE_USABLE+MATERIAL_BIND_GROUP=3+MESH_PIPELINE+MULTIPLE_LIGHTMAPS_IN_ARRAY+MULTIPLE_LIGHT_PROBES_IN_ARRAY+SCREEN_SPACE_SPECULAR_TRANSMISSION_BLUR_TAPS=8+SHADOW_FILTER_METHOD_GAUSSIAN+VERTEX_COLORS+VERTEX_NORMALS+VERTEX_OUTPUT_INSTANCE_INDEX+VERTEX_POSITIONS+VERTEX_UVS+VERTEX_UVS_A+VIEW_PROJECTION_PERSPECTIVE+WOW_WATER_CLIP] blend=Some(BlendState { color: BlendComponent { src_factor: One, dst_factor: One, operation: Add }, alpha: BlendComponent { src_factor: Zero, dst_factor: One, operation: Add } }) mask=ColorWrites(15) samples=1`` (x1; stderr.log:1444 through stderr.log:1444)

### Textures
- ``benilla_app::ui_script::extract: ui_script: runtime error: Interface\FrameXML\ChannelFrame.lua:716: attempt to index local `texture' (a nil value)`` (x8; stderr.log:639 through stderr.log:3362)
- ``benilla_app::ui_script::addons: ui_script(GlueXML/Interface\GlueXML\CharacterCreate.xml): Texture CharCreatePreviewFrameLeftEdge: Unable to load texture file Interface\Glues\CharacterCreate\Frame-Sides`` (x4; stderr.log:117 through stderr.log:2924)
- ``benilla_app::ui_script::addons: ui_script(GlueXML/Interface\GlueXML\CharacterCreate.xml): Texture CharacterCreateAlternateFormBottomPushedTexture: Unable to load texture file Interface\Glues\CharacterCreate\AlternateForm`` (x4; stderr.log:116 through stderr.log:2923)
- ``benilla_app::ui_script::addons: ui_script(GlueXML/Interface\GlueXML\CharacterCreate.xml): Texture CharacterCreateAlternateFormTopTop: Unable to load texture file Interface\Glues\CharacterCreate\AlternateForm`` (x4; stderr.log:111 through stderr.log:2918)
- ``benilla_app::ui_script::addons: ui_script(GlueXML/Interface\GlueXML\CharacterCreate.xml): Texture <unnamed>: Unable to load texture file Interface\Glues\CharacterCreate\CharacterCreate-Arrow`` (x4; stderr.log:118 through stderr.log:2925)
- ``benilla_app::ui_script::addons: ui_script(GlueXML/Interface\GlueXML\CharacterCreate.xml): Texture CharacterCreateAlternateFormTopPushedTexture: Unable to load texture file Interface\Glues\CharacterCreate\AlternateForm`` (x4; stderr.log:113 through stderr.log:2920)
- ``benilla_app::ui_script::addons: ui_script(GlueXML/Interface\GlueXML\CharacterCreate.xml): Texture CharacterCreateAlternateFormTopHighlight: Unable to load texture file Interface\Glues\CharacterCreate\AlternateForm`` (x4; stderr.log:112 through stderr.log:2919)
- ``benilla_app::ui_script::addons: ui_script(GlueXML/Interface\GlueXML\CharacterCreate.xml): Texture CharacterCreateAlternateFormBottomHighlight: Unable to load texture file Interface\Glues\CharacterCreate\AlternateForm`` (x4; stderr.log:115 through stderr.log:2922)
- ``benilla_app::ui_script::addons: ui_script(GlueXML/Interface\GlueXML\CharacterCreate.xml): Texture Bottom: Unable to load texture file Interface\Glues\CharacterCreate\AlternateForm`` (x4; stderr.log:114 through stderr.log:2921)
- ``benilla_assets::blp: BLP mpq://dungeons/textures/brian/abbey/piinetree01.blp: rejected mip 9; keeping 9 valid levels`` (x2; stderr.log:670 through stderr.log:2006)
- ``benilla_app::ui_script::extract: ui_script: runtime error: Interface\AddOns\bartender4\ActionButton.lua:295: attempt to index field `normalTexture' (a nil value)`` (x2; stderr.log:641 through stderr.log:653)
- ``bevy_asset::server: Encountered an I/O error while loading asset: file not in profile `wm-hd`: creature/nozdormudragon/nozdormu2.blp`` (x1; stderr.log:2773 through stderr.log:2773)
- ``bevy_asset::server: Encountered an I/O error while loading asset: file not in profile `wm-hd`: creature/nozdormudragon/nozdormu3.blp`` (x1; stderr.log:2772 through stderr.log:2772)
- ``benilla_app::pipe_warm: pipeline compiled LIVE [154] label=pbr_alpha_blend_mesh_pipeline vs=embedded://benilla_assets/shaders/wow_model.wgsl vs_defs=[IRRADIANCE_VOLUMES_ARE_USABLE+MATERIAL_BIND_GROUP=3+MESH_PIPELINE+MULTIPLE_LIGHTMAPS_IN_ARRAY+MULTIPLE_LIGHT_PROBES_IN_ARRAY+SCREEN_SPACE_SPECULAR_TRANSMISSION_BLUR_TAPS=8+SHADOW_FILTER_METHOD_GAUSSIAN+VERTEX_NORMALS+VERTEX_OUTPUT_INSTANCE_INDEX+VERTEX_POSITIONS+VERTEX_UVS+VERTEX_UVS_A+VIEW_PROJECTION_PERSPECTIVE] bufs=[stride32@[0,1,2]] cull=None bias=0 depth_write=false cmp=GreaterEqual fs=embedded://benilla_assets/shaders/wow_model.wgsl fs_defs=[IRRADIANCE_VOLUMES_ARE_USABLE+MATERIAL_BIND_GROUP=3+MESH_PIPELINE+MULTIPLE_LIGHTMAPS_IN_ARRAY+MULTIPLE_LIGHT_PROBES_IN_ARRAY+SCREEN_SPACE_SPECULAR_TRANSMISSION_BLUR_TAPS=8+SHADOW_FILTER_METHOD_GAUSSIAN+VERTEX_NORMALS+VERTEX_OUTPUT_INSTANCE_INDEX+VERTEX_POSITIONS+VERTEX_UVS+VERTEX_UVS_A+VIEW_PROJECTION_PERSPECTIVE+WOW_WATER_CLIP] blend=Some(BlendState { color: BlendComponent { src_factor: SrcAlpha, dst_factor: OneMinusSrcAlpha, operation: Add }, alpha: BlendComponent { src_factor: One, dst_factor: OneMinusSrcAlpha, operation: Add } }) mask=ColorWrites(15) samples=1`` (x1; stderr.log:1480 through stderr.log:1480)
- ``bevy_asset::server: Encountered an I/O error while loading asset: file not in profile `wm-hd`: creature/nozdormudragon/nozdormu1.blp`` (x1; stderr.log:2774 through stderr.log:2774)
- ``benilla_app::pipe_warm: pipeline compiled LIVE [1401] label=blit pipeline vs=embedded://bevy_core_pipeline\fullscreen_vertex_shader\fullscreen.wgsl vs_defs=[] bufs=[] cull=None bias=0 depth_write=false cmp=none fs=embedded://bevy_core_pipeline\blit\blit.wgsl fs_defs=[] blend=Some(BlendState { color: BlendComponent { src_factor: One, dst_factor: OneMinusSrcAlpha, operation: Add }, alpha: BlendComponent { src_factor: One, dst_factor: OneMinusSrcAlpha, operation: Add } }) mask=ColorWrites(15) samples=1`` (x1; stderr.log:3579 through stderr.log:3579)
- ``benilla_app::pipe_warm: pipeline compiled LIVE [1400] label=egui_pipeline vs=AssetId<bevy_shader::shader::Shader>{uuid: 05a4d7a0-4f24-4d7f-b606-3f399074261f} vs_defs=[BINDLESS=16] bufs=[stride20@[0,1,2]] cull=None bias=0 depth_write=false cmp=none fs=AssetId<bevy_shader::shader::Shader>{uuid: 05a4d7a0-4f24-4d7f-b606-3f399074261f} fs_defs=[BINDLESS=16] blend=Some(BlendState { color: BlendComponent { src_factor: One, dst_factor: OneMinusSrcAlpha, operation: Add }, alpha: BlendComponent { src_factor: One, dst_factor: OneMinusSrcAlpha, operation: Add } }) mask=ColorWrites(15) samples=1`` (x1; stderr.log:3578 through stderr.log:3578)
- ``benilla_app::entities::skin_composite: body skin composite finished without 1 layer(s): CompositeReport { missing: [CompositeMissingLayer { tile: (0, 0, 128, 64), candidates: ["Item\\TextureComponents\\ArmUpperTexture\\Robe_D_01Cyan_Sleeve_AU_U.blp", "Item\\TextureComponents\\ArmUpperTexture\\Robe_D_01Cyan_Sleeve_AU_F.blp"], errors: ["file not in patch chain: Item\\TextureComponents\\ArmUpperTexture\\Robe_D_01Cyan_Sleeve_AU_U.blp", "file not in patch chain: Item\\TextureComponents\\ArmUpperTexture\\Robe_D_01Cyan_Sleeve_AU_F.blp"] }] }`` (x1; stderr.log:2802 through stderr.log:2802)
- ``benilla_app::pipe_warm: pipeline compiled LIVE [153] label=pbr_opaque_mesh_pipeline vs=embedded://benilla_assets/shaders/wow_model.wgsl vs_defs=[IRRADIANCE_VOLUMES_ARE_USABLE+MATERIAL_BIND_GROUP=3+MAY_DISCARD+MESH_PIPELINE+MULTIPLE_LIGHTMAPS_IN_ARRAY+MULTIPLE_LIGHT_PROBES_IN_ARRAY+SCREEN_SPACE_SPECULAR_TRANSMISSION_BLUR_TAPS=8+SHADOW_FILTER_METHOD_GAUSSIAN+VERTEX_NORMALS+VERTEX_OUTPUT_INSTANCE_INDEX+VERTEX_POSITIONS+VERTEX_UVS+VERTEX_UVS_A+VIEW_PROJECTION_NONSTANDARD+WOW_RIG_SKIN] bufs=[stride56@[0,1,2,10,11]] cull=Some(Back) bias=0 depth_write=true cmp=GreaterEqual fs=embedded://benilla_assets/shaders/wow_model.wgsl fs_defs=[IRRADIANCE_VOLUMES_ARE_USABLE+MATERIAL_BIND_GROUP=3+MAY_DISCARD+MESH_PIPELINE+MULTIPLE_LIGHTMAPS_IN_ARRAY+MULTIPLE_LIGHT_PROBES_IN_ARRAY+SCREEN_SPACE_SPECULAR_TRANSMISSION_BLUR_TAPS=8+SHADOW_FILTER_METHOD_GAUSSIAN+VERTEX_NORMALS+VERTEX_OUTPUT_INSTANCE_INDEX+VERTEX_POSITIONS+VERTEX_UVS+VERTEX_UVS_A+VIEW_PROJECTION_NONSTANDARD] blend=None mask=ColorWrites(15) samples=1`` (x1; stderr.log:1469 through stderr.log:1469)
- ``benilla_app::pipe_warm: pipeline compiled LIVE [149] label=pbr_alpha_blend_mesh_pipeline vs=embedded://benilla_assets/shaders/wow_model.wgsl vs_defs=[IRRADIANCE_VOLUMES_ARE_USABLE+MATERIAL_BIND_GROUP=3+MESH_PIPELINE+MULTIPLE_LIGHTMAPS_IN_ARRAY+MULTIPLE_LIGHT_PROBES_IN_ARRAY+SCREEN_SPACE_SPECULAR_TRANSMISSION_BLUR_TAPS=8+SHADOW_FILTER_METHOD_GAUSSIAN+VERTEX_COLORS+VERTEX_NORMALS+VERTEX_OUTPUT_INSTANCE_INDEX+VERTEX_POSITIONS+VERTEX_UVS+VERTEX_UVS_A+VIEW_PROJECTION_PERSPECTIVE] bufs=[stride48@[0,1,2,5]] cull=Some(Back) bias=0 depth_write=false cmp=GreaterEqual fs=embedded://benilla_assets/shaders/wow_model.wgsl fs_defs=[IRRADIANCE_VOLUMES_ARE_USABLE+MATERIAL_BIND_GROUP=3+MESH_PIPELINE+MULTIPLE_LIGHTMAPS_IN_ARRAY+MULTIPLE_LIGHT_PROBES_IN_ARRAY+SCREEN_SPACE_SPECULAR_TRANSMISSION_BLUR_TAPS=8+SHADOW_FILTER_METHOD_GAUSSIAN+VERTEX_COLORS+VERTEX_NORMALS+VERTEX_OUTPUT_INSTANCE_INDEX+VERTEX_POSITIONS+VERTEX_UVS+VERTEX_UVS_A+VIEW_PROJECTION_PERSPECTIVE+WOW_WATER_CLIP] blend=Some(BlendState { color: BlendComponent { src_factor: One, dst_factor: One, operation: Add }, alpha: BlendComponent { src_factor: Zero, dst_factor: One, operation: Add } }) mask=ColorWrites(15) samples=1`` (x1; stderr.log:1425 through stderr.log:1425)
- ``benilla_app::pipe_warm: pipeline compiled LIVE [148] label=pbr_opaque_mesh_pipeline vs=embedded://benilla_assets/shaders/wow_model.wgsl vs_defs=[IRRADIANCE_VOLUMES_ARE_USABLE+MATERIAL_BIND_GROUP=3+MAY_DISCARD+MESH_PIPELINE+MULTIPLE_LIGHTMAPS_IN_ARRAY+MULTIPLE_LIGHT_PROBES_IN_ARRAY+SCREEN_SPACE_SPECULAR_TRANSMISSION_BLUR_TAPS=8+SHADOW_FILTER_METHOD_GAUSSIAN+VERTEX_NORMALS+VERTEX_OUTPUT_INSTANCE_INDEX+VERTEX_POSITIONS+VERTEX_UVS+VERTEX_UVS_A+VIEW_PROJECTION_PERSPECTIVE] bufs=[stride32@[0,1,2]] cull=None bias=0 depth_write=true cmp=GreaterEqual fs=embedded://benilla_assets/shaders/wow_model.wgsl fs_defs=[IRRADIANCE_VOLUMES_ARE_USABLE+MATERIAL_BIND_GROUP=3+MAY_DISCARD+MESH_PIPELINE+MULTIPLE_LIGHTMAPS_IN_ARRAY+MULTIPLE_LIGHT_PROBES_IN_ARRAY+SCREEN_SPACE_SPECULAR_TRANSMISSION_BLUR_TAPS=8+SHADOW_FILTER_METHOD_GAUSSIAN+VERTEX_NORMALS+VERTEX_OUTPUT_INSTANCE_INDEX+VERTEX_POSITIONS+VERTEX_UVS+VERTEX_UVS_A+VIEW_PROJECTION_PERSPECTIVE] blend=None mask=ColorWrites(15) samples=1`` (x1; stderr.log:1419 through stderr.log:1419)
- ``benilla_app::pipe_warm: pipeline compiled LIVE [147] label=pbr_opaque_mesh_pipeline vs=embedded://benilla_assets/shaders/wow_model.wgsl vs_defs=[IRRADIANCE_VOLUMES_ARE_USABLE+MATERIAL_BIND_GROUP=3+MESH_PIPELINE+MULTIPLE_LIGHTMAPS_IN_ARRAY+MULTIPLE_LIGHT_PROBES_IN_ARRAY+SCREEN_SPACE_SPECULAR_TRANSMISSION_BLUR_TAPS=8+SHADOW_FILTER_METHOD_GAUSSIAN+VERTEX_COLORS+VERTEX_NORMALS+VERTEX_OUTPUT_INSTANCE_INDEX+VERTEX_POSITIONS+VERTEX_UVS+VERTEX_UVS_A+VIEW_PROJECTION_PERSPECTIVE+WOW_RIG_SKIN] bufs=[stride72@[0,1,2,5,10,11]] cull=Some(Back) bias=0 depth_write=true cmp=GreaterEqual fs=embedded://benilla_assets/shaders/wow_model.wgsl fs_defs=[IRRADIANCE_VOLUMES_ARE_USABLE+MATERIAL_BIND_GROUP=3+MESH_PIPELINE+MULTIPLE_LIGHTMAPS_IN_ARRAY+MULTIPLE_LIGHT_PROBES_IN_ARRAY+SCREEN_SPACE_SPECULAR_TRANSMISSION_BLUR_TAPS=8+SHADOW_FILTER_METHOD_GAUSSIAN+VERTEX_COLORS+VERTEX_NORMALS+VERTEX_OUTPUT_INSTANCE_INDEX+VERTEX_POSITIONS+VERTEX_UVS+VERTEX_UVS_A+VIEW_PROJECTION_PERSPECTIVE] blend=None mask=ColorWrites(15) samples=1`` (x1; stderr.log:695 through stderr.log:695)
- ``benilla_app::pipe_warm: pipeline compiled LIVE [150] label=pbr_alpha_blend_mesh_pipeline vs=embedded://benilla_assets/shaders/wow_model.wgsl vs_defs=[IRRADIANCE_VOLUMES_ARE_USABLE+MATERIAL_BIND_GROUP=3+MESH_PIPELINE+MULTIPLE_LIGHTMAPS_IN_ARRAY+MULTIPLE_LIGHT_PROBES_IN_ARRAY+SCREEN_SPACE_SPECULAR_TRANSMISSION_BLUR_TAPS=8+SHADOW_FILTER_METHOD_GAUSSIAN+VERTEX_COLORS+VERTEX_NORMALS+VERTEX_OUTPUT_INSTANCE_INDEX+VERTEX_POSITIONS+VERTEX_UVS+VERTEX_UVS_A+VIEW_PROJECTION_PERSPECTIVE] bufs=[stride48@[0,1,2,5]] cull=None bias=0 depth_write=false cmp=GreaterEqual fs=embedded://benilla_assets/shaders/wow_model.wgsl fs_defs=[IRRADIANCE_VOLUMES_ARE_USABLE+MATERIAL_BIND_GROUP=3+MESH_PIPELINE+MULTIPLE_LIGHTMAPS_IN_ARRAY+MULTIPLE_LIGHT_PROBES_IN_ARRAY+SCREEN_SPACE_SPECULAR_TRANSMISSION_BLUR_TAPS=8+SHADOW_FILTER_METHOD_GAUSSIAN+VERTEX_COLORS+VERTEX_NORMALS+VERTEX_OUTPUT_INSTANCE_INDEX+VERTEX_POSITIONS+VERTEX_UVS+VERTEX_UVS_A+VIEW_PROJECTION_PERSPECTIVE+WOW_WATER_CLIP] blend=Some(BlendState { color: BlendComponent { src_factor: One, dst_factor: One, operation: Add }, alpha: BlendComponent { src_factor: Zero, dst_factor: One, operation: Add } }) mask=ColorWrites(15) samples=1`` (x1; stderr.log:1444 through stderr.log:1444)
- ``benilla_app::pipe_warm: pipeline compiled LIVE [152] label=pbr_alpha_blend_mesh_pipeline vs=embedded://benilla_assets/shaders/wow_model.wgsl vs_defs=[IRRADIANCE_VOLUMES_ARE_USABLE+MATERIAL_BIND_GROUP=3+MESH_PIPELINE+MULTIPLE_LIGHTMAPS_IN_ARRAY+MULTIPLE_LIGHT_PROBES_IN_ARRAY+SCREEN_SPACE_SPECULAR_TRANSMISSION_BLUR_TAPS=8+SHADOW_FILTER_METHOD_GAUSSIAN+VERTEX_NORMALS+VERTEX_OUTPUT_INSTANCE_INDEX+VERTEX_POSITIONS+VERTEX_UVS+VERTEX_UVS_A+VIEW_PROJECTION_NONSTANDARD+WOW_RIG_SKIN] bufs=[stride56@[0,1,2,10,11]] cull=Some(Back) bias=0 depth_write=false cmp=GreaterEqual fs=embedded://benilla_assets/shaders/wow_model.wgsl fs_defs=[IRRADIANCE_VOLUMES_ARE_USABLE+MATERIAL_BIND_GROUP=3+MESH_PIPELINE+MULTIPLE_LIGHTMAPS_IN_ARRAY+MULTIPLE_LIGHT_PROBES_IN_ARRAY+SCREEN_SPACE_SPECULAR_TRANSMISSION_BLUR_TAPS=8+SHADOW_FILTER_METHOD_GAUSSIAN+VERTEX_NORMALS+VERTEX_OUTPUT_INSTANCE_INDEX+VERTEX_POSITIONS+VERTEX_UVS+VERTEX_UVS_A+VIEW_PROJECTION_NONSTANDARD+WOW_WATER_CLIP] blend=Some(BlendState { color: BlendComponent { src_factor: SrcAlpha, dst_factor: OneMinusSrcAlpha, operation: Add }, alpha: BlendComponent { src_factor: One, dst_factor: OneMinusSrcAlpha, operation: Add } }) mask=ColorWrites(15) samples=1`` (x1; stderr.log:1468 through stderr.log:1468)
- ``benilla_app::portrait: booth target: 1 texture(s) never landed after 10s — releasing the wake hold with the still as-is`` (x1; stderr.log:1453 through stderr.log:1453)
- ``benilla_app::pipe_warm: pipeline compiled LIVE [151] label=pbr_alpha_blend_mesh_pipeline vs=embedded://benilla_assets/shaders/wow_model.wgsl vs_defs=[IRRADIANCE_VOLUMES_ARE_USABLE+MATERIAL_BIND_GROUP=3+MESH_PIPELINE+MULTIPLE_LIGHTMAPS_IN_ARRAY+MULTIPLE_LIGHT_PROBES_IN_ARRAY+SCREEN_SPACE_SPECULAR_TRANSMISSION_BLUR_TAPS=8+SHADOW_FILTER_METHOD_GAUSSIAN+VERTEX_NORMALS+VERTEX_OUTPUT_INSTANCE_INDEX+VERTEX_POSITIONS+VERTEX_UVS+VERTEX_UVS_A+VIEW_PROJECTION_NONSTANDARD] bufs=[stride32@[0,1,2]] cull=Some(Back) bias=0 depth_write=false cmp=GreaterEqual fs=embedded://benilla_assets/shaders/wow_model.wgsl fs_defs=[IRRADIANCE_VOLUMES_ARE_USABLE+MATERIAL_BIND_GROUP=3+MESH_PIPELINE+MULTIPLE_LIGHTMAPS_IN_ARRAY+MULTIPLE_LIGHT_PROBES_IN_ARRAY+SCREEN_SPACE_SPECULAR_TRANSMISSION_BLUR_TAPS=8+SHADOW_FILTER_METHOD_GAUSSIAN+VERTEX_NORMALS+VERTEX_OUTPUT_INSTANCE_INDEX+VERTEX_POSITIONS+VERTEX_UVS+VERTEX_UVS_A+VIEW_PROJECTION_NONSTANDARD+WOW_WATER_CLIP] blend=Some(BlendState { color: BlendComponent { src_factor: One, dst_factor: One, operation: Add }, alpha: BlendComponent { src_factor: Zero, dst_factor: One, operation: Add } }) mask=ColorWrites(15) samples=1`` (x1; stderr.log:1451 through stderr.log:1451)

### Quests
- ``benilla_app::ui_script::addons: ui_script(GlueXML/Interface\GlueXML\CharacterCreate.xml): Couldn't find inherited node: QuestFontNormalHuge`` (x44; stderr.log:99 through stderr.log:2917)
- ``benilla_app::ui_script::addons: ui_script: WMQuestieCompat is disabled — not loaded`` (x4; stderr.log:592 through stderr.log:3330)
- ``benilla_app::ui_script::addons: diag[ADDON] WMQuestieCompat is DISABLED in AddOn enable configuration`` (x4; stderr.log:593 through stderr.log:3331)
- ``benilla_app::ui_script::extract: ui_script: runtime error: [string "LFDQueueFrame:OnShow"]:5: attempt to call global `RequestLFDPartyLockInfo' (a nil value)`` (x4; stderr.log:1446 through stderr.log:2832)
- ``benilla_app::ui_script::addons: ui_script(FrameXML/Interface\FrameXML\WorldMapFrame.xml): Unknown frame type: QuestPOIFrame`` (x4; stderr.log:384 through stderr.log:3161)
- ``benilla_app::ui_script::addons: ui_script: Questie-335 is disabled — not loaded`` (x4; stderr.log:582 through stderr.log:3320)
- ``benilla_app::ui_script::addons: diag[ADDON] Questie-335 is DISABLED in AddOn enable configuration`` (x4; stderr.log:583 through stderr.log:3321)

### LootBags
- ``benilla_app::ui_script::extract: ui_script: runtime error: Interface\FrameXML\PaperDollFrame.lua:1793: attempt to call global `GetInventoryItemsForSlot' (a nil value)`` (x5; stderr.log:2050 through stderr.log:2059)
- ``benilla_app::ui_script::input: ui_script(input): runtime error: Interface\FrameXML\PaperDollFrame.lua:1793: attempt to call global `GetInventoryItemsForSlot' (a nil value)`` (x1; stderr.log:2073 through stderr.log:2073)

### Abilities
- ``benilla_app::ui_script::extract: ui_script: runtime error: Interface\FrameXML\OptionsPanelTemplates.lua:335: Couldn't find CVar named 'showArenaEnemyCastbar'`` (x4; stderr.log:646 through stderr.log:3358)
- ``benilla_app::ui_script::addons: diag[ADDON] combatlogfix is DISABLED in AddOn enable configuration`` (x4; stderr.log:528 through stderr.log:3266)
- ``benilla_app::ui_script::addons: ui_script: combatlogfix is disabled — not loaded`` (x4; stderr.log:527 through stderr.log:3265)
- ``benilla_app::ui_script::extract: ui_script: Post-hook: Interface\AddOns\bartender4\ActionButton.lua:453: attempt to index local `icon' (a nil value)`` (x4; stderr.log:640 through stderr.log:1428)
- ``benilla_app::ui_script::extract: ui_script: runtime error: Interface\FrameXML\OptionsPanelTemplates.lua:335: Couldn't find CVar named 'showCastableDebuffs'`` (x4; stderr.log:647 through stderr.log:3359)
- ``benilla_app::ui_script::input: ui_script(input): runtime error: Interface\FrameXML\PaperDollFrame.lua:919: attempt to call global `GetSpellBonusDamage' (a nil value)`` (x2; stderr.log:1463 through stderr.log:3409)
- ``benilla_app::ui_script::input: ui_script(input): runtime error: Interface\FrameXML\PaperDollFrame.lua:886: attempt to call global `GetCombatRatingBonus' (a nil value)`` (x2; stderr.log:1454 through stderr.log:1461)
- ``benilla_app::ui_script::extract: ui_script: runtime error: Interface\AddOns\bartender4\ActionButton.lua:295: attempt to index field `normalTexture' (a nil value)`` (x2; stderr.log:641 through stderr.log:653)
- ``benilla_app::spell::mods: spell_mods: dropping an out-of-range flat modifier — mask bit 83 (0..=63), op 1 (0..=28), value 6000`` (x2; stderr.log:198 through stderr.log:1591)
- ``benilla_app::spell::mods: spell_mods: dropping an out-of-range flat modifier — mask bit 76 (0..=63), op 18 (0..=28), value 10`` (x2; stderr.log:197 through stderr.log:1590)
- ``benilla_app::spell::mods: spell_mods: dropping an out-of-range pct modifier — mask bit 95 (0..=63), op 9 (0..=28), value 100`` (x2; stderr.log:196 through stderr.log:1589)
- ``benilla_app::spell::mods: spell_mods: dropping an out-of-range pct modifier — mask bit 76 (0..=63), op 3 (0..=28), value 30`` (x2; stderr.log:201 through stderr.log:1594)
- ``benilla_app::spell::mods: spell_mods: dropping an out-of-range flat modifier — mask bit 77 (0..=63), op 12 (0..=28), value -5`` (x2; stderr.log:200 through stderr.log:1593)
- ``benilla_app::spell::mods: spell_mods: dropping an out-of-range pct modifier — mask bit 67 (0..=63), op 11 (0..=28), value -20`` (x2; stderr.log:199 through stderr.log:1592)
- ``benilla_app::spell::mods: spell_mods: dropping an out-of-range flat modifier — mask bit 68 (0..=63), op 23 (0..=28), value -13`` (x1; stderr.log:2960 through stderr.log:2960)
- ``benilla_app::spell::mods: spell_mods: dropping an out-of-range flat modifier — mask bit 65 (0..=63), op 28 (0..=28), value 30`` (x1; stderr.log:2175 through stderr.log:2175)
- ``benilla_app::spell::mods: spell_mods: dropping an out-of-range flat modifier — mask bit 67 (0..=63), op 11 (0..=28), value -2000`` (x1; stderr.log:2174 through stderr.log:2174)
- ``benilla_app::spell::mods: spell_mods: dropping an out-of-range pct modifier — mask bit 72 (0..=63), op 2 (0..=28), value -20`` (x1; stderr.log:2171 through stderr.log:2171)
- ``benilla_app::spell::mods: spell_mods: dropping an out-of-range pct modifier — mask bit 67 (0..=63), op 14 (0..=28), value -10`` (x1; stderr.log:2172 through stderr.log:2172)
- ``benilla_app::spell::mods: spell_mods: dropping an out-of-range flat modifier — mask bit 89 (0..=63), op 12 (0..=28), value 4`` (x1; stderr.log:2173 through stderr.log:2173)
- ``benilla_app::spell::mods: spell_mods: dropping an out-of-range pct modifier — mask bit 69 (0..=63), op 15 (0..=28), value 100`` (x1; stderr.log:2961 through stderr.log:2961)
- ``benilla_assets::m2: M2 particle data unavailable for mpq://spells/frost_nova_area.m2: Wrath anisotropic/variable scale requires runtime support`` (x1; stderr.log:3572 through stderr.log:3572)
- ``benilla_app::spell::mods: spell_mods: dropping an out-of-range flat modifier — mask bit 68 (0..=63), op 23 (0..=28), value 0`` (x1; stderr.log:3592 through stderr.log:3592)
- ``benilla_app::spell::mods: spell_mods: dropping an out-of-range pct modifier — mask bit 69 (0..=63), op 15 (0..=28), value 0`` (x1; stderr.log:3593 through stderr.log:3593)
- ``benilla_app::spell::mods: spell_mods: dropping an out-of-range pct modifier — mask bit 64 (0..=63), op 14 (0..=28), value -50`` (x1; stderr.log:2962 through stderr.log:2962)
- ``benilla_app::preflight: preflight: IN COMBAT on arrival — something is already fighting this character. Do not leave it unattended (docs/METHOD.md's unattended-combat ban): break the fight or move it out.`` (x1; stderr.log:3385 through stderr.log:3385)
- ``benilla_assets::m2: M2 particle data unavailable for mpq://spells/frostbolt.m2: Wrath lifespan/emission variation requires runtime support`` (x1; stderr.log:3404 through stderr.log:3404)
- ``benilla_assets::m2: M2 particle data unavailable for mpq://spells/hunter_rapidfire.m2: Wrath anisotropic/variable scale requires runtime support`` (x1; stderr.log:1440 through stderr.log:1440)
- ``benilla_app::ui_script::input: ui_script(input): runtime error: Interface\FrameXML\PaperDollFrame.lua:646: attempt to call global `GetCombatRatingBonus' (a nil value)`` (x1; stderr.log:1459 through stderr.log:1459)
- ``benilla_assets::m2: M2 particle data unavailable for mpq-loose://interface/buttons/ui-autocastbutton.m2: Wrath spline particles remain unsupported`` (x1; stderr.log:167 through stderr.log:167)
- ``benilla_app::ui_script::extract: ui_script: Post-hook: Interface\AddOns\bartender4\ActionButton.lua:377: attempt to compare nil with number`` (x1; stderr.log:668 through stderr.log:668)
- ``benilla_assets::m2: M2 particle data unavailable for mpq://spells/lightningbolt_impact_chest.m2: Wrath lifespan/emission variation requires runtime support`` (x1; stderr.log:1421 through stderr.log:1421)
- ``benilla_assets::m2: M2 particle data unavailable for mpq://spells/lightning_precast_low_hand.m2: Wrath lifespan/emission variation requires runtime support`` (x1; stderr.log:1422 through stderr.log:1422)
- ``benilla_app::ui_script::input: ui_script(input): runtime error: Interface\FrameXML\PaperDollFrame.lua:465: attempt to call global `GetCombatRatingBonus' (a nil value)`` (x1; stderr.log:1465 through stderr.log:1465)
- ``benilla_app::spell::mods: spell_mods: dropping an out-of-range flat modifier — mask bit 76 (0..=63), op 18 (0..=28), value 0`` (x1; stderr.log:2077 through stderr.log:2077)
- ``benilla_app::spell::mods: spell_mods: dropping an out-of-range flat modifier — mask bit 83 (0..=63), op 1 (0..=28), value 0`` (x1; stderr.log:2078 through stderr.log:2078)
- ``benilla_app::spell::mods: spell_mods: dropping an out-of-range flat modifier — mask bit 75 (0..=63), op 21 (0..=28), value -500`` (x1; stderr.log:2170 through stderr.log:2170)
- ``benilla_app::wrath: wrath: SMSG_INITIAL_SPELLS declared 3 cooldowns but wrote 1; using the entries present`` (x1; stderr.log:1588 through stderr.log:1588)
- ``benilla_assets::m2: M2 particle data unavailable for mpq://spells/poisonshot_missile.m2: Wrath anisotropic/variable scale requires runtime support`` (x1; stderr.log:2046 through stderr.log:2046)
- ``benilla_assets::m2: M2 particle data unavailable for mpq://spells/bestowdisease_impact_chest.m2: Wrath geometry/recursive particles remain guarded`` (x1; stderr.log:2048 through stderr.log:2048)

### UI
- ``benilla_app::ui_script::extract: ui_script: error handler itself failed: runtime error: Interface\FrameXML\UIPanelTemplates.lua:365: attempt to perform arithmetic on field `cursorOffset' (a nil value)`` (x490; stderr.log:658 through stderr.log:3589)
- ``benilla_app::ui_script::extract: ui_script: runtime error: Interface\AddOns\bartender4\libs\SimpleSticky.lua:179: attempt to perform arithmetic on local `xB' (a nil value)`` (x348; stderr.log:704 through stderr.log:1401)
- ``benilla_app::ui_script::addons: ui_script(FrameXML/Interface\FrameXML\ChannelFrame.xml): unknown template 'VoiceChatSpeakerTemplate' referenced by inherits; skipping`` (x168; stderr.log:337 through stderr.log:3155)
- ``benilla_app::ui_script::extract: ui_script: runtime error: Interface\FrameXML\MoneyFrame.lua:274: attempt to perform arithmetic on local `money' (a nil value)`` (x48; stderr.log:3456 through stderr.log:3570)
- ``benilla_app::ui_script::addons: ui_script(GlueXML/Interface\GlueXML\CharacterCreate.xml): Couldn't find inherited node: QuestFontNormalHuge`` (x44; stderr.log:99 through stderr.log:2917)
- ``benilla_app::ui_script::addons: ui_script(FrameXML/Interface\FrameXML\PartyFrame.xml): unknown template 'VoiceChatSpeakerTemplate' referenced by inherits; skipping`` (x32; stderr.log:300 through stderr.log:3084)
- ``benilla_app::ui_script::extract: ui_script: unknown template 'VoiceChatSpeakerTemplate' referenced by inherits; skipping`` (x20; stderr.log:661 through stderr.log:3369)
- ``benilla_app::ui_script::input: ui_script(input): runtime error: Interface\FrameXML\MoneyFrame.lua:274: attempt to perform arithmetic on local `money' (a nil value)`` (x15; stderr.log:3446 through stderr.log:3569)
- ``benilla_app::ui_script::input: ui_script(input): runtime error: Interface\FrameXML\RestrictedFrames.lua:99: attempt to call method `IsProtected' (a nil value)`` (x13; stderr.log:698 through stderr.log:1490)
- ``benilla_app::ui_script::extract: ui_script: runtime error: Interface\FrameXML\PaperDollFrame.lua:280: attempt to call global `GetUnitMaxHealthModifier' (a nil value)`` (x10; stderr.log:638 through stderr.log:3407)
- ``benilla_app::ui_script::extract: ui_script: runtime error: Interface\FrameXML\BattlefieldFrame.lua:509: attempt to call global `GetRandomBGHonorCurrencyBonuses' (a nil value)`` (x9; stderr.log:691 through stderr.log:3424)
- ``benilla_app::ui_script::lifecycle: ui_script: Wrath host source integration is incomplete; addon/API and rendered acceptance remain unverified`` (x9; stderr.log:77 through stderr.log:3003)
- ``benilla_app::ui_script::extract: ui_script: runtime error: Interface\FrameXML\ChannelFrame.lua:716: attempt to index local `texture' (a nil value)`` (x8; stderr.log:639 through stderr.log:3362)
- ``benilla_app::ui_script::extract: ui_script: SetOwner: unknown anchor 'ANCHOR_BOTTOM' (mode 0, ANCHOR_LEFT)`` (x7; stderr.log:2065 through stderr.log:2072)
- ``benilla_app::bindings: bindings(TOGGLEACHIEVEMENT): runtime error: Interface\FrameXML\UIParent.lua:352: attempt to call global `CanShowAchievementUI' (a nil value)`` (x7; stderr.log:1426 through stderr.log:2854)
- ``benilla_app::ui_script::input: ui_script(input): runtime error: Interface\AddOns\Bagnon\components\item.lua:27: attempt to call method `GetDummyBag' (a nil value)`` (x6; stderr.log:1406 through stderr.log:1417)
- ``benilla_app::ui_script::extract: ui_script: runtime error: Interface\FrameXML\PetFrame.lua:71: attempt to call global `UnitInVehicle' (a nil value)`` (x5; stderr.log:654 through stderr.log:2576)
- ``benilla_app::ui_script::extract: ui_script: runtime error: Interface\FrameXML\PetStable.lua:199: attempt to call global `IsAtStableMaster' (a nil value)`` (x5; stderr.log:655 through stderr.log:2577)
- ``benilla_app::ui_script::extract: ui_script: runtime error: Interface\FrameXML\RestrictedFrames.lua:99: attempt to call method `IsProtected' (a nil value)`` (x5; stderr.log:632 through stderr.log:636)
- ``benilla_app::ui_script::extract: ui_script: runtime error: Interface\FrameXML\PVPFrame.lua:88: attempt to call global `GetCurrentArenaSeason' (a nil value)`` (x5; stderr.log:651 through stderr.log:3363)
- ``benilla_app::ui_script::input: ui_script(input): runtime error: [string "MiniMapTrackingButton:OnEnter"]:3: attempt to call method `SetTracking' (a nil value)`` (x5; stderr.log:696 through stderr.log:3588)
- ``benilla_app::ui_script::extract: ui_script: runtime error: Interface\FrameXML\PaperDollFrame.lua:1793: attempt to call global `GetInventoryItemsForSlot' (a nil value)`` (x5; stderr.log:2050 through stderr.log:2059)
- ``benilla_app::ui_script::addons: diag[ADDON] pawn is DISABLED in AddOn enable configuration`` (x4; stderr.log:577 through stderr.log:3315)
- ``benilla_app::ui_script::addons: ui_script: pawn is disabled — not loaded`` (x4; stderr.log:576 through stderr.log:3314)
- ``benilla_app::ui_script::addons: diag[ADDON] Quartz is DISABLED in AddOn enable configuration`` (x4; stderr.log:581 through stderr.log:3319)
- ``benilla_app::ui_script::addons: ui_script: Questie-335 is disabled — not loaded`` (x4; stderr.log:582 through stderr.log:3320)
- ``benilla_app::ui_script::addons: diag[ADDON] newlevelframe is DISABLED in AddOn enable configuration`` (x4; stderr.log:575 through stderr.log:3313)
- ``benilla_app::ui_script::addons: diag[ADDON] Routes is DISABLED in AddOn enable configuration`` (x4; stderr.log:585 through stderr.log:3323)
- ``benilla_app::ui_script::addons: diag[ADDON] Questie-335 is DISABLED in AddOn enable configuration`` (x4; stderr.log:583 through stderr.log:3321)
- ``benilla_app::ui_script::addons: ui_script: Routes is disabled — not loaded`` (x4; stderr.log:584 through stderr.log:3322)
- ``benilla_app::ui_script::addons: ui_script: postal is disabled — not loaded`` (x4; stderr.log:578 through stderr.log:3316)
- ``benilla_app::ui_script::addons: diag[ADDON] postal is DISABLED in AddOn enable configuration`` (x4; stderr.log:579 through stderr.log:3317)
- ``benilla_app::ui_script::addons: ui_script: Quartz is disabled — not loaded`` (x4; stderr.log:580 through stderr.log:3318)
- ``benilla_app::ui_script::addons: ui_script: newlevelframe is disabled — not loaded`` (x4; stderr.log:574 through stderr.log:3312)
- ``benilla_app::ui_script::addons: ui_script: handynotes is disabled — not loaded`` (x4; stderr.log:560 through stderr.log:3298)
- ``benilla_app::ui_script::addons: ui_script: libstratafix is disabled — not loaded`` (x4; stderr.log:566 through stderr.log:3304)
- ``benilla_app::ui_script::addons: diag[ADDON] libstratafix is DISABLED in AddOn enable configuration`` (x4; stderr.log:567 through stderr.log:3305)
- ``benilla_app::ui_script::addons: diag[ADDON] handynotes is DISABLED in AddOn enable configuration`` (x4; stderr.log:561 through stderr.log:3299)
- ``benilla_app::ui_script::addons: diag[ADDON] ignoremore is DISABLED in AddOn enable configuration`` (x4; stderr.log:563 through stderr.log:3301)
- ``benilla_app::ui_script::addons: ui_script: leatrix_gfx is disabled — not loaded`` (x4; stderr.log:564 through stderr.log:3302)
- ``benilla_app::ui_script::addons: diag[ADDON] leatrix_gfx is DISABLED in AddOn enable configuration`` (x4; stderr.log:565 through stderr.log:3303)
- ``benilla_app::ui_script::addons: diag[ADDON] mapster is DISABLED in AddOn enable configuration`` (x4; stderr.log:571 through stderr.log:3309)
- ``benilla_app::ui_script::addons: ui_script: MultiBot is disabled — not loaded`` (x4; stderr.log:572 through stderr.log:3310)
- ``benilla_app::ui_script::addons: diag[ADDON] MultiBot is DISABLED in AddOn enable configuration`` (x4; stderr.log:573 through stderr.log:3311)
- ``benilla_app::ui_script::addons: ui_script: mapster is disabled — not loaded`` (x4; stderr.log:570 through stderr.log:3308)
- ``benilla_app::ui_script::addons: ui_script: ignoremore is disabled — not loaded`` (x4; stderr.log:562 through stderr.log:3300)
- ``benilla_app::ui_script::addons: ui_script: MacroBank is disabled — not loaded`` (x4; stderr.log:568 through stderr.log:3306)
- ``benilla_app::ui_script::addons: diag[ADDON] MacroBank is DISABLED in AddOn enable configuration`` (x4; stderr.log:569 through stderr.log:3307)
- ``benilla_app::ui_script::extract: ui_script: runtime error: Interface\FrameXML\OptionsPanelTemplates.lua:335: Couldn't find CVar named 'showArenaEnemyCastbar'`` (x4; stderr.log:646 through stderr.log:3358)
- ``benilla_app::ui_script::extract: ui_script: runtime error: Interface\FrameXML\OptionsPanelTemplates.lua:335: Couldn't find CVar named 'showCastableDebuffs'`` (x4; stderr.log:647 through stderr.log:3359)
- ``benilla_app::ui_script::extract: ui_script: runtime error: Interface\FrameXML\GameTime.lua:158: attempt to compare nil with number`` (x4; stderr.log:648 through stderr.log:3360)
- ``benilla_app::ui_script::extract: ui_script: runtime error: Interface\FrameXML\OptionsPanelTemplates.lua:335: Couldn't find CVar named 'showItemLevel'`` (x4; stderr.log:643 through stderr.log:3355)
- ``benilla_app::ui_script::extract: ui_script: runtime error: Interface\FrameXML\OptionsPanelTemplates.lua:335: Couldn't find CVar named 'UnitNameFriendlyTotemName'`` (x4; stderr.log:644 through stderr.log:3356)
- ``benilla_app::ui_script::extract: ui_script: runtime error: Interface\FrameXML\OptionsPanelTemplates.lua:335: Couldn't find CVar named 'fctDamageReduction'`` (x4; stderr.log:645 through stderr.log:3357)
- ``benilla_app::ui_script::extract: ui_script: runtime error: Interface\FrameXML\PlayerFrame.lua:372: attempt to call global `VoiceChat_Animate' (a nil value)`` (x4; stderr.log:649 through stderr.log:3361)
- ``benilla_app::ui_script::extract: ui_script: runtime error: Interface\FrameXML\PlayerFrame.lua:64: attempt to call global `HasLFGRestrictions' (a nil value)`` (x4; stderr.log:2578 through stderr.log:2869)
- ``benilla_app::ui_script::extract: ui_script: runtime error: Interface\FrameXML\PartyMemberFrame.lua:256: attempt to call global `GetVoiceStatus' (a nil value)`` (x4; stderr.log:2579 through stderr.log:2864)
- ``benilla_app::ui_script::extract: ui_script: runtime error: Interface\FrameXML\RaidFrame.lua:70: attempt to call global `HasLFGRestrictions' (a nil value)`` (x4; stderr.log:2581 through stderr.log:2870)
- ``benilla_app::ui_script::extract: ui_script: runtime error: Interface\FrameXML\ReputationFrame.lua:167: attempt to call global `GetText' (a nil value)`` (x4; stderr.log:1433 through stderr.log:2840)
- ``benilla_app::ui_script::extract: ui_script: runtime error: [string "LFDQueueFrame:OnShow"]:5: attempt to call global `RequestLFDPartyLockInfo' (a nil value)`` (x4; stderr.log:1446 through stderr.log:2832)
- ``benilla_app::ui_script::input: ui_script(input): runtime error: ...rface\AddOns\Blizzard_TalentUI\Blizzard_TalentUI.lua:303: Couldn't find CVar named 'talentFrameShown'`` (x4; stderr.log:2062 through stderr.log:2855)
- ``benilla_app::ui_script::addons: diag[ADDON] Specialni Znaky is DISABLED in AddOn enable configuration`` (x4; stderr.log:589 through stderr.log:3327)
- ``benilla_app::ui_script::addons: ui_script: tweakwow is disabled — not loaded`` (x4; stderr.log:590 through stderr.log:3328)
- ``benilla_app::ui_script::addons: diag[ADDON] tweakwow is DISABLED in AddOn enable configuration`` (x4; stderr.log:591 through stderr.log:3329)
- ``benilla_app::ui_script::addons: ui_script: Skillet is disabled — not loaded`` (x4; stderr.log:586 through stderr.log:3324)
- ``benilla_app::ui_script::addons: diag[ADDON] Skillet is DISABLED in AddOn enable configuration`` (x4; stderr.log:587 through stderr.log:3325)
- ``benilla_app::ui_script::addons: ui_script: Specialni Znaky is disabled — not loaded`` (x4; stderr.log:588 through stderr.log:3326)
- ``benilla_app::ui_script::addons: ui_script: WMQuestieCompat is disabled — not loaded`` (x4; stderr.log:592 through stderr.log:3330)
- ``benilla_app::ui_script::extract: ui_script: runtime error: Interface\FrameXML\InterfaceOptionsFrame.lua:359: Couldn't find CVar named 'fctAuras'`` (x4; stderr.log:637 through stderr.log:3351)
- ``benilla_app::ui_script::extract: ui_script: Post-hook: Interface\AddOns\bartender4\ActionButton.lua:453: attempt to index local `icon' (a nil value)`` (x4; stderr.log:640 through stderr.log:1428)
- ``benilla_app::ui_script::extract: ui_script: runtime error: Interface\FrameXML\OptionsPanelTemplates.lua:335: Couldn't find CVar named 'showTargetOfTarget'`` (x4; stderr.log:642 through stderr.log:3354)
- ``benilla_app::ui_script::addons: diag[ADDON] WMQuestieCompat is DISABLED in AddOn enable configuration`` (x4; stderr.log:593 through stderr.log:3331)
- ``benilla_app::ui_script::addons: ui_script: zzzmopitemtooltips is disabled — not loaded`` (x4; stderr.log:604 through stderr.log:3341)
- ``benilla_app::ui_script::addons: diag[ADDON] zzzmopitemtooltips is DISABLED in AddOn enable configuration`` (x4; stderr.log:605 through stderr.log:3342)
- ``benilla_app::ui_script::addons: diag[ADDON] gearscorelite is DISABLED in AddOn enable configuration`` (x4; stderr.log:559 through stderr.log:3297)
- ``benilla_app::ui_script::addons: ui_script(FrameXML/Interface\FrameXML\KnowledgeBaseFrame.xml): KnowledgeBaseErrorFrame: text="ERROR" is shaped like a GlobalStrings key but no such string global exists — showing the key`` (x4; stderr.log:394 through stderr.log:3171)
- ``benilla_app::ui_script::addons: ui_script(FrameXML/Interface\FrameXML\KnowledgeBaseFrame.xml): KnowledgeBaseArticleListFrame: text="ARTICLE_COUNT" is shaped like a GlobalStrings key but no such string global exists — showing the key`` (x4; stderr.log:393 through stderr.log:3170)
- ``benilla_app::ui_script::addons: ui_script(FrameXML/Interface\FrameXML\PVPBattlegroundFrame.xml): PVPBattlegroundFrameInfoScrollFrameChildFrame: text="BATTLEGROUND_LORETEXT_1" is shaped like a GlobalStrings key but no such string global exists — showing the key`` (x4; stderr.log:407 through stderr.log:3184)
- ``benilla_app::ui_script::addons: ui_script(FrameXML/Interface\FrameXML\PVPFrame.xml): PVPFrameOffSeason: text="CANHAZDUMMYTEXT" is shaped like a GlobalStrings key but no such string global exists — showing the key`` (x4; stderr.log:405 through stderr.log:3182)
- ``benilla_app::ui_script::addons: ui_script(FrameXML/Interface\FrameXML\HonorFrame.xml): HonorFrame: text="HONOR_THISWEEK" is shaped like a GlobalStrings key but no such string global exists — showing the key`` (x4; stderr.log:320 through stderr.log:3097)
- ``benilla_app::ui_script::addons: ui_script(FrameXML/Interface\FrameXML\PlayerFrame.xml): unknown template 'VoiceChatSpeakerTemplate' referenced by inherits; skipping`` (x4; stderr.log:298 through stderr.log:3075)
- ``benilla_app::ui_script::addons: ui_script(FrameXML/Interface\FrameXML\WorldMapFrame.xml): Unknown frame type: QuestPOIFrame`` (x4; stderr.log:384 through stderr.log:3161)
- ``benilla_app::ui_script::addons: ui_script(FrameXML/Interface\FrameXML\RaidFrame.xml): RaidInfoInstanceLabel: text="TEST" is shaped like a GlobalStrings key but no such string global exists — showing the key`` (x4; stderr.log:335 through stderr.log:3112)
- ``benilla_app::ui_script::addons: diag[ADDON] !blizzbugssuck is DISABLED in AddOn enable configuration`` (x4; stderr.log:471 through stderr.log:3248)
- ``benilla_app::ui_script::addons: ui_script: !blizzbugssuck is disabled — not loaded`` (x4; stderr.log:470 through stderr.log:3247)
- ``benilla_app::ui_script::addons: diag[ADDON] aux-addon is DISABLED in AddOn enable configuration`` (x4; stderr.log:473 through stderr.log:3250)
- ``benilla_app::ui_script::addons: ui_script: aux-addon is disabled — not loaded`` (x4; stderr.log:472 through stderr.log:3249)
- ``benilla_app::ui_script::addons: diag[ADDON] WMBridge is DISABLED in AddOn enable configuration`` (x4; stderr.log:433 through stderr.log:3210)
- ``benilla_app::ui_script::addons: ui_script: WMBridge is disabled — not loaded`` (x4; stderr.log:432 through stderr.log:3209)
- ``benilla_app::ui_script::addons: diag[ADDON] !!!garbageprotector is DISABLED in AddOn enable configuration`` (x4; stderr.log:469 through stderr.log:3246)
- ``benilla_app::ui_script::addons: ui_script: !!!garbageprotector is disabled — not loaded`` (x4; stderr.log:468 through stderr.log:3245)
- ``benilla_app::ui_script::addons: ui_script(FrameXML/Interface\FrameXML\GameTime.xml): GameTimeFrame: OnLoad: runtime error: Interface\FrameXML\GameTime.lua:141: attempt to index a nil value`` (x4; stderr.log:279 through stderr.log:3058)
- ``benilla_app::ui_script::addons: ui_script(GlueXML/Interface\GlueXML\CharacterCreate.xml): Texture CharacterCreateAlternateFormTopHighlight: Unable to load texture file Interface\Glues\CharacterCreate\AlternateForm`` (x4; stderr.log:112 through stderr.log:2919)
- ``benilla_app::ui_script::addons: ui_script(GlueXML/Interface\GlueXML\CharacterCreate.xml): Texture CharacterCreateAlternateFormTopTop: Unable to load texture file Interface\Glues\CharacterCreate\AlternateForm`` (x4; stderr.log:111 through stderr.log:2918)
- ``benilla_app::ui_script::addons: ui_script(GlueXML/Interface\GlueXML\CharacterCreate.xml): Texture Bottom: Unable to load texture file Interface\Glues\CharacterCreate\AlternateForm`` (x4; stderr.log:114 through stderr.log:2921)
- ``benilla_app::ui_script::addons: ui_script(GlueXML/Interface\GlueXML\CharacterCreate.xml): Texture CharacterCreateAlternateFormTopPushedTexture: Unable to load texture file Interface\Glues\CharacterCreate\AlternateForm`` (x4; stderr.log:113 through stderr.log:2920)
- ``benilla_app::ui_script::addons: ui_script(GlueXML/Interface\GlueXML\CharacterCreate.xml): CharCreateRaceFrame: text="CUSTOMIZE_OPTIONS" is shaped like a GlobalStrings key but no such string global exists — showing the key`` (x4; stderr.log:97 through stderr.log:2904)
- ``benilla_app::ui_script::addons: ui_script(GlueXML/Interface\GlueXML\CharacterCreate.xml): CharacterCreateTooltip2: Unknown frame strata: TOOLTIP2`` (x4; stderr.log:96 through stderr.log:2903)
- ``benilla_app::ui_script::addons: ui_script(GlueXML/Interface\GlueXML\CharacterCreate.xml): Couldn't find relative frame: CharCreateClassButton6`` (x4; stderr.log:109 through stderr.log:2916)
- ``benilla_app::ui_script::addons: ui_script(GlueXML/Interface\GlueXML\CharacterCreate.xml): SETALLPOINTS set to true in frame with anchors (ignored)`` (x4; stderr.log:98 through stderr.log:2905)
- ``benilla_app::ui_script::addons: ui_script(FrameXML/Interface\FrameXML\VideoOptionsFrame.xml): VideoOptionsFrame: text="OPTIONS" is shaped like a GlobalStrings key but no such string global exists — showing the key`` (x4; stderr.log:259 through stderr.log:3038)
- ``benilla_app::ui_script::addons: ui_script(GlueXML/Interface\GlueXML\CharacterCreate.xml): CharacterTemplateConfirmDialogContents: text="TEMPLATE_CHARACTER_CHOOSE_FACTION" is shaped like a GlobalStrings key but no such string global exists — showing the key`` (x4; stderr.log:119 through stderr.log:2926)
- ``benilla_app::ui_script::addons: ui_script(FrameXML/Interface\FrameXML\BattlefieldFrame.xml): BattlefieldFrameInfoScrollFrameChildFrame: text="BATTLEGROUND_LORETEXT_1" is shaped like a GlobalStrings key but no such string global exists — showing the key`` (x4; stderr.log:273 through stderr.log:3052)
- ``benilla_app::ui_script::addons: ui_script(FrameXML/Interface\FrameXML\AudioOptionsFrame.xml): AudioOptionsFrame: text="OPTIONS" is shaped like a GlobalStrings key but no such string global exists — showing the key`` (x4; stderr.log:262 through stderr.log:3041)
- ``benilla_app::ui_script::addons: ui_script(GlueXML/Interface\GlueXML\CharacterCreate.xml): Texture CharacterCreateAlternateFormBottomPushedTexture: Unable to load texture file Interface\Glues\CharacterCreate\AlternateForm`` (x4; stderr.log:116 through stderr.log:2923)
- ``benilla_app::ui_script::addons: ui_script(GlueXML/Interface\GlueXML\CharacterCreate.xml): Texture CharacterCreateAlternateFormBottomHighlight: Unable to load texture file Interface\Glues\CharacterCreate\AlternateForm`` (x4; stderr.log:115 through stderr.log:2922)
- ``benilla_app::ui_script::addons: ui_script(GlueXML/Interface\GlueXML\CharacterCreate.xml): Texture <unnamed>: Unable to load texture file Interface\Glues\CharacterCreate\CharacterCreate-Arrow`` (x4; stderr.log:118 through stderr.log:2925)
- ``benilla_app::ui_script::addons: ui_script(GlueXML/Interface\GlueXML\CharacterCreate.xml): Texture CharCreatePreviewFrameLeftEdge: Unable to load texture file Interface\Glues\CharacterCreate\Frame-Sides`` (x4; stderr.log:117 through stderr.log:2924)
- ``benilla_app::ui_script::addons: diag[ADDON] Details_TimeLine is DISABLED in AddOn enable configuration`` (x4; stderr.log:544 through stderr.log:3282)
- ``benilla_app::ui_script::addons: ui_script: gearscorelite is disabled — not loaded`` (x4; stderr.log:558 through stderr.log:3296)
- ``benilla_app::ui_script::addons: ui_script: Details_TinyThreat is disabled — not loaded`` (x4; stderr.log:545 through stderr.log:3283)
- ``benilla_app::ui_script::addons: diag[ADDON] GatherMate2 is DISABLED in AddOn enable configuration`` (x4; stderr.log:556 through stderr.log:3294)
- ``benilla_app::ui_script::addons: ui_script: Details_SunderCount is disabled — not loaded`` (x4; stderr.log:541 through stderr.log:3279)
- ``benilla_app::ui_script::addons: diag[ADDON] Details_EncounterDetails is DISABLED in AddOn enable configuration`` (x4; stderr.log:540 through stderr.log:3278)
- ``benilla_app::ui_script::addons: ui_script: Details_TimeLine is disabled — not loaded`` (x4; stderr.log:543 through stderr.log:3281)
- ``benilla_app::ui_script::addons: diag[ADDON] Details_SunderCount is DISABLED in AddOn enable configuration`` (x4; stderr.log:542 through stderr.log:3280)
- ``benilla_app::ui_script::addons: diag[ADDON] Details_TinyThreat is DISABLED in AddOn enable configuration`` (x4; stderr.log:546 through stderr.log:3284)
- ``benilla_app::ui_script::addons: diag[ADDON] GatherMate is DISABLED in AddOn enable configuration`` (x4; stderr.log:554 through stderr.log:3292)
- ``benilla_app::ui_script::addons: ui_script: GatherHud is disabled — not loaded`` (x4; stderr.log:551 through stderr.log:3289)
- ``benilla_app::ui_script::addons: diag[ADDON] GatherHud is DISABLED in AddOn enable configuration`` (x4; stderr.log:552 through stderr.log:3290)
- ``benilla_app::ui_script::addons: ui_script: GatherMate is disabled — not loaded`` (x4; stderr.log:553 through stderr.log:3291)
- ``benilla_app::ui_script::addons: diag[ADDON] errorfilter is DISABLED in AddOn enable configuration`` (x4; stderr.log:548 through stderr.log:3286)
- ``benilla_app::ui_script::addons: ui_script: errorfilter is disabled — not loaded`` (x4; stderr.log:547 through stderr.log:3285)
- ``benilla_app::ui_script::addons: diag[ADDON] fizzle is DISABLED in AddOn enable configuration`` (x4; stderr.log:550 through stderr.log:3288)
- ``benilla_app::ui_script::addons: ui_script: fizzle is disabled — not loaded`` (x4; stderr.log:549 through stderr.log:3287)
- ``benilla_app::ui_script::addons: diag[ADDON] combatlogfix is DISABLED in AddOn enable configuration`` (x4; stderr.log:528 through stderr.log:3266)
- ``benilla_app::ui_script::addons: ui_script: combatlogfix is disabled — not loaded`` (x4; stderr.log:527 through stderr.log:3265)
- ``benilla_app::ui_script::addons: diag[ADDON] cromulent is DISABLED in AddOn enable configuration`` (x4; stderr.log:530 through stderr.log:3268)
- ``benilla_app::ui_script::addons: ui_script: cromulent is disabled — not loaded`` (x4; stderr.log:529 through stderr.log:3267)
- ``benilla_app::ui_script::addons: diag[ADDON] cerror is DISABLED in AddOn enable configuration`` (x4; stderr.log:526 through stderr.log:3264)
- ``benilla_app::ui_script::addons: ui_script: blizzmove is disabled — not loaded`` (x4; stderr.log:523 through stderr.log:3261)
- ``benilla_app::ui_script::addons: ui_script: GatherMate2 is disabled — not loaded`` (x4; stderr.log:555 through stderr.log:3293)
- ``benilla_app::ui_script::addons: ui_script: cerror is disabled — not loaded`` (x4; stderr.log:525 through stderr.log:3263)
- ``benilla_app::ui_script::addons: diag[ADDON] blizzmove is DISABLED in AddOn enable configuration`` (x4; stderr.log:524 through stderr.log:3262)
- ``benilla_app::ui_script::addons: diag[ADDON] Details_DeathGraphs is DISABLED in AddOn enable configuration`` (x4; stderr.log:538 through stderr.log:3276)
- ``benilla_app::ui_script::addons: diag[ADDON] Details_ChartViewer is DISABLED in AddOn enable configuration`` (x4; stderr.log:535 through stderr.log:3273)
- ``benilla_app::ui_script::addons: ui_script: Details_DeathGraphs is disabled — not loaded`` (x4; stderr.log:537 through stderr.log:3275)
- ``benilla_app::ui_script::addons: ui_script: Details_ChartViewer is disabled — not loaded`` (x4; stderr.log:534 through stderr.log:3272)
- ``benilla_app::ui_script::addons: diag[ADDON] Details is DISABLED in AddOn enable configuration`` (x4; stderr.log:532 through stderr.log:3270)
- ``benilla_app::ui_script::addons: ui_script: Details_EncounterDetails is disabled — not loaded`` (x4; stderr.log:539 through stderr.log:3277)
- ``benilla_app::ui_script::addons: ui_script: Details is disabled — not loaded`` (x4; stderr.log:531 through stderr.log:3269)
- ``benilla_app::ui_script::addons: diag[ADDON] Bagnon_Tooltips is DISABLED in AddOn enable configuration`` (x3; stderr.log:1869 through stderr.log:3258)
- ``benilla_app::ui_script::addons: ui_script: bartender4 is disabled — not loaded`` (x3; stderr.log:1870 through stderr.log:3259)
- ``benilla_app::ui_script::addons: diag[ADDON] bartender4 is DISABLED in AddOn enable configuration`` (x3; stderr.log:1871 through stderr.log:3260)
- ``benilla_app::ui_script::addons: ui_script: Bagnon_Forever is disabled — not loaded`` (x3; stderr.log:1865 through stderr.log:3254)
- ``benilla_app::ui_script::input: ui_script(input): runtime error: Interface\FrameXML\ReputationFrame.lua:167: attempt to call global `GetText' (a nil value)`` (x3; stderr.log:2805 through stderr.log:2824)
- ``benilla_app::ui_script::addons: diag[ADDON] Bagnon is DISABLED in AddOn enable configuration`` (x3; stderr.log:1863 through stderr.log:3252)
- ``benilla_app::ui_script::addons: ui_script: Bagnon_Tooltips is disabled — not loaded`` (x3; stderr.log:1868 through stderr.log:3257)
- ``benilla_app::ui_script::addons: diag[ADDON] Bagnon_Forever is DISABLED in AddOn enable configuration`` (x3; stderr.log:1866 through stderr.log:3255)
- ``benilla_app::ui_script::addons: ui_script: Bagnon is disabled — not loaded`` (x3; stderr.log:1862 through stderr.log:3251)
- ``benilla_app::ui_script::input: ui_script(input): runtime error: Interface\FrameXML\PaperDollFrame.lua:280: attempt to call global `GetUnitMaxHealthModifier' (a nil value)`` (x3; stderr.log:1450 through stderr.log:2825)
- ``benilla_app::ui_script::extract: ui_script: CreateFrame("Button", "TargetFrameBuff2", inherits="TargetBuffFrameTemplate"): 'TargetBuffFrameTemplate' is a <Frame>, but a Button was created; the frame keeps the kind CreateFrame was given, so the <Frame>-only parts of the template do not apply`` (x3; stderr.log:1471 through stderr.log:2846)
- ``benilla_app::ui_script::extract: ui_script: CreateFrame("Button", "TargetFrameBuff1", inherits="TargetBuffFrameTemplate"): 'TargetBuffFrameTemplate' is a <Frame>, but a Button was created; the frame keeps the kind CreateFrame was given, so the <Frame>-only parts of the template do not apply`` (x3; stderr.log:1470 through stderr.log:2845)
- ``benilla_app::ui_script::extract: ui_script: runtime error: Interface\FrameXML\LFRFrame.lua:45: attempt to call method `HasFocus' (a nil value)`` (x2; stderr.log:2582 through stderr.log:2867)
- ``benilla_app::ui_script::extract: ui_script: runtime error: Interface\FrameXML\TargetFrame.lua:201: attempt to call global `ClearFocus' (a nil value)`` (x2; stderr.log:2580 through stderr.log:2865)
- ``benilla_app::ui_script::extract: ui_script: runtime error: Interface\FrameXML\ArenaFrame.lua:11: attempt to call global `IsBattlefieldArena' (a nil value)`` (x2; stderr.log:2586 through stderr.log:2871)
- ``benilla_app::ui_script::input: ui_script(input): runtime error: [string "LFDQueueFrameRandomScrollFrameChildFrameIte..."]:3: attempt to call method `SetLFGDungeonReward' (a nil value)`` (x2; stderr.log:2792 through stderr.log:2796)
- ``benilla_app::ui_script::input: ui_script(input): runtime error: Interface\FrameXML\MainMenuBar.lua:400: attempt to perform arithmetic on local `exhaustionStateMultiplier' (a nil value)`` (x2; stderr.log:2787 through stderr.log:2822)
- ``benilla_app::bindings: bindings(NAMEPLATES): runtime error: [string "NAMEPLATES"]:7: Couldn't find CVar named 'nameplateShowEnemies'`` (x2; stderr.log:2785 through stderr.log:2786)
- ``benilla_app::ui_script::input: ui_script(input): runtime error: Interface\FrameXML\PaperDollFrame.lua:886: attempt to call global `GetCombatRatingBonus' (a nil value)`` (x2; stderr.log:1454 through stderr.log:1461)
- ``benilla_app::ui_script::input: ui_script(input): runtime error: Interface\FrameXML\PaperDollFrame.lua:919: attempt to call global `GetSpellBonusDamage' (a nil value)`` (x2; stderr.log:1463 through stderr.log:3409)
- ``benilla_app::ui_script::extract: ui_script: CreateFrame("Button", "TargetFrameBuff3", inherits="TargetBuffFrameTemplate"): 'TargetBuffFrameTemplate' is a <Frame>, but a Button was created; the frame keeps the kind CreateFrame was given, so the <Frame>-only parts of the template do not apply`` (x2; stderr.log:1472 through stderr.log:2037)
- ``benilla_app::ui_script::extract: ui_script: runtime error: Interface\AddOns\bartender4\ActionButton.lua:295: attempt to index field `normalTexture' (a nil value)`` (x2; stderr.log:641 through stderr.log:653)
- ``benilla_app::bindings: bindings(FRIENDNAMEPLATES): runtime error: [string "FRIENDNAMEPLATES"]:7: Couldn't find CVar named 'nameplateShowFriends'`` (x2; stderr.log:3419 through stderr.log:3420)
- ``benilla_app::ui_script::extract: ui_script: runtime error: ...rface\AddOns\Blizzard_TalentUI\Blizzard_TalentUI.lua:303: Couldn't find CVar named 'talentFrameShown'`` (x2; stderr.log:2849 through stderr.log:2852)
- ``benilla_app::ui_script::extract: ui_script: CreateFrame("Button", "TargetFrameBuff7", inherits="TargetBuffFrameTemplate"): 'TargetBuffFrameTemplate' is a <Frame>, but a Button was created; the frame keeps the kind CreateFrame was given, so the <Frame>-only parts of the template do not apply`` (x2; stderr.log:1476 through stderr.log:2041)
- ``benilla_app::ui_script::extract: ui_script: CreateFrame("Button", "TargetFrameBuff6", inherits="TargetBuffFrameTemplate"): 'TargetBuffFrameTemplate' is a <Frame>, but a Button was created; the frame keeps the kind CreateFrame was given, so the <Frame>-only parts of the template do not apply`` (x2; stderr.log:1475 through stderr.log:2040)
- ``benilla_app::ui_script::extract: ui_script: CreateFrame("Button", "TargetFrameBuff9", inherits="TargetBuffFrameTemplate"): 'TargetBuffFrameTemplate' is a <Frame>, but a Button was created; the frame keeps the kind CreateFrame was given, so the <Frame>-only parts of the template do not apply`` (x2; stderr.log:1478 through stderr.log:2043)
- ``benilla_app::ui_script::extract: ui_script: CreateFrame("Button", "TargetFrameBuff8", inherits="TargetBuffFrameTemplate"): 'TargetBuffFrameTemplate' is a <Frame>, but a Button was created; the frame keeps the kind CreateFrame was given, so the <Frame>-only parts of the template do not apply`` (x2; stderr.log:1477 through stderr.log:2042)
- ``benilla_app::ui_script::extract: ui_script: CreateFrame("Button", "TargetFrameBuff5", inherits="TargetBuffFrameTemplate"): 'TargetBuffFrameTemplate' is a <Frame>, but a Button was created; the frame keeps the kind CreateFrame was given, so the <Frame>-only parts of the template do not apply`` (x2; stderr.log:1474 through stderr.log:2039)
- ``benilla_app::ui_script::extract: ui_script: CreateFrame("Button", "TargetFrameBuff4", inherits="TargetBuffFrameTemplate"): 'TargetBuffFrameTemplate' is a <Frame>, but a Button was created; the frame keeps the kind CreateFrame was given, so the <Frame>-only parts of the template do not apply`` (x2; stderr.log:1473 through stderr.log:2038)
- ``benilla_app::cvars: config: unknown cvar 'threatShowNumeric' — preserved, not applied`` (x1; stderr.log:16 through stderr.log:16)
- ``benilla_app::ui_script::extract: ui_script: runtime error: ...terface\AddOns\Blizzard_TokenUI\Blizzard_TokenUI.lua:180: attempt to call global `GetBackpackCurrencyInfo' (a nil value)`` (x1; stderr.log:3443 through stderr.log:3443)
- ``benilla_app::ui_script::extract: ui_script: runtime error: Interface\FrameXML\FriendsFrame.lua:757: attempt to call global `BNGetNumFriendInvites' (a nil value)`` (x1; stderr.log:2834 through stderr.log:2834)
- ``benilla_app::cvars: config: unknown cvar 'targetStatusText' — preserved, not applied`` (x1; stderr.log:15 through stderr.log:15)
- ``benilla_app::ui_script::input: ui_script(input): runtime error: ...terface\AddOns\Blizzard_TokenUI\Blizzard_TokenUI.lua:180: attempt to call global `GetBackpackCurrencyInfo' (a nil value)`` (x1; stderr.log:2858 through stderr.log:2858)
- ``benilla_app::ui_script::input: ui_script(input): runtime error: Interface\FrameXML\FriendsFrame.lua:757: attempt to call global `BNGetNumFriendInvites' (a nil value)`` (x1; stderr.log:2815 through stderr.log:2815)
- ``benilla_app::ui_script::input: ui_script(input): runtime error: Interface\FrameXML\PVPFrame.lua:88: attempt to call global `GetCurrentArenaSeason' (a nil value)`` (x1; stderr.log:2813 through stderr.log:2813)
- ``benilla_app::ui_script::lifecycle: ui_script: stock glue up in 291 ms on "charselect" (0 load failure(s))`` (x1; stderr.log:2943 through stderr.log:2943)
- ``benilla_app::ui_script::extract: ui_script: CreateFrame("CheckButton", "Bartender4DialogLock", inherits="OptionsButtonTemplate"): 'OptionsButtonTemplate' is a <Button>, but a CheckButton was created; the frame keeps the kind CreateFrame was given, so the <Button>-only parts of the template do not apply`` (x1; stderr.log:703 through stderr.log:703)
- ``benilla_app::ui_script::extract: ui_script: CreateFrame("Button", "TargetFrameDebuff1", inherits="TargetDebuffFrameTemplate"): 'TargetDebuffFrameTemplate' is a <Frame>, but a Button was created; the frame keeps the kind CreateFrame was given, so the <Frame>-only parts of the template do not apply`` (x1; stderr.log:1442 through stderr.log:1442)
- ``benilla_app::ui_script::extract: ui_script: runtime error: Interface\AddOns\Bagnon\components\item.lua:27: attempt to call method `GetDummyBag' (a nil value)`` (x1; stderr.log:1448 through stderr.log:1448)
- ``benilla_app::ui_script::lifecycle: ui_script: stock glue up in 3399 ms on "login" (0 load failure(s))`` (x1; stderr.log:136 through stderr.log:136)
- ``benilla_app::ui_script::extract: ui_script: CreateFrame("CheckButton", "KeyboundDialogOkay", inherits="OptionsButtonTemplate"): 'OptionsButtonTemplate' is a <Button>, but a CheckButton was created; the frame keeps the kind CreateFrame was given, so the <Button>-only parts of the template do not apply`` (x1; stderr.log:659 through stderr.log:659)
- ``benilla_app::ui_script::extract: ui_script: CreateFrame("CheckButton", "KeyboundDialogCancel", inherits="OptionsButtonTemplate"): 'OptionsButtonTemplate' is a <Button>, but a CheckButton was created; the frame keeps the kind CreateFrame was given, so the <Button>-only parts of the template do not apply`` (x1; stderr.log:660 through stderr.log:660)
- ``benilla_app::ui_script::extract: ui_script: Post-hook: Interface\AddOns\bartender4\ActionButton.lua:377: attempt to compare nil with number`` (x1; stderr.log:668 through stderr.log:668)
- ``benilla_app::ui_script::input: ui_script(input): runtime error: Interface\FrameXML\PaperDollFrame.lua:646: attempt to call global `GetCombatRatingBonus' (a nil value)`` (x1; stderr.log:1459 through stderr.log:1459)
- ``benilla_app::ui_script::input: ui_script(input): runtime error: Interface\FrameXML\PaperDollFrame.lua:1793: attempt to call global `GetInventoryItemsForSlot' (a nil value)`` (x1; stderr.log:2073 through stderr.log:2073)
- ``benilla_app::ui_script::lifecycle: ui_script: stock glue up in 298 ms on "charselect" (0 load failure(s))`` (x1; stderr.log:2150 through stderr.log:2150)
- ``benilla_app::ui_script::extract: ui_script: runtime error: Interface\FrameXML\MainMenuBar.lua:317: attempt to compare number with nil`` (x1; stderr.log:2571 through stderr.log:2571)
- ``benilla_app::ui_script::extract: ui_script: CreateFrame("Button", "TargetFrameBuff10", inherits="TargetBuffFrameTemplate"): 'TargetBuffFrameTemplate' is a <Frame>, but a Button was created; the frame keeps the kind CreateFrame was given, so the <Frame>-only parts of the template do not apply`` (x1; stderr.log:2044 through stderr.log:2044)
- ``benilla_app::ui_script::input: ui_script(input): runtime error: Interface\FrameXML\PaperDollFrame.lua:465: attempt to call global `GetCombatRatingBonus' (a nil value)`` (x1; stderr.log:1465 through stderr.log:1465)
- ``benilla_app::ui_script::addons: ui_script(Bagnon/components.xml): <Script file="components\item.lua">: runtime error: Interface\AddOns\Bagnon\components\item.lua:562: bad argument #2 to `format' (string expected, got nil)`` (x1; stderr.log:480 through stderr.log:480)
- ``benilla_app::ui_script::lifecycle: ui_script: stock glue up in 281 ms on "charselect" (0 load failure(s))`` (x1; stderr.log:1569 through stderr.log:1569)

### Movement
- ``benilla_app::ui_script::addons: ui_script(FrameXML/Interface\FrameXML\BattlefieldFrame.xml): BattlefieldFrameInfoScrollFrameChildFrame: text="BATTLEGROUND_LORETEXT_1" is shaped like a GlobalStrings key but no such string global exists — showing the key`` (x4; stderr.log:273 through stderr.log:3052)
- ``benilla_app::ui_script::addons: ui_script(FrameXML/Interface\FrameXML\PVPBattlegroundFrame.xml): PVPBattlegroundFrameInfoScrollFrameChildFrame: text="BATTLEGROUND_LORETEXT_1" is shaped like a GlobalStrings key but no such string global exists — showing the key`` (x4; stderr.log:407 through stderr.log:3184)
- ``benilla_app::pipe_warm: pipeline compiled LIVE [152] label=pbr_alpha_blend_mesh_pipeline vs=embedded://benilla_assets/shaders/wow_model.wgsl vs_defs=[IRRADIANCE_VOLUMES_ARE_USABLE+MATERIAL_BIND_GROUP=3+MESH_PIPELINE+MULTIPLE_LIGHTMAPS_IN_ARRAY+MULTIPLE_LIGHT_PROBES_IN_ARRAY+SCREEN_SPACE_SPECULAR_TRANSMISSION_BLUR_TAPS=8+SHADOW_FILTER_METHOD_GAUSSIAN+VERTEX_NORMALS+VERTEX_OUTPUT_INSTANCE_INDEX+VERTEX_POSITIONS+VERTEX_UVS+VERTEX_UVS_A+VIEW_PROJECTION_NONSTANDARD+WOW_RIG_SKIN] bufs=[stride56@[0,1,2,10,11]] cull=Some(Back) bias=0 depth_write=false cmp=GreaterEqual fs=embedded://benilla_assets/shaders/wow_model.wgsl fs_defs=[IRRADIANCE_VOLUMES_ARE_USABLE+MATERIAL_BIND_GROUP=3+MESH_PIPELINE+MULTIPLE_LIGHTMAPS_IN_ARRAY+MULTIPLE_LIGHT_PROBES_IN_ARRAY+SCREEN_SPACE_SPECULAR_TRANSMISSION_BLUR_TAPS=8+SHADOW_FILTER_METHOD_GAUSSIAN+VERTEX_NORMALS+VERTEX_OUTPUT_INSTANCE_INDEX+VERTEX_POSITIONS+VERTEX_UVS+VERTEX_UVS_A+VIEW_PROJECTION_NONSTANDARD+WOW_WATER_CLIP] blend=Some(BlendState { color: BlendComponent { src_factor: SrcAlpha, dst_factor: OneMinusSrcAlpha, operation: Add }, alpha: BlendComponent { src_factor: One, dst_factor: OneMinusSrcAlpha, operation: Add } }) mask=ColorWrites(15) samples=1`` (x1; stderr.log:1468 through stderr.log:1468)
- ``benilla_app::pipe_warm: pipeline compiled LIVE [151] label=pbr_alpha_blend_mesh_pipeline vs=embedded://benilla_assets/shaders/wow_model.wgsl vs_defs=[IRRADIANCE_VOLUMES_ARE_USABLE+MATERIAL_BIND_GROUP=3+MESH_PIPELINE+MULTIPLE_LIGHTMAPS_IN_ARRAY+MULTIPLE_LIGHT_PROBES_IN_ARRAY+SCREEN_SPACE_SPECULAR_TRANSMISSION_BLUR_TAPS=8+SHADOW_FILTER_METHOD_GAUSSIAN+VERTEX_NORMALS+VERTEX_OUTPUT_INSTANCE_INDEX+VERTEX_POSITIONS+VERTEX_UVS+VERTEX_UVS_A+VIEW_PROJECTION_NONSTANDARD] bufs=[stride32@[0,1,2]] cull=Some(Back) bias=0 depth_write=false cmp=GreaterEqual fs=embedded://benilla_assets/shaders/wow_model.wgsl fs_defs=[IRRADIANCE_VOLUMES_ARE_USABLE+MATERIAL_BIND_GROUP=3+MESH_PIPELINE+MULTIPLE_LIGHTMAPS_IN_ARRAY+MULTIPLE_LIGHT_PROBES_IN_ARRAY+SCREEN_SPACE_SPECULAR_TRANSMISSION_BLUR_TAPS=8+SHADOW_FILTER_METHOD_GAUSSIAN+VERTEX_NORMALS+VERTEX_OUTPUT_INSTANCE_INDEX+VERTEX_POSITIONS+VERTEX_UVS+VERTEX_UVS_A+VIEW_PROJECTION_NONSTANDARD+WOW_WATER_CLIP] blend=Some(BlendState { color: BlendComponent { src_factor: One, dst_factor: One, operation: Add }, alpha: BlendComponent { src_factor: Zero, dst_factor: One, operation: Add } }) mask=ColorWrites(15) samples=1`` (x1; stderr.log:1451 through stderr.log:1451)
- ``benilla_app::pipe_warm: pipeline compiled LIVE [150] label=pbr_alpha_blend_mesh_pipeline vs=embedded://benilla_assets/shaders/wow_model.wgsl vs_defs=[IRRADIANCE_VOLUMES_ARE_USABLE+MATERIAL_BIND_GROUP=3+MESH_PIPELINE+MULTIPLE_LIGHTMAPS_IN_ARRAY+MULTIPLE_LIGHT_PROBES_IN_ARRAY+SCREEN_SPACE_SPECULAR_TRANSMISSION_BLUR_TAPS=8+SHADOW_FILTER_METHOD_GAUSSIAN+VERTEX_COLORS+VERTEX_NORMALS+VERTEX_OUTPUT_INSTANCE_INDEX+VERTEX_POSITIONS+VERTEX_UVS+VERTEX_UVS_A+VIEW_PROJECTION_PERSPECTIVE] bufs=[stride48@[0,1,2,5]] cull=None bias=0 depth_write=false cmp=GreaterEqual fs=embedded://benilla_assets/shaders/wow_model.wgsl fs_defs=[IRRADIANCE_VOLUMES_ARE_USABLE+MATERIAL_BIND_GROUP=3+MESH_PIPELINE+MULTIPLE_LIGHTMAPS_IN_ARRAY+MULTIPLE_LIGHT_PROBES_IN_ARRAY+SCREEN_SPACE_SPECULAR_TRANSMISSION_BLUR_TAPS=8+SHADOW_FILTER_METHOD_GAUSSIAN+VERTEX_COLORS+VERTEX_NORMALS+VERTEX_OUTPUT_INSTANCE_INDEX+VERTEX_POSITIONS+VERTEX_UVS+VERTEX_UVS_A+VIEW_PROJECTION_PERSPECTIVE+WOW_WATER_CLIP] blend=Some(BlendState { color: BlendComponent { src_factor: One, dst_factor: One, operation: Add }, alpha: BlendComponent { src_factor: Zero, dst_factor: One, operation: Add } }) mask=ColorWrites(15) samples=1`` (x1; stderr.log:1444 through stderr.log:1444)
- ``benilla_app::pipe_warm: pipeline compiled LIVE [154] label=pbr_alpha_blend_mesh_pipeline vs=embedded://benilla_assets/shaders/wow_model.wgsl vs_defs=[IRRADIANCE_VOLUMES_ARE_USABLE+MATERIAL_BIND_GROUP=3+MESH_PIPELINE+MULTIPLE_LIGHTMAPS_IN_ARRAY+MULTIPLE_LIGHT_PROBES_IN_ARRAY+SCREEN_SPACE_SPECULAR_TRANSMISSION_BLUR_TAPS=8+SHADOW_FILTER_METHOD_GAUSSIAN+VERTEX_NORMALS+VERTEX_OUTPUT_INSTANCE_INDEX+VERTEX_POSITIONS+VERTEX_UVS+VERTEX_UVS_A+VIEW_PROJECTION_PERSPECTIVE] bufs=[stride32@[0,1,2]] cull=None bias=0 depth_write=false cmp=GreaterEqual fs=embedded://benilla_assets/shaders/wow_model.wgsl fs_defs=[IRRADIANCE_VOLUMES_ARE_USABLE+MATERIAL_BIND_GROUP=3+MESH_PIPELINE+MULTIPLE_LIGHTMAPS_IN_ARRAY+MULTIPLE_LIGHT_PROBES_IN_ARRAY+SCREEN_SPACE_SPECULAR_TRANSMISSION_BLUR_TAPS=8+SHADOW_FILTER_METHOD_GAUSSIAN+VERTEX_NORMALS+VERTEX_OUTPUT_INSTANCE_INDEX+VERTEX_POSITIONS+VERTEX_UVS+VERTEX_UVS_A+VIEW_PROJECTION_PERSPECTIVE+WOW_WATER_CLIP] blend=Some(BlendState { color: BlendComponent { src_factor: SrcAlpha, dst_factor: OneMinusSrcAlpha, operation: Add }, alpha: BlendComponent { src_factor: One, dst_factor: OneMinusSrcAlpha, operation: Add } }) mask=ColorWrites(15) samples=1`` (x1; stderr.log:1480 through stderr.log:1480)
- ``benilla_app::sound::mixer: audio: ambience stream starved — ~159 ms of injected silence over a 0.85 s window (counted 1.02 s, position advanced 0.86 s) — this is what a crackle sounds like`` (x1; stderr.log:3401 through stderr.log:3401)
- ``benilla_app::pipe_warm: pipeline compiled LIVE [153] label=pbr_opaque_mesh_pipeline vs=embedded://benilla_assets/shaders/wow_model.wgsl vs_defs=[IRRADIANCE_VOLUMES_ARE_USABLE+MATERIAL_BIND_GROUP=3+MAY_DISCARD+MESH_PIPELINE+MULTIPLE_LIGHTMAPS_IN_ARRAY+MULTIPLE_LIGHT_PROBES_IN_ARRAY+SCREEN_SPACE_SPECULAR_TRANSMISSION_BLUR_TAPS=8+SHADOW_FILTER_METHOD_GAUSSIAN+VERTEX_NORMALS+VERTEX_OUTPUT_INSTANCE_INDEX+VERTEX_POSITIONS+VERTEX_UVS+VERTEX_UVS_A+VIEW_PROJECTION_NONSTANDARD+WOW_RIG_SKIN] bufs=[stride56@[0,1,2,10,11]] cull=Some(Back) bias=0 depth_write=true cmp=GreaterEqual fs=embedded://benilla_assets/shaders/wow_model.wgsl fs_defs=[IRRADIANCE_VOLUMES_ARE_USABLE+MATERIAL_BIND_GROUP=3+MAY_DISCARD+MESH_PIPELINE+MULTIPLE_LIGHTMAPS_IN_ARRAY+MULTIPLE_LIGHT_PROBES_IN_ARRAY+SCREEN_SPACE_SPECULAR_TRANSMISSION_BLUR_TAPS=8+SHADOW_FILTER_METHOD_GAUSSIAN+VERTEX_NORMALS+VERTEX_OUTPUT_INSTANCE_INDEX+VERTEX_POSITIONS+VERTEX_UVS+VERTEX_UVS_A+VIEW_PROJECTION_NONSTANDARD] blend=None mask=ColorWrites(15) samples=1`` (x1; stderr.log:1469 through stderr.log:1469)
- ``benilla_app::sound::mixer: audio: Lua music stream starved — ~133 ms of injected silence over a 1.00 s window (counted 1.13 s, position advanced 1.00 s) — this is what a crackle sounds like`` (x1; stderr.log:679 through stderr.log:679)
- ``benilla_assets::m2: M2 particle data unavailable for mpq-loose://interface/buttons/ui-autocastbutton.m2: Wrath spline particles remain unsupported`` (x1; stderr.log:167 through stderr.log:167)
- ``benilla_app::preflight: preflight: NET OFF — no IO thread this run, so the drain gets no packets and NOTHING on the wire path executes (net::apply, the movement stream, every MSG_MOVE_*). Fine for a visual capture; NOT evidence for a change to any of it. docs/METHOD.md's gate is a clean run of the AFFECTED path — for wire work that means a live server run.`` (x1; stderr.log:12 through stderr.log:12)
- ``benilla_app::pipe_warm: pipeline compiled LIVE [149] label=pbr_alpha_blend_mesh_pipeline vs=embedded://benilla_assets/shaders/wow_model.wgsl vs_defs=[IRRADIANCE_VOLUMES_ARE_USABLE+MATERIAL_BIND_GROUP=3+MESH_PIPELINE+MULTIPLE_LIGHTMAPS_IN_ARRAY+MULTIPLE_LIGHT_PROBES_IN_ARRAY+SCREEN_SPACE_SPECULAR_TRANSMISSION_BLUR_TAPS=8+SHADOW_FILTER_METHOD_GAUSSIAN+VERTEX_COLORS+VERTEX_NORMALS+VERTEX_OUTPUT_INSTANCE_INDEX+VERTEX_POSITIONS+VERTEX_UVS+VERTEX_UVS_A+VIEW_PROJECTION_PERSPECTIVE] bufs=[stride48@[0,1,2,5]] cull=Some(Back) bias=0 depth_write=false cmp=GreaterEqual fs=embedded://benilla_assets/shaders/wow_model.wgsl fs_defs=[IRRADIANCE_VOLUMES_ARE_USABLE+MATERIAL_BIND_GROUP=3+MESH_PIPELINE+MULTIPLE_LIGHTMAPS_IN_ARRAY+MULTIPLE_LIGHT_PROBES_IN_ARRAY+SCREEN_SPACE_SPECULAR_TRANSMISSION_BLUR_TAPS=8+SHADOW_FILTER_METHOD_GAUSSIAN+VERTEX_COLORS+VERTEX_NORMALS+VERTEX_OUTPUT_INSTANCE_INDEX+VERTEX_POSITIONS+VERTEX_UVS+VERTEX_UVS_A+VIEW_PROJECTION_PERSPECTIVE+WOW_WATER_CLIP] blend=Some(BlendState { color: BlendComponent { src_factor: One, dst_factor: One, operation: Add }, alpha: BlendComponent { src_factor: Zero, dst_factor: One, operation: Add } }) mask=ColorWrites(15) samples=1`` (x1; stderr.log:1425 through stderr.log:1425)
- ``benilla_app::pipe_warm: pipeline compiled LIVE [148] label=pbr_opaque_mesh_pipeline vs=embedded://benilla_assets/shaders/wow_model.wgsl vs_defs=[IRRADIANCE_VOLUMES_ARE_USABLE+MATERIAL_BIND_GROUP=3+MAY_DISCARD+MESH_PIPELINE+MULTIPLE_LIGHTMAPS_IN_ARRAY+MULTIPLE_LIGHT_PROBES_IN_ARRAY+SCREEN_SPACE_SPECULAR_TRANSMISSION_BLUR_TAPS=8+SHADOW_FILTER_METHOD_GAUSSIAN+VERTEX_NORMALS+VERTEX_OUTPUT_INSTANCE_INDEX+VERTEX_POSITIONS+VERTEX_UVS+VERTEX_UVS_A+VIEW_PROJECTION_PERSPECTIVE] bufs=[stride32@[0,1,2]] cull=None bias=0 depth_write=true cmp=GreaterEqual fs=embedded://benilla_assets/shaders/wow_model.wgsl fs_defs=[IRRADIANCE_VOLUMES_ARE_USABLE+MATERIAL_BIND_GROUP=3+MAY_DISCARD+MESH_PIPELINE+MULTIPLE_LIGHTMAPS_IN_ARRAY+MULTIPLE_LIGHT_PROBES_IN_ARRAY+SCREEN_SPACE_SPECULAR_TRANSMISSION_BLUR_TAPS=8+SHADOW_FILTER_METHOD_GAUSSIAN+VERTEX_NORMALS+VERTEX_OUTPUT_INSTANCE_INDEX+VERTEX_POSITIONS+VERTEX_UVS+VERTEX_UVS_A+VIEW_PROJECTION_PERSPECTIVE] blend=None mask=ColorWrites(15) samples=1`` (x1; stderr.log:1419 through stderr.log:1419)
- ``benilla_app::pipe_warm: pipeline compiled LIVE [147] label=pbr_opaque_mesh_pipeline vs=embedded://benilla_assets/shaders/wow_model.wgsl vs_defs=[IRRADIANCE_VOLUMES_ARE_USABLE+MATERIAL_BIND_GROUP=3+MESH_PIPELINE+MULTIPLE_LIGHTMAPS_IN_ARRAY+MULTIPLE_LIGHT_PROBES_IN_ARRAY+SCREEN_SPACE_SPECULAR_TRANSMISSION_BLUR_TAPS=8+SHADOW_FILTER_METHOD_GAUSSIAN+VERTEX_COLORS+VERTEX_NORMALS+VERTEX_OUTPUT_INSTANCE_INDEX+VERTEX_POSITIONS+VERTEX_UVS+VERTEX_UVS_A+VIEW_PROJECTION_PERSPECTIVE+WOW_RIG_SKIN] bufs=[stride72@[0,1,2,5,10,11]] cull=Some(Back) bias=0 depth_write=true cmp=GreaterEqual fs=embedded://benilla_assets/shaders/wow_model.wgsl fs_defs=[IRRADIANCE_VOLUMES_ARE_USABLE+MATERIAL_BIND_GROUP=3+MESH_PIPELINE+MULTIPLE_LIGHTMAPS_IN_ARRAY+MULTIPLE_LIGHT_PROBES_IN_ARRAY+SCREEN_SPACE_SPECULAR_TRANSMISSION_BLUR_TAPS=8+SHADOW_FILTER_METHOD_GAUSSIAN+VERTEX_COLORS+VERTEX_NORMALS+VERTEX_OUTPUT_INSTANCE_INDEX+VERTEX_POSITIONS+VERTEX_UVS+VERTEX_UVS_A+VIEW_PROJECTION_PERSPECTIVE] blend=None mask=ColorWrites(15) samples=1`` (x1; stderr.log:695 through stderr.log:695)

### Connection
- ``benilla_app::wrath: wrath: outbox full (256), refusing client opcode 0x56`` (x170; stderr.log:2593 through stderr.log:2762)
- ``benilla_app::entities::attach: diag[ENTITY_MODEL] Player 12735v4 BodyAtlas pending timeout (3.62s), forcing atlas build`` (x1; stderr.log:3390 through stderr.log:3390)
- ``benilla_app::entities::attach: diag[ENTITY_MODEL] Player 22063v6 BodyAtlas pending timeout (3.62s), forcing atlas build`` (x1; stderr.log:3389 through stderr.log:3389)
- ``benilla_app::entities::attach: diag[ENTITY_MODEL] Player 16945v9 BodyAtlas pending timeout (3.83s), forcing atlas build`` (x1; stderr.log:3395 through stderr.log:3395)
- ``benilla_app::entities::attach: diag[ENTITY_MODEL] Player 1053v5 BodyAtlas pending timeout (3.83s), forcing atlas build`` (x1; stderr.log:3396 through stderr.log:3396)
- ``benilla_app::entities::attach: diag[ENTITY_MODEL] Player 13571v5 BodyAtlas pending timeout (3.83s), forcing atlas build`` (x1; stderr.log:3397 through stderr.log:3397)
- ``benilla_app::ui_script::lifecycle: ui_script: stock glue up in 3399 ms on "login" (0 load failure(s))`` (x1; stderr.log:136 through stderr.log:136)
- ``benilla_app::preflight: preflight: NET OFF — no IO thread this run, so the drain gets no packets and NOTHING on the wire path executes (net::apply, the movement stream, every MSG_MOVE_*). Fine for a visual capture; NOT evidence for a change to any of it. docs/METHOD.md's gate is a clean run of the AFFECTED path — for wire work that means a live server run.`` (x1; stderr.log:12 through stderr.log:12)
- ``benilla_app::wrath: wrath: outbox full (256), refusing client opcode 0x182`` (x1; stderr.log:2763 through stderr.log:2763)
- ``benilla_app::entities::attach: diag[ENTITY_MODEL] Player 12752v4 BodyAtlas pending timeout (3.62s), forcing atlas build`` (x1; stderr.log:3388 through stderr.log:3388)
- ``benilla_app::entities::attach: diag[ENTITY_MODEL] Player 12734v4 BodyAtlas pending timeout (3.62s), forcing atlas build`` (x1; stderr.log:3387 through stderr.log:3387)

### Other
- ``benilla_app::bindings: bindings(TOGGLEBACKPACK): runtime error: ...terface\AddOns\Blizzard_TokenUI\Blizzard_TokenUI.lua:180: attempt to call global `GetBackpackCurrencyInfo' (a nil value)`` (x5; stderr.log:2024 through stderr.log:2851)
- ``benilla_app::net::session: net: wrath session ended — tearing down the streamed world`` (x4; stderr.log:1496 through stderr.log:3596)
- ``benilla_assets::m2: M2 particle data unavailable for mpq://world/generic/human/passive doodads/gryphonroost/gryphonroost01.m2: Wrath anisotropic/variable scale requires runtime support`` (x3; stderr.log:686 through stderr.log:3382)
- ``benilla_assets::m2: M2 particle data unavailable for mpq://world/generic/passivedoodads/particleemitters/blacksmith_smoke.m2: Wrath wind/tumble particles remain guarded`` (x3; stderr.log:682 through stderr.log:3383)
- ``benilla_app::pipe_warm: pipeline warm: TIMED OUT with 0 pipelines pending — cover released`` (x2; stderr.log:2769 through stderr.log:3378)
- ``symphonia_core::formats::probe: skipped 2183 bytes of junk at 0`` (x2; stderr.log:164 through stderr.log:2017)
- ``benilla_assets::m2: M2 particle data unavailable for mpq://world/generic/human/passive doodads/smokestack/smokestack.m2: Wrath wind/tumble particles remain guarded`` (x2; stderr.log:685 through stderr.log:2012)
- ``benilla_assets::m2: M2 particle data unavailable for mpq://world/azeroth/westfall/passivedoodads/lamppost/westfalllamppost02.m2: Wrath lifespan/emission variation requires runtime support`` (x1; stderr.log:3379 through stderr.log:3379)
- ``benilla_app::perf::stats: frame hitch: 11536 ms (main thread blocked this long)`` (x1; stderr.log:1492 through stderr.log:1492)
- ``benilla_app::perf::stats: frame hitch: 6853 ms (main thread blocked this long)`` (x1; stderr.log:2003 through stderr.log:2003)
- ``benilla_app::perf::stats: frame hitch: 643 ms (main thread blocked this long)`` (x1; stderr.log:1574 through stderr.log:1574)
- ``benilla_app::perf::stats: frame hitch: 12578 ms (main thread blocked this long)`` (x1; stderr.log:976 through stderr.log:976)
- ``benilla_app::perf::stats: frame hitch: 1084 ms (main thread blocked this long)`` (x1; stderr.log:2764 through stderr.log:2764)
- ``benilla_app::perf::stats: frame hitch: 11565 ms (main thread blocked this long)`` (x1; stderr.log:1489 through stderr.log:1489)
- ``symphonia_core::formats::probe: skipped 2194 bytes of junk at 0`` (x1; stderr.log:3391 through stderr.log:3391)
- ``benilla_app::pipe_warm: pipeline warm: TIMED OUT with 24 pipelines pending — cover released`` (x1; stderr.log:2011 through stderr.log:2011)
- ``benilla_app::perf::stats: frame hitch: 450 ms (main thread blocked this long)`` (x1; stderr.log:2947 through stderr.log:2947)
- ``benilla_app::perf::stats: frame hitch: 7795 ms (main thread blocked this long)`` (x1; stderr.log:2765 through stderr.log:2765)
- ``symphonia_core::formats::probe: skipped 2219 bytes of junk at 0`` (x1; stderr.log:2777 through stderr.log:2777)
- ``benilla_assets::m2: M2 particle data unavailable for mpq://creature/elementalearth2/elementalearth2.m2: Wrath lifespan/emission variation requires runtime support`` (x1; stderr.log:2766 through stderr.log:2766)
- ``benilla_app::bindings: bindings(OPENALLBAGS): runtime error: ...terface\AddOns\Blizzard_TokenUI\Blizzard_TokenUI.lua:180: attempt to call global `GetBackpackCurrencyInfo' (a nil value)`` (x1; stderr.log:2025 through stderr.log:2025)
- ``benilla_app::perf::stats: frame hitch: 9121 ms (main thread blocked this long)`` (x1; stderr.log:3371 through stderr.log:3371)
- ``benilla_app::perf::stats: frame hitch: 287 ms (main thread blocked this long)`` (x1; stderr.log:3347 through stderr.log:3347)
- ``benilla_app::perf::stats: frame hitch: 492 ms (main thread blocked this long)`` (x1; stderr.log:2154 through stderr.log:2154)
- ``benilla_app::perf::stats: frame hitch: 11604 ms (main thread blocked this long)`` (x1; stderr.log:975 through stderr.log:975)
- ``error: unrecognized escape sequence) — the filter is off`` (x1; stderr.log:65 through stderr.log:65)
- ``benilla_app::text_filter: chat profanity filter: the pattern set would not build (regex parse error:`` (x1; stderr.log:62 through stderr.log:62)
- ``benilla_app::entities: pet stat tables unavailable, happiness/loyalty stay blank: reading DBFilesClient\PetLoyalty.dbc: file not in patch chain: DBFilesClient\PetLoyalty.dbc`` (x1; stderr.log:142 through stderr.log:142)
- ``benilla_app::perf::stats: frame hitch: 537 ms (main thread blocked this long)`` (x1; stderr.log:171 through stderr.log:171)
- ``benilla_assets::m2: M2 particle data unavailable for mpq-loose://interface/glues/models/ui_mainmenu_northrend/ui_mainmenu_northrend.m2: Wrath lifespan/emission variation requires runtime support`` (x1; stderr.log:169 through stderr.log:169)
- ``benilla_app::sound::vocal: sound: 641 vocal UI error-speech rows`` (x1; stderr.log:28 through stderr.log:28)
- ``bevy_asset::io::source: AssetSourceId::Default does not have an AssetWatcher configured. Consider adding an "assets" directory.`` (x1; stderr.log:3 through stderr.log:3)
- ``benilla_app::text_filter: chat spam filter unavailable: applying SpamMessages schema (field-count mismatch?): schema has 3 fields but file has 2`` (x1; stderr.log:47 through stderr.log:47)
- ``error: repetition operator missing expression`` (x1; stderr.log:58 through stderr.log:58)
- ``benilla_app::text_filter: skipping chat profanity filter expression "{性}交" (record ID 11957): regex parse error:`` (x1; stderr.log:55 through stderr.log:55)
- ``benilla_app::perf::stats: frame hitch: 364 ms (main thread blocked this long)`` (x1; stderr.log:677 through stderr.log:677)
- ``benilla_app::perf::stats: frame hitch: 16889 ms (main thread blocked this long)`` (x1; stderr.log:672 through stderr.log:672)
- ``benilla_app::perf::stats: frame hitch: 366 ms (main thread blocked this long)`` (x1; stderr.log:678 through stderr.log:678)
- ``bevy_asset::io::source: Skip creating file watcher because path "D:\\Projects\\project-b-client\\target\\debug\\assets" does not exist.`` (x1; stderr.log:2 through stderr.log:2)
- ``benilla_app::perf::stats: frame hitch: 362 ms (main thread blocked this long)`` (x1; stderr.log:681 through stderr.log:681)
- ``benilla_app::ui_text::engine: ui_text: 'Fonts\FRIENDS.TTF' loaded as family "Friz Quadrata TT" (weight 400, style Normal) but 'A' shaped in FrizQuadrataTT instead — text in this face draws and measures in the wrong one`` (x1; stderr.log:295 through stderr.log:295)
- ``benilla_assets::m2: M2 particle data unavailable for mpq-loose://interface/glues/models/ui_orc/ui_orc.m2: Wrath lifespan/emission variation requires runtime support`` (x1; stderr.log:177 through stderr.log:177)
- ``bevy_asset::server::loaders: Multiple AssetLoaders found for Asset: Some(TypeId(0xe8ec642b9ee8b10b3da3674542b8529b)); Path: Some(mpq://maehne@cc); Extension: None`` (x1; stderr.log:666 through stderr.log:666)
- ``benilla_app::pipe_warm: pipeline warm: TIMED OUT with 41 pipelines pending — cover released`` (x1; stderr.log:671 through stderr.log:671)
- ``bevy_asset::server: Encountered an I/O error while loading asset: file not in profile `wm-hd`: maehne@cc`` (x1; stderr.log:667 through stderr.log:667)

## Launch Metadata
```text
pid=44664
binary_sha256=C88F1450C5A1F034E3427F4779DAB5ECC4EBA89F82B193BFA6891BADCA61AC7B
```

## UI Startup Summary (8 unique)
- ``ui_script: stock glue up in 291 ms on "charselect" (0 load failure(s))`` (x1)
- ``ui_script: stock glue up in 281 ms on "charselect" (0 load failure(s))`` (x1)
- ``ui_script: in-game UI up in 6425 ms (behind the cover: true, vm session: 5)`` (x1)
- ``ui_script: in-game UI up in 8593 ms (behind the cover: true, vm session: 9)`` (x1)
- ``ui_script: in-game UI up in 16415 ms (behind the cover: true, vm session: 3)`` (x1)
- ``ui_script: stock glue up in 3399 ms on "login" (0 load failure(s))`` (x1)
- ``ui_script: in-game UI up in 7270 ms (behind the cover: true, vm session: 7)`` (x1)
- ``ui_script: stock glue up in 298 ms on "charselect" (0 load failure(s))`` (x1)

## Missing Global Functions (Lua) (18 unique)
- ``GetUnitMaxHealthModifier`` (x13)
- ``GetRandomBGHonorCurrencyBonuses`` (x9)
- ``HasLFGRestrictions`` (x8)
- ``GetBackpackCurrencyInfo`` (x8)
- ``GetText`` (x7)
- ``CanShowAchievementUI`` (x7)
- ``GetCurrentArenaSeason`` (x6)
- ``GetInventoryItemsForSlot`` (x6)
- ``UnitInVehicle`` (x5)
- ``IsAtStableMaster`` (x5)
- ``GetVoiceStatus`` (x4)
- ``VoiceChat_Animate`` (x4)
- ``GetCombatRatingBonus`` (x4)
- ``RequestLFDPartyLockInfo`` (x4)
- ``IsBattlefieldArena`` (x2)
- ``ClearFocus`` (x2)
- ``BNGetNumFriendInvites`` (x2)
- ``GetSpellBonusDamage`` (x2)

## Missing Widget Methods (Lua) (5 unique)
- ``IsProtected`` (x18)
- ``GetDummyBag`` (x7)
- ``SetTracking`` (x5)
- ``SetLFGDungeonReward`` (x2)
- ``HasFocus`` (x2)

## Missing Frame / Table Fields (Lua) (1 unique)
- ``normalTexture`` (x2)

## Missing CVars (10 unique)
- ``talentFrameShown`` (x6)
- ``fctAuras`` (x4)
- ``showArenaEnemyCastbar`` (x4)
- ``UnitNameFriendlyTotemName`` (x4)
- ``showCastableDebuffs`` (x4)
- ``showTargetOfTarget`` (x4)
- ``showItemLevel`` (x4)
- ``fctDamageReduction`` (x4)
- ``nameplateShowEnemies`` (x2)
- ``nameplateShowFriends`` (x2)

## DBC Load / Schema Diagnostics (2 unique)
- ``benilla_app::ui_script::input: ui_script(input): runtime error: Interface\FrameXML\MainMenuBar.lua:400: attempt to perform arithmetic on local `exhaustionStateMultiplier' (a nil value)`` (x2)
- ``benilla_app::entities: pet stat tables unavailable, happiness/loyalty stay blank: reading DBFilesClient\PetLoyalty.dbc: file not in patch chain: DBFilesClient\PetLoyalty.dbc`` (x1)

## Asset / M2 / Skin / BLP Errors (21 unique)
- ``benilla_assets::m2: M2 particle data unavailable for mpq://world/generic/human/passive doodads/gryphonroost/gryphonroost01.m2: Wrath anisotropic/variable scale requires runtime support`` (x3)
- ``benilla_assets::m2: M2 particle data unavailable for mpq://world/generic/passivedoodads/particleemitters/blacksmith_smoke.m2: Wrath wind/tumble particles remain guarded`` (x3)
- ``benilla_assets::blp: BLP mpq://dungeons/textures/brian/abbey/piinetree01.blp: rejected mip 9; keeping 9 valid levels`` (x2)
- ``benilla_assets::m2: M2 particle data unavailable for mpq://world/generic/human/passive doodads/smokestack/smokestack.m2: Wrath wind/tumble particles remain guarded`` (x2)
- ``benilla_assets::m2: M2 particle data unavailable for mpq://particles/bloodspurts/bloodspurt.m2: Wrath geometry/recursive particles remain guarded`` (x2)
- ``benilla_assets::m2: M2 particle data unavailable for mpq://spells/bestowdisease_impact_chest.m2: Wrath geometry/recursive particles remain guarded`` (x1)
- ``bevy_asset::server: Encountered an I/O error while loading asset: file not in profile `wm-hd`: creature/nozdormudragon/nozdormu1.blp`` (x1)
- ``benilla_assets::m2: M2 particle data unavailable for mpq://world/azeroth/westfall/passivedoodads/lamppost/westfalllamppost02.m2: Wrath lifespan/emission variation requires runtime support`` (x1)
- ``benilla_assets::m2: M2 particle data unavailable for mpq-loose://interface/glues/models/ui_mainmenu_northrend/ui_mainmenu_northrend.m2: Wrath lifespan/emission variation requires runtime support`` (x1)
- ``benilla_assets::m2: M2 particle data unavailable for mpq://spells/lightning_precast_low_hand.m2: Wrath lifespan/emission variation requires runtime support`` (x1)
- ``benilla_assets::m2: M2 particle data unavailable for mpq://spells/poisonshot_missile.m2: Wrath anisotropic/variable scale requires runtime support`` (x1)
- ``benilla_assets::m2: M2 particle data unavailable for mpq-loose://interface/glues/models/ui_orc/ui_orc.m2: Wrath lifespan/emission variation requires runtime support`` (x1)
- ``benilla_assets::m2: M2 particle data unavailable for mpq://spells/frost_nova_area.m2: Wrath anisotropic/variable scale requires runtime support`` (x1)
- ``benilla_assets::m2: M2 particle data unavailable for mpq://creature/elementalearth2/elementalearth2.m2: Wrath lifespan/emission variation requires runtime support`` (x1)
- ``benilla_assets::m2: M2 particle data unavailable for mpq://spells/hunter_rapidfire.m2: Wrath anisotropic/variable scale requires runtime support`` (x1)
- ``benilla_app::entities::skin_composite: body skin composite finished without 1 layer(s): CompositeReport { missing: [CompositeMissingLayer { tile: (0, 0, 128, 64), candidates: ["Item\\TextureComponents\\ArmUpperTexture\\Robe_D_01Cyan_Sleeve_AU_U.blp", "Item\\TextureComponents\\ArmUpperTexture\\Robe_D_01Cyan_Sleeve_AU_F.blp"], errors: ["file not in patch chain: Item\\TextureComponents\\ArmUpperTexture\\Robe_D_01Cyan_Sleeve_AU_U.blp", "file not in patch chain: Item\\TextureComponents\\ArmUpperTexture\\Robe_D_01Cyan_Sleeve_AU_F.blp"] }] }`` (x1)
- ``bevy_asset::server: Encountered an I/O error while loading asset: file not in profile `wm-hd`: creature/nozdormudragon/nozdormu3.blp`` (x1)
- ``benilla_assets::m2: M2 particle data unavailable for mpq://spells/lightningbolt_impact_chest.m2: Wrath lifespan/emission variation requires runtime support`` (x1)
- ``benilla_assets::m2: M2 particle data unavailable for mpq-loose://interface/buttons/ui-autocastbutton.m2: Wrath spline particles remain unsupported`` (x1)
- ``benilla_assets::m2: M2 particle data unavailable for mpq://spells/frostbolt.m2: Wrath lifespan/emission variation requires runtime support`` (x1)
- ``bevy_asset::server: Encountered an I/O error while loading asset: file not in profile `wm-hd`: creature/nozdormudragon/nozdormu2.blp`` (x1)

## All Lua / UI Runtime Errors & Warnings (147 unique)
- ``benilla_app::ui_script::extract: ui_script: error handler itself failed: runtime error: Interface\FrameXML\UIPanelTemplates.lua:365: attempt to perform arithmetic on field `cursorOffset' (a nil value)`` (x490)
- ``benilla_app::ui_script::extract: ui_script: runtime error: Interface\AddOns\bartender4\libs\SimpleSticky.lua:179: attempt to perform arithmetic on local `xB' (a nil value)`` (x348)
- ``benilla_app::ui_script::addons: ui_script(FrameXML/Interface\FrameXML\ChannelFrame.xml): unknown template 'VoiceChatSpeakerTemplate' referenced by inherits; skipping`` (x168)
- ``benilla_app::ui_script::extract: ui_script: runtime error: Interface\FrameXML\MoneyFrame.lua:274: attempt to perform arithmetic on local `money' (a nil value)`` (x48)
- ``benilla_app::ui_script::addons: ui_script(GlueXML/Interface\GlueXML\CharacterCreate.xml): Couldn't find inherited node: QuestFontNormalHuge`` (x44)
- ``benilla_app::ui_script::addons: ui_script(FrameXML/Interface\FrameXML\PartyFrame.xml): unknown template 'VoiceChatSpeakerTemplate' referenced by inherits; skipping`` (x32)
- ``benilla_app::ui_script::extract: ui_script: unknown template 'VoiceChatSpeakerTemplate' referenced by inherits; skipping`` (x20)
- ``benilla_app::ui_script::input: ui_script(input): runtime error: Interface\FrameXML\MoneyFrame.lua:274: attempt to perform arithmetic on local `money' (a nil value)`` (x15)
- ``benilla_app::ui_script::input: ui_script(input): runtime error: Interface\FrameXML\RestrictedFrames.lua:99: attempt to call method `IsProtected' (a nil value)`` (x13)
- ``benilla_app::ui_script::extract: ui_script: runtime error: Interface\FrameXML\PaperDollFrame.lua:280: attempt to call global `GetUnitMaxHealthModifier' (a nil value)`` (x10)
- ``benilla_app::ui_script::extract: ui_script: runtime error: Interface\FrameXML\BattlefieldFrame.lua:509: attempt to call global `GetRandomBGHonorCurrencyBonuses' (a nil value)`` (x9)
- ``benilla_app::ui_script::lifecycle: ui_script: Wrath host source integration is incomplete; addon/API and rendered acceptance remain unverified`` (x9)
- ``benilla_app::ui_script::extract: ui_script: runtime error: Interface\FrameXML\ChannelFrame.lua:716: attempt to index local `texture' (a nil value)`` (x8)
- ``benilla_app::ui_script::extract: ui_script: SetOwner: unknown anchor 'ANCHOR_BOTTOM' (mode 0, ANCHOR_LEFT)`` (x7)
- ``benilla_app::bindings: bindings(TOGGLEACHIEVEMENT): runtime error: Interface\FrameXML\UIParent.lua:352: attempt to call global `CanShowAchievementUI' (a nil value)`` (x7)
- ``benilla_app::ui_script::input: ui_script(input): runtime error: Interface\AddOns\Bagnon\components\item.lua:27: attempt to call method `GetDummyBag' (a nil value)`` (x6)
- ``benilla_app::bindings: bindings(TOGGLEBACKPACK): runtime error: ...terface\AddOns\Blizzard_TokenUI\Blizzard_TokenUI.lua:180: attempt to call global `GetBackpackCurrencyInfo' (a nil value)`` (x5)
- ``benilla_app::ui_script::extract: ui_script: runtime error: Interface\FrameXML\PaperDollFrame.lua:1793: attempt to call global `GetInventoryItemsForSlot' (a nil value)`` (x5)
- ``benilla_app::ui_script::extract: ui_script: runtime error: Interface\FrameXML\RestrictedFrames.lua:99: attempt to call method `IsProtected' (a nil value)`` (x5)
- ``benilla_app::ui_script::extract: ui_script: runtime error: Interface\FrameXML\PetStable.lua:199: attempt to call global `IsAtStableMaster' (a nil value)`` (x5)
- ``benilla_app::ui_script::input: ui_script(input): runtime error: [string "MiniMapTrackingButton:OnEnter"]:3: attempt to call method `SetTracking' (a nil value)`` (x5)
- ``benilla_app::ui_script::extract: ui_script: runtime error: Interface\FrameXML\PVPFrame.lua:88: attempt to call global `GetCurrentArenaSeason' (a nil value)`` (x5)
- ``benilla_app::ui_script::extract: ui_script: runtime error: Interface\FrameXML\PetFrame.lua:71: attempt to call global `UnitInVehicle' (a nil value)`` (x5)
- ``benilla_app::ui_script::addons: ui_script(GlueXML/Interface\GlueXML\CharacterCreate.xml): SETALLPOINTS set to true in frame with anchors (ignored)`` (x4)
- ``benilla_app::ui_script::extract: ui_script: runtime error: Interface\FrameXML\OptionsPanelTemplates.lua:335: Couldn't find CVar named 'showItemLevel'`` (x4)
- ``benilla_app::ui_script::addons: diag[ADDON] newlevelframe is DISABLED in AddOn enable configuration`` (x4)
- ``benilla_app::ui_script::addons: diag[ADDON] gearscorelite is DISABLED in AddOn enable configuration`` (x4)
- ``benilla_app::ui_script::addons: diag[ADDON] handynotes is DISABLED in AddOn enable configuration`` (x4)
- ``benilla_app::ui_script::addons: diag[ADDON] Details_EncounterDetails is DISABLED in AddOn enable configuration`` (x4)
- ``benilla_app::ui_script::addons: ui_script(GlueXML/Interface\GlueXML\CharacterCreate.xml): Texture <unnamed>: Unable to load texture file Interface\Glues\CharacterCreate\CharacterCreate-Arrow`` (x4)
- ``benilla_app::ui_script::addons: ui_script(GlueXML/Interface\GlueXML\CharacterCreate.xml): Couldn't find relative frame: CharCreateClassButton6`` (x4)
- ``benilla_app::ui_script::addons: diag[ADDON] GatherMate is DISABLED in AddOn enable configuration`` (x4)
- ``benilla_app::ui_script::addons: diag[ADDON] errorfilter is DISABLED in AddOn enable configuration`` (x4)
- ``benilla_app::ui_script::extract: ui_script: runtime error: Interface\FrameXML\InterfaceOptionsFrame.lua:359: Couldn't find CVar named 'fctAuras'`` (x4)
- ``benilla_app::ui_script::addons: diag[ADDON] WMBridge is DISABLED in AddOn enable configuration`` (x4)
- ``benilla_app::ui_script::addons: diag[ADDON] postal is DISABLED in AddOn enable configuration`` (x4)
- ``benilla_app::ui_script::addons: ui_script(GlueXML/Interface\GlueXML\CharacterCreate.xml): Texture CharCreatePreviewFrameLeftEdge: Unable to load texture file Interface\Glues\CharacterCreate\Frame-Sides`` (x4)
- ``benilla_app::ui_script::addons: ui_script(GlueXML/Interface\GlueXML\CharacterCreate.xml): Texture Bottom: Unable to load texture file Interface\Glues\CharacterCreate\AlternateForm`` (x4)
- ``benilla_app::ui_script::addons: diag[ADDON] Quartz is DISABLED in AddOn enable configuration`` (x4)
- ``benilla_app::ui_script::extract: ui_script: runtime error: Interface\FrameXML\PlayerFrame.lua:372: attempt to call global `VoiceChat_Animate' (a nil value)`` (x4)
- ``benilla_app::ui_script::addons: diag[ADDON] WMQuestieCompat is DISABLED in AddOn enable configuration`` (x4)
- ``benilla_app::ui_script::addons: ui_script(FrameXML/Interface\FrameXML\WorldMapFrame.xml): Unknown frame type: QuestPOIFrame`` (x4)
- ``benilla_app::ui_script::extract: ui_script: runtime error: [string "LFDQueueFrame:OnShow"]:5: attempt to call global `RequestLFDPartyLockInfo' (a nil value)`` (x4)
- ``benilla_app::ui_script::addons: diag[ADDON] Questie-335 is DISABLED in AddOn enable configuration`` (x4)
- ``benilla_app::ui_script::addons: diag[ADDON] Routes is DISABLED in AddOn enable configuration`` (x4)
- ``benilla_app::ui_script::addons: ui_script(GlueXML/Interface\GlueXML\CharacterCreate.xml): CharacterTemplateConfirmDialogContents: text="TEMPLATE_CHARACTER_CHOOSE_FACTION" is shaped like a GlobalStrings key but no such string global exists — showing the key`` (x4)
- ``benilla_app::ui_script::addons: diag[ADDON] fizzle is DISABLED in AddOn enable configuration`` (x4)
- ``benilla_app::ui_script::addons: ui_script(FrameXML/Interface\FrameXML\KnowledgeBaseFrame.xml): KnowledgeBaseArticleListFrame: text="ARTICLE_COUNT" is shaped like a GlobalStrings key but no such string global exists — showing the key`` (x4)
- ``benilla_app::ui_script::addons: diag[ADDON] Details_ChartViewer is DISABLED in AddOn enable configuration`` (x4)
- ``benilla_app::ui_script::extract: ui_script: runtime error: Interface\FrameXML\PlayerFrame.lua:64: attempt to call global `HasLFGRestrictions' (a nil value)`` (x4)
- ``benilla_app::ui_script::addons: diag[ADDON] blizzmove is DISABLED in AddOn enable configuration`` (x4)
- ``benilla_app::ui_script::addons: diag[ADDON] Skillet is DISABLED in AddOn enable configuration`` (x4)
- ``benilla_app::ui_script::addons: ui_script(FrameXML/Interface\FrameXML\PVPFrame.xml): PVPFrameOffSeason: text="CANHAZDUMMYTEXT" is shaped like a GlobalStrings key but no such string global exists — showing the key`` (x4)
- ``benilla_app::ui_script::addons: diag[ADDON] aux-addon is DISABLED in AddOn enable configuration`` (x4)
- ``benilla_app::ui_script::input: ui_script(input): runtime error: ...rface\AddOns\Blizzard_TalentUI\Blizzard_TalentUI.lua:303: Couldn't find CVar named 'talentFrameShown'`` (x4)
- ``benilla_app::ui_script::addons: ui_script(FrameXML/Interface\FrameXML\HonorFrame.xml): HonorFrame: text="HONOR_THISWEEK" is shaped like a GlobalStrings key but no such string global exists — showing the key`` (x4)
- ``benilla_app::ui_script::addons: ui_script(FrameXML/Interface\FrameXML\GameTime.xml): GameTimeFrame: OnLoad: runtime error: Interface\FrameXML\GameTime.lua:141: attempt to index a nil value`` (x4)
- ``benilla_app::ui_script::addons: diag[ADDON] tweakwow is DISABLED in AddOn enable configuration`` (x4)
- ``benilla_app::ui_script::extract: ui_script: runtime error: Interface\FrameXML\OptionsPanelTemplates.lua:335: Couldn't find CVar named 'showCastableDebuffs'`` (x4)
- ``benilla_app::ui_script::extract: ui_script: runtime error: Interface\FrameXML\PartyMemberFrame.lua:256: attempt to call global `GetVoiceStatus' (a nil value)`` (x4)
- ``benilla_app::ui_script::addons: diag[ADDON] Details_TinyThreat is DISABLED in AddOn enable configuration`` (x4)
- ``benilla_app::ui_script::addons: diag[ADDON] cromulent is DISABLED in AddOn enable configuration`` (x4)
- ``benilla_app::ui_script::addons: diag[ADDON] !!!garbageprotector is DISABLED in AddOn enable configuration`` (x4)
- ``benilla_app::ui_script::addons: ui_script(GlueXML/Interface\GlueXML\CharacterCreate.xml): CharacterCreateTooltip2: Unknown frame strata: TOOLTIP2`` (x4)
- ``benilla_app::ui_script::addons: diag[ADDON] Details is DISABLED in AddOn enable configuration`` (x4)
- ``benilla_app::ui_script::addons: diag[ADDON] Specialni Znaky is DISABLED in AddOn enable configuration`` (x4)
- ``benilla_app::ui_script::extract: ui_script: runtime error: Interface\FrameXML\OptionsPanelTemplates.lua:335: Couldn't find CVar named 'showTargetOfTarget'`` (x4)
- ``benilla_app::ui_script::addons: diag[ADDON] !blizzbugssuck is DISABLED in AddOn enable configuration`` (x4)
- ``benilla_app::ui_script::addons: ui_script(GlueXML/Interface\GlueXML\CharacterCreate.xml): CharCreateRaceFrame: text="CUSTOMIZE_OPTIONS" is shaped like a GlobalStrings key but no such string global exists — showing the key`` (x4)
- ``benilla_app::ui_script::extract: ui_script: runtime error: Interface\FrameXML\OptionsPanelTemplates.lua:335: Couldn't find CVar named 'showArenaEnemyCastbar'`` (x4)
- ``benilla_app::ui_script::addons: diag[ADDON] MultiBot is DISABLED in AddOn enable configuration`` (x4)
- ``benilla_app::ui_script::addons: ui_script(GlueXML/Interface\GlueXML\CharacterCreate.xml): Texture CharacterCreateAlternateFormTopHighlight: Unable to load texture file Interface\Glues\CharacterCreate\AlternateForm`` (x4)
- ``benilla_app::ui_script::addons: ui_script(FrameXML/Interface\FrameXML\PVPBattlegroundFrame.xml): PVPBattlegroundFrameInfoScrollFrameChildFrame: text="BATTLEGROUND_LORETEXT_1" is shaped like a GlobalStrings key but no such string global exists — showing the key`` (x4)
- ``benilla_app::ui_script::addons: diag[ADDON] Details_SunderCount is DISABLED in AddOn enable configuration`` (x4)
- ``benilla_app::ui_script::addons: ui_script(FrameXML/Interface\FrameXML\PlayerFrame.xml): unknown template 'VoiceChatSpeakerTemplate' referenced by inherits; skipping`` (x4)
- ``benilla_app::ui_script::addons: diag[ADDON] leatrix_gfx is DISABLED in AddOn enable configuration`` (x4)
- ``benilla_app::ui_script::addons: diag[ADDON] MacroBank is DISABLED in AddOn enable configuration`` (x4)
- ``benilla_app::ui_script::addons: diag[ADDON] zzzmopitemtooltips is DISABLED in AddOn enable configuration`` (x4)
- ``benilla_app::ui_script::addons: ui_script(GlueXML/Interface\GlueXML\CharacterCreate.xml): Texture CharacterCreateAlternateFormTopPushedTexture: Unable to load texture file Interface\Glues\CharacterCreate\AlternateForm`` (x4)
- ``benilla_app::ui_script::addons: ui_script(FrameXML/Interface\FrameXML\RaidFrame.xml): RaidInfoInstanceLabel: text="TEST" is shaped like a GlobalStrings key but no such string global exists — showing the key`` (x4)
- ``benilla_app::ui_script::addons: ui_script(GlueXML/Interface\GlueXML\CharacterCreate.xml): Texture CharacterCreateAlternateFormBottomPushedTexture: Unable to load texture file Interface\Glues\CharacterCreate\AlternateForm`` (x4)
- ``benilla_app::ui_script::addons: diag[ADDON] mapster is DISABLED in AddOn enable configuration`` (x4)
- ``benilla_app::ui_script::addons: diag[ADDON] GatherMate2 is DISABLED in AddOn enable configuration`` (x4)
- ``benilla_app::ui_script::addons: diag[ADDON] ignoremore is DISABLED in AddOn enable configuration`` (x4)
- ``benilla_app::ui_script::addons: diag[ADDON] GatherHud is DISABLED in AddOn enable configuration`` (x4)
- ``benilla_app::ui_script::addons: ui_script(FrameXML/Interface\FrameXML\KnowledgeBaseFrame.xml): KnowledgeBaseErrorFrame: text="ERROR" is shaped like a GlobalStrings key but no such string global exists — showing the key`` (x4)
- ``benilla_app::ui_script::extract: ui_script: runtime error: Interface\FrameXML\ReputationFrame.lua:167: attempt to call global `GetText' (a nil value)`` (x4)
- ``benilla_app::ui_script::extract: ui_script: runtime error: Interface\FrameXML\RaidFrame.lua:70: attempt to call global `HasLFGRestrictions' (a nil value)`` (x4)
- ``benilla_app::ui_script::extract: ui_script: runtime error: Interface\FrameXML\GameTime.lua:158: attempt to compare nil with number`` (x4)
- ``benilla_app::ui_script::addons: ui_script(GlueXML/Interface\GlueXML\CharacterCreate.xml): Texture CharacterCreateAlternateFormBottomHighlight: Unable to load texture file Interface\Glues\CharacterCreate\AlternateForm`` (x4)
- ``benilla_app::ui_script::extract: ui_script: runtime error: Interface\FrameXML\OptionsPanelTemplates.lua:335: Couldn't find CVar named 'fctDamageReduction'`` (x4)
- ``benilla_app::ui_script::addons: ui_script(GlueXML/Interface\GlueXML\CharacterCreate.xml): Texture CharacterCreateAlternateFormTopTop: Unable to load texture file Interface\Glues\CharacterCreate\AlternateForm`` (x4)
- ``benilla_app::ui_script::addons: ui_script(FrameXML/Interface\FrameXML\VideoOptionsFrame.xml): VideoOptionsFrame: text="OPTIONS" is shaped like a GlobalStrings key but no such string global exists — showing the key`` (x4)
- ``benilla_app::ui_script::addons: diag[ADDON] pawn is DISABLED in AddOn enable configuration`` (x4)
- ``benilla_app::ui_script::extract: ui_script: runtime error: Interface\FrameXML\OptionsPanelTemplates.lua:335: Couldn't find CVar named 'UnitNameFriendlyTotemName'`` (x4)
- ``benilla_app::ui_script::addons: diag[ADDON] libstratafix is DISABLED in AddOn enable configuration`` (x4)
- ``benilla_app::ui_script::extract: ui_script: Post-hook: Interface\AddOns\bartender4\ActionButton.lua:453: attempt to index local `icon' (a nil value)`` (x4)
- ``benilla_app::ui_script::addons: ui_script(FrameXML/Interface\FrameXML\BattlefieldFrame.xml): BattlefieldFrameInfoScrollFrameChildFrame: text="BATTLEGROUND_LORETEXT_1" is shaped like a GlobalStrings key but no such string global exists — showing the key`` (x4)
- ``benilla_app::ui_script::addons: ui_script(FrameXML/Interface\FrameXML\AudioOptionsFrame.xml): AudioOptionsFrame: text="OPTIONS" is shaped like a GlobalStrings key but no such string global exists — showing the key`` (x4)
- ``benilla_app::ui_script::addons: diag[ADDON] combatlogfix is DISABLED in AddOn enable configuration`` (x4)
- ``benilla_app::ui_script::addons: diag[ADDON] cerror is DISABLED in AddOn enable configuration`` (x4)
- ``benilla_app::ui_script::addons: diag[ADDON] Details_DeathGraphs is DISABLED in AddOn enable configuration`` (x4)
- ``benilla_app::ui_script::addons: diag[ADDON] Details_TimeLine is DISABLED in AddOn enable configuration`` (x4)
- ``benilla_app::ui_script::extract: ui_script: CreateFrame("Button", "TargetFrameBuff1", inherits="TargetBuffFrameTemplate"): 'TargetBuffFrameTemplate' is a <Frame>, but a Button was created; the frame keeps the kind CreateFrame was given, so the <Frame>-only parts of the template do not apply`` (x3)
- ``benilla_app::ui_script::addons: diag[ADDON] bartender4 is DISABLED in AddOn enable configuration`` (x3)
- ``benilla_app::ui_script::input: ui_script(input): runtime error: Interface\FrameXML\ReputationFrame.lua:167: attempt to call global `GetText' (a nil value)`` (x3)
- ``benilla_app::ui_script::extract: ui_script: CreateFrame("Button", "TargetFrameBuff2", inherits="TargetBuffFrameTemplate"): 'TargetBuffFrameTemplate' is a <Frame>, but a Button was created; the frame keeps the kind CreateFrame was given, so the <Frame>-only parts of the template do not apply`` (x3)
- ``benilla_app::ui_script::input: ui_script(input): runtime error: Interface\FrameXML\PaperDollFrame.lua:280: attempt to call global `GetUnitMaxHealthModifier' (a nil value)`` (x3)
- ``benilla_app::ui_script::addons: diag[ADDON] Bagnon_Forever is DISABLED in AddOn enable configuration`` (x3)
- ``benilla_app::ui_script::addons: diag[ADDON] Bagnon is DISABLED in AddOn enable configuration`` (x3)
- ``benilla_app::ui_script::addons: diag[ADDON] Bagnon_Tooltips is DISABLED in AddOn enable configuration`` (x3)
- ``benilla_app::ui_script::extract: ui_script: CreateFrame("Button", "TargetFrameBuff9", inherits="TargetBuffFrameTemplate"): 'TargetBuffFrameTemplate' is a <Frame>, but a Button was created; the frame keeps the kind CreateFrame was given, so the <Frame>-only parts of the template do not apply`` (x2)
- ``benilla_app::bindings: bindings(NAMEPLATES): runtime error: [string "NAMEPLATES"]:7: Couldn't find CVar named 'nameplateShowEnemies'`` (x2)
- ``benilla_app::ui_script::extract: ui_script: runtime error: Interface\FrameXML\TargetFrame.lua:201: attempt to call global `ClearFocus' (a nil value)`` (x2)
- ``benilla_app::ui_script::input: ui_script(input): runtime error: Interface\FrameXML\MainMenuBar.lua:400: attempt to perform arithmetic on local `exhaustionStateMultiplier' (a nil value)`` (x2)
- ``benilla_app::ui_script::extract: ui_script: CreateFrame("Button", "TargetFrameBuff4", inherits="TargetBuffFrameTemplate"): 'TargetBuffFrameTemplate' is a <Frame>, but a Button was created; the frame keeps the kind CreateFrame was given, so the <Frame>-only parts of the template do not apply`` (x2)
- ``benilla_app::ui_script::input: ui_script(input): runtime error: Interface\FrameXML\PaperDollFrame.lua:919: attempt to call global `GetSpellBonusDamage' (a nil value)`` (x2)
- ``benilla_app::ui_script::input: ui_script(input): runtime error: Interface\FrameXML\PaperDollFrame.lua:886: attempt to call global `GetCombatRatingBonus' (a nil value)`` (x2)
- ``benilla_app::bindings: bindings(FRIENDNAMEPLATES): runtime error: [string "FRIENDNAMEPLATES"]:7: Couldn't find CVar named 'nameplateShowFriends'`` (x2)
- ``benilla_app::ui_script::extract: ui_script: runtime error: Interface\AddOns\bartender4\ActionButton.lua:295: attempt to index field `normalTexture' (a nil value)`` (x2)
- ``benilla_app::ui_script::extract: ui_script: runtime error: ...rface\AddOns\Blizzard_TalentUI\Blizzard_TalentUI.lua:303: Couldn't find CVar named 'talentFrameShown'`` (x2)
- ``benilla_app::ui_script::extract: ui_script: CreateFrame("Button", "TargetFrameBuff6", inherits="TargetBuffFrameTemplate"): 'TargetBuffFrameTemplate' is a <Frame>, but a Button was created; the frame keeps the kind CreateFrame was given, so the <Frame>-only parts of the template do not apply`` (x2)
- ``benilla_app::ui_script::extract: ui_script: CreateFrame("Button", "TargetFrameBuff7", inherits="TargetBuffFrameTemplate"): 'TargetBuffFrameTemplate' is a <Frame>, but a Button was created; the frame keeps the kind CreateFrame was given, so the <Frame>-only parts of the template do not apply`` (x2)
- ``benilla_app::ui_script::extract: ui_script: CreateFrame("Button", "TargetFrameBuff3", inherits="TargetBuffFrameTemplate"): 'TargetBuffFrameTemplate' is a <Frame>, but a Button was created; the frame keeps the kind CreateFrame was given, so the <Frame>-only parts of the template do not apply`` (x2)
- ``benilla_app::ui_script::input: ui_script(input): runtime error: [string "LFDQueueFrameRandomScrollFrameChildFrameIte..."]:3: attempt to call method `SetLFGDungeonReward' (a nil value)`` (x2)
- ``benilla_app::ui_script::extract: ui_script: CreateFrame("Button", "TargetFrameBuff8", inherits="TargetBuffFrameTemplate"): 'TargetBuffFrameTemplate' is a <Frame>, but a Button was created; the frame keeps the kind CreateFrame was given, so the <Frame>-only parts of the template do not apply`` (x2)
- ``benilla_app::ui_script::extract: ui_script: runtime error: Interface\FrameXML\ArenaFrame.lua:11: attempt to call global `IsBattlefieldArena' (a nil value)`` (x2)
- ``benilla_app::ui_script::extract: ui_script: CreateFrame("Button", "TargetFrameBuff5", inherits="TargetBuffFrameTemplate"): 'TargetBuffFrameTemplate' is a <Frame>, but a Button was created; the frame keeps the kind CreateFrame was given, so the <Frame>-only parts of the template do not apply`` (x2)
- ``benilla_app::ui_script::extract: ui_script: runtime error: Interface\FrameXML\LFRFrame.lua:45: attempt to call method `HasFocus' (a nil value)`` (x2)
- ``benilla_app::ui_script::extract: ui_script: CreateFrame("CheckButton", "KeyboundDialogOkay", inherits="OptionsButtonTemplate"): 'OptionsButtonTemplate' is a <Button>, but a CheckButton was created; the frame keeps the kind CreateFrame was given, so the <Button>-only parts of the template do not apply`` (x1)
- ``benilla_app::ui_script::input: ui_script(input): runtime error: Interface\FrameXML\PVPFrame.lua:88: attempt to call global `GetCurrentArenaSeason' (a nil value)`` (x1)
- ``benilla_app::ui_script::extract: ui_script: CreateFrame("CheckButton", "KeyboundDialogCancel", inherits="OptionsButtonTemplate"): 'OptionsButtonTemplate' is a <Button>, but a CheckButton was created; the frame keeps the kind CreateFrame was given, so the <Button>-only parts of the template do not apply`` (x1)
- ``benilla_app::ui_script::addons: ui_script(Bagnon/components.xml): <Script file="components\item.lua">: runtime error: Interface\AddOns\Bagnon\components\item.lua:562: bad argument #2 to `format' (string expected, got nil)`` (x1)
- ``benilla_app::ui_script::extract: ui_script: runtime error: Interface\AddOns\Bagnon\components\item.lua:27: attempt to call method `GetDummyBag' (a nil value)`` (x1)
- ``benilla_app::ui_script::extract: ui_script: Post-hook: Interface\AddOns\bartender4\ActionButton.lua:377: attempt to compare nil with number`` (x1)
- ``benilla_app::ui_script::extract: ui_script: CreateFrame("Button", "TargetFrameDebuff1", inherits="TargetDebuffFrameTemplate"): 'TargetDebuffFrameTemplate' is a <Frame>, but a Button was created; the frame keeps the kind CreateFrame was given, so the <Frame>-only parts of the template do not apply`` (x1)
- ``benilla_app::ui_script::input: ui_script(input): runtime error: ...terface\AddOns\Blizzard_TokenUI\Blizzard_TokenUI.lua:180: attempt to call global `GetBackpackCurrencyInfo' (a nil value)`` (x1)
- ``benilla_app::ui_script::extract: ui_script: runtime error: Interface\FrameXML\FriendsFrame.lua:757: attempt to call global `BNGetNumFriendInvites' (a nil value)`` (x1)
- ``benilla_app::ui_script::extract: ui_script: runtime error: ...terface\AddOns\Blizzard_TokenUI\Blizzard_TokenUI.lua:180: attempt to call global `GetBackpackCurrencyInfo' (a nil value)`` (x1)
- ``benilla_app::ui_script::input: ui_script(input): runtime error: Interface\FrameXML\PaperDollFrame.lua:465: attempt to call global `GetCombatRatingBonus' (a nil value)`` (x1)
- ``benilla_app::ui_script::extract: ui_script: CreateFrame("Button", "TargetFrameBuff10", inherits="TargetBuffFrameTemplate"): 'TargetBuffFrameTemplate' is a <Frame>, but a Button was created; the frame keeps the kind CreateFrame was given, so the <Frame>-only parts of the template do not apply`` (x1)
- ``benilla_app::ui_script::input: ui_script(input): runtime error: Interface\FrameXML\FriendsFrame.lua:757: attempt to call global `BNGetNumFriendInvites' (a nil value)`` (x1)
- ``benilla_app::ui_script::extract: ui_script: runtime error: Interface\FrameXML\MainMenuBar.lua:317: attempt to compare number with nil`` (x1)
- ``benilla_app::ui_script::input: ui_script(input): runtime error: Interface\FrameXML\PaperDollFrame.lua:646: attempt to call global `GetCombatRatingBonus' (a nil value)`` (x1)
- ``benilla_app::ui_script::extract: ui_script: CreateFrame("CheckButton", "Bartender4DialogLock", inherits="OptionsButtonTemplate"): 'OptionsButtonTemplate' is a <Button>, but a CheckButton was created; the frame keeps the kind CreateFrame was given, so the <Button>-only parts of the template do not apply`` (x1)
- ``benilla_app::bindings: bindings(OPENALLBAGS): runtime error: ...terface\AddOns\Blizzard_TokenUI\Blizzard_TokenUI.lua:180: attempt to call global `GetBackpackCurrencyInfo' (a nil value)`` (x1)
- ``benilla_app::ui_script::input: ui_script(input): runtime error: Interface\FrameXML\PaperDollFrame.lua:1793: attempt to call global `GetInventoryItemsForSlot' (a nil value)`` (x1)


