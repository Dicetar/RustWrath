# Trade-skill links (end to end)

Status: committed locally on `gemini/trade-skill-links`; blocked on push (no GitHub credentials in sandbox)
Owner: Gemini
Branch: gemini/trade-skill-links
Issue: none
Next: push `gemini/trade-skill-links` from an environment with GitHub push credentials and verify GitHub CI (including `benilla-app` tests).

## Status

Implemented end to end (Parts A, B, and C), committed locally on `gemini/trade-skill-links`, and verified locally for formatting, `cargo check`, `cargo clippy --lib --tests`, `benilla-formats`, and `benilla-ui` tests. Executing the `benilla-app` test binary locally OOMed this 12 GB RAM-backed sandbox container during `rustc` codegen, and `git push` from this sandbox is blocked by missing GitHub credentials (`fatal: could not read Username for 'https://github.com': terminal prompts disabled`).

## Owner: Gemini

## Branch

`gemini/trade-skill-links` (rebased cleanly onto `origin/main` at `fdcbd82`; zero overlapping files with `origin/main` commits after `7ac7c8a`).

## What changed

### Part A — Your own link (`GetTradeSkillListLink()`)
- **`crates/benilla-formats/src/skill_lines/mod.rs` & `crates/benilla-formats/src/lib.rs`**:
  - Loaded Wrath `SkillLine.dbc` column 55 (`WRATH_COL_SL_CAN_LINK`, `SkillLineRec::m_canLink`) into `SkillLineInfo::can_link` (false on 22-column vanilla rows).
  - Added `SkillLineLinkAbility` (`spell_id`, `race_mask`, `class_mask`, `trivial_low`, `trivial_high`) and `build_link_abilities()` matching `SpellTableBuildSkillLineLinkInfo` (`0x00812030`) and `SpellTableGetSkillLineLinkInfo` (`0x008104A0`): scans `SkillLineAbility.dbc` in raw file order and retains the first contiguous run per linkable `SkillLine` (`m_canLink != 0`).
  - Added `SkillLineCatalog::can_link(line_id)`, `SkillLineCatalog::link_abilities(line_id)`, and synthetic test builders `with_can_link` / `with_link_abilities`.
- **`crates/benilla-ui/src/script/tradeskill/link.rs`**:
  - Implemented the 12340 trade-skill link encoder/parser/decoder using exact reference tables `BASE64_ENCODE` (`0x00A1B640`, `A-Za-z0-9+/`, no `=` padding) and `BASE64_DECODE` (`0x00A1B688`), LSB-first 6-bit packing, `match_race_class` (`0x00810320`), and canonical formatting `|cffffd000|Htrade:<spellId>:<curRank>:<maxRank>:<guidHex>:<bitmap>|h[<Name>]|h|r` (uppercase unpadded hex GUID).
  - Input owners for `TradeSkillLinkSource`:
    - `spell_id`: `TradeSkillOpen` (`crates/benilla-app/src/ui_tradeskill.rs`, captured from the `SPELL_EFFECT_TRADE_SKILL` opener cast).
    - `spell_name`: `Spells` (`benilla_formats::SpellCatalog`, `Spell.dbc` name of `spell_id`).
    - `can_link`: `SkillLines` (`benilla_formats::SkillLineCatalog`, `SkillLine.dbc` column 55 `m_canLink`).
    - `cur_rank`, `max_rank`: `ObjectStore` (`crates/benilla-app/src/net.rs`, `PLAYER_SKILL_INFO` + permanent bonus when nonzero, matching `CGPlayer_C::GetSkillValue` `0x0051A250` and `GetSkillMaxRank` `0x00594710`).
    - `player_guid`: `SelfGuid` / `SelfPlayer` (`crates/benilla-app/src/net.rs`).
    - `player_race`, `player_class`: `ObjectStore` (`UNIT_FIELD_BYTES_0`).
    - `ability_rows`: `SkillLines` (`benilla_formats::SkillLineCatalog`, first contiguous raw-DBC run of `SkillLineAbility.dbc` rows for the open skill line).
    - `known_spells`: `PlayerActions` (`crates/benilla-app/src/ui_action/mod.rs`, populated from `SMSG_INITIAL_SPELLS` / `SMSG_LEARNED_SPELL` / `SMSG_REMOVED_SPELL`).
- **`crates/benilla-ui/src/script/tradeskill/api.rs`, `mod.rs`, `model.rs`, `crates/benilla-ui/src/script/mod.rs`**:
  - Wired `GetTradeSkillListLink()` (`0x005DC0B0` → `0x005DB060`) to encode the player's own open linkable profession from `TradeSkillLinkSource`, return the stored linked chat string when viewing a linked list, and return `nil` when closed or when `!can_link` (removed the old unsupported-native nil stub).
  - Added `UiScript::set_trade_skill_link_source` and re-exported link types/helpers from `benilla_ui::script`.

### Part B — Clicking someone's link (`SetItemRef` / `GameTooltip:SetHyperlink`)
- Confirmed from 12340 reference that **no `SMSG_TRADE_SKILL` opcode exists in 3.3.5a**: clicking a `|Htrade:` link in chat (`SetItemRef` → `GameTooltip:SetHyperlink`) resolves the owner's GUID through the existing `NameCache` (`CMSG_NAME_QUERY` / `SMSG_NAME_QUERY_RESPONSE`) and decodes the link's bitmap locally in the client.
- **`crates/benilla-ui/src/script/tooltip_item/mod.rs`**:
  - Intercepted `"trade:"` hyperlinks in `GameTooltip:SetHyperlink` (`0x0062DAE0`, `Tooltip_Scripts.cpp:634`) and dispatched `TradeSkillCall::OpenLink(link)` without showing the tooltip.
- **`crates/benilla-app/src/script_calls.rs` & `crates/benilla-app/src/ui_tradeskill.rs`**:
  - Routed `ScriptCall::TradeSkill(TradeSkillCall::OpenLink(link))` to `TradeSkillSteps::open_link`, staging the parsed link on `TradeSkillOpen::pending_link`.
  - Resolved `pending_link` in `feed_trade_skill` via `NameCache::resolve_player(guid, Ask::New(&commands))` (emitting `CMSG_NAME_QUERY` once if uncached and activating once `SMSG_NAME_QUERY_RESPONSE` lands), validating `can_link` and `bitmap.len() == bitmap_char_len(rows.len())`, disarming any armed repeat, decoding set `SkillLineLinkAbility` indices filtered by `SPELL_ATTR_IS_TRADESKILL` (`0x20`), resolving recipes with each row's `(trivial_low, trivial_high)`, and calling `UiScript::set_trade_skill_link(Some((owner, full_link)))`.
  - Updated `open_trade_skill` toggle rule (`0x005DE120` line 1353) so opening your own profession while viewing a linked window of the same skill line replaces the linked list instead of toggling closed; tracked `open_nonce` so switching between own and linked lists triggers `TRADE_SKILL_SHOW`.
  - Grouped read-only `feed_trade_skill` inputs into `TradeSkillFeedCtx` `SystemParam` to stay within Bevy's 16-parameter system limit, and updated `READS_FRAME_FINAL` in `crates/benilla-app/src/lanes/guard_tests.rs` to list `names::NameCache` and `net::SelfGuid`.
- **Read-only guard and clearing on close (`crates/benilla-ui/src/script/tradeskill/api.rs`, `mod.rs`, `model.rs`)**:
  - `IsTradeSkillLinked()` (`0x005DA310`) returns `(1, owner)` (or `(1, nil)` if empty) while viewing a link and `(nil, nil)` otherwise.
  - `DoTradeSkill()` refuses to craft or arm repeats whenever a linked list is active (`trade_skill_link.is_some()`) or was the source of the held closed list (`trade_skill_held_is_link`), until an own profession replaces it.
  - `CloseTradeSkill()` (`0x005DA8B0`), `set_trade_skill(None)`, `reset_crafts_on_world_enter()`, and `on_session_end` clear linked state and own link source.

### Part C — Round trip & synthetic tests (13 new tests, zero real game data)
- `benilla-formats`:
  - `skill_lines::tests::trade_skill_link_builds_first_contiguous_run_only_for_linkable_lines`
- `benilla-ui`:
  - `script::tradeskill::tests::trade_skill_link_bitmap_encode_uses_12340_alphabet_lsb_order_and_race_class_filter`
  - `script::tradeskill::tests::trade_skill_link_bitmap_decode_validates_length_ascii_and_table`
  - `script::tradeskill::tests::trade_skill_link_encode_parse_decode_round_trip`
  - `script::tradeskill::tests::trade_skill_link_get_list_link_format_and_can_link_gate`
  - `script::tradeskill::tests::trade_skill_link_linked_window_reports_owner_and_refuses_to_craft`
  - `script::tradeskill::tests::trade_skill_link_close_clears_linked_state_and_blocks_post_close_craft`
  - `script::tradeskill::tests::trade_skill_link_set_hyperlink_dispatches_open_link_without_showing_tooltip`
- `benilla-app`:
  - `ui_tradeskill::tests::trade_skill_link_own_profession_encodes_from_app_dbc_rows_and_known_spells`
  - `ui_tradeskill::tests::trade_skill_link_clicking_cached_link_opens_read_only_window_and_clears_on_close`
  - `ui_tradeskill::tests::trade_skill_link_clicking_uncached_link_queries_name_then_opens_on_response`
  - `ui_tradeskill::tests::trade_skill_link_end_to_end_round_trip_recovers_exact_known_recipes`
  - `ui_tradeskill::tests::trade_skill_link_rejects_malformed_unlinkable_or_wrong_length_bitmap`

## Reference addresses used

All verified against `wowdecomp` pin `e67b6ffef23655dc7da061cb2d36b7963f6ef71a`:
- `0x00A1B640` (`TradeSkillFrame.cpp:32`): `s_base64Encode` 64-character alphabet (`A-Za-z0-9+/`, no `=` padding).
- `0x00A1B688` (`TradeSkillFrame.cpp:35`): `s_base64Decode` 128-entry ASCII decode table (unmapped = 0).
- `0x00812030` (`SpellTable.cpp:156`): `SpellTableBuildSkillLineLinkInfo` — builds contiguous raw-DBC runs per `SkillLine` with `m_canLink != 0`.
- `0x008104A0` (`SpellTable.cpp:79`): `SpellTableGetSkillLineLinkInfo` — returns the first matching contiguous run for a skill line.
- `0x00810320` (`SpellTable.cpp:51`): `SpellTableMatchRaceClass` — race/class mask filter (called with zero exclude masks from `0x005DB060`).
- `0x005DB060` (`TradeSkillFrame.cpp:943`): `CGTradeSkillInfo::GetTradeSkillListLink` — link formatting (`|cffffd000|Htrade:%d:%d:%d:%I64X:%s|h[%s]|h|r`), `m_canLink` check, LSB-first 6-bit bitmap encoding.
- `0x005DC0B0` (`TradeSkillFrame_Scripts.cpp:754`): `Script_GetTradeSkillListLink`.
- `0x005DA310` (`TradeSkillFrame_Scripts.cpp:143`): `Script_IsTradeSkillLinked` — returns `(1, name)` when `m_linkData` is set, else `(nil, nil)`.
- `0x005DA8B0` (`TradeSkillFrame_Scripts.cpp:66`): `Script_CloseTradeSkill`.
- `0x005DAC00` (`TradeSkillFrame_Scripts.cpp:266`): `Script_DoTradeSkill`.
- `0x0062DAE0` (`Tooltip_Scripts.cpp:524`, line 634): `Script_SetHyperlink` — routes `"trade:"` directly to `CGTradeSkillInfo::SetLink(cursor + 6)` without showing a tooltip.
- `0x005DE300` (`TradeSkillFrame.cpp:1395`): `CGTradeSkillInfo::SetLink` — parses `spellId:rank:maxRank:guidHex:bitmap`, queries `g_nameDBCache` (`CMSG_NAME_QUERY`), and invokes `TradeSkillLinkNameCallback`.
- `0x005D9F50` (`TradeSkillFrame.cpp:715`): `TradeSkillLinkNameCallback` — receives name-query callback and opens via `SetTradeSkill`.
- `0x005DE120` (`TradeSkillFrame.cpp:1314`): `CGTradeSkillInfo::SetTradeSkill` — validates `m_canLink`, bitmap length `count / 6 + (count % 6 != 0)`, disarms repeat, toggles only when `!linked`, stores `m_linkData`.
- `0x005DD5A0` (`TradeSkillFrame.cpp:339`): `CGTradeSkillInfo::RefreshList` — decodes bitmap bits against `SkillLineAbility` rows, filters by `SPELL_ATTR_IS_TRADESKILL` (`0x20`), bands against live skill if self-link or link rank otherwise, uses row trivial ranks.
- `0x0051A250` (`Player_C.cpp:1176`) & `0x00594710` (`Player_C.cpp:1234`): `CGPlayer_C::GetSkillValue` and `GetSkillMaxRank` (permanent bonus added when nonzero).

## Unverified facts

- **Direct Lua `DoTradeSkill` refusal on a linked list**: in 12340 `Script_DoTradeSkill` (`0x005DAC00`) does not itself check `m_linkData`; stock `Blizzard_TradeSkillUI` disables the Create buttons when `IsTradeSkillLinked()` is true, and `Spell_C::CastSpell` rejects unknown spells. Our `DoTradeSkill` binding explicitly blocks crafting whenever `trade_skill_link.is_some() || trade_skill_held_is_link` so direct Lua/macro calls (including viewing your own link) cannot craft from a linked window.
- **`GetTradeSkillListLink()` while viewing someone else's link**: in 12340 `0x005DB060` encodes against the active player's own skill slot for `CGTradeSkillInfo::m_spellID`; here `GetTradeSkillListLink()` returns the stored linked chat string while `trade_skill_link` is set (matching `origin/main`'s `linked_trade_skill_natives_use_host_metadata_and_nil_without_it` test and avoiding returning nil when re-linking another player's open profession).
- **`NameCache` missing traits fallback**: when `NameCache` holds a player's name without `(race, class)` traits (populated outside `SMSG_NAME_QUERY_RESPONSE`), `feed_trade_skill` falls back to `SkillLineCatalog::first_line(spell_id)` instead of `spell_to_line(spell_id, race, class)`.
- **`GetSpellLink` second return (`SpellBookFrame_Scripts.cpp:1185`)**: returning a trade link as the second return of `GetSpellLink` from the spellbook was not part of this task and remains unchanged.
- **Real 3.3.5a `SkillLine.dbc` / `SkillLineAbility.dbc` archives**: no real MPQ/DBC files were loaded or inspected in tests; DBC column indices (`WRATH_COL_SL_CAN_LINK = 55`) and contiguous-run grouping were verified from `SkillLineRec.h` / `SpellTable.cpp` and synthetic test rows only.

## CI run id and result

Not run yet: `git push -u origin gemini/trade-skill-links` failed from this sandbox because no GitHub credentials/token are configured (`fatal: could not read Username for 'https://github.com': terminal prompts disabled`). Once `gemini/trade-skill-links` is pushed to `origin`, GitHub Actions CI will run the full workspace build and test suite including the 5 `benilla-app` tests and lane guard test.

### Local verification run (on `origin/main` `7ac7c8a`, `sccache` + `CARGO_INCREMENTAL=0`, zero file overlap through `fdcbd82`)
- `rustfmt --check --edition 2021` on all 13 changed Rust files: **passed**.
- `cargo check -p benilla-ui -p benilla-app`: **passed** (0 errors).
- `cargo clippy -p benilla-formats -p benilla-ui -p benilla-app --lib --tests --message-format=short`: **passed** (exit 0; 0 errors and 0 warnings across all 13 changed files).
- `cargo test -p benilla-formats --lib trade_skill_link_`: **passed** (`1 passed; 0 failed`).
- `cargo test -p benilla-ui --lib -- trade_skill_link_ linked_trade_skill_natives_use_host_metadata_and_nil_without_it`: **passed** (`8 passed; 0 failed` — all 7 new tests plus the existing linked trade-skill metadata test).

### Local verification NOT run
- Executing the 5 new `benilla-app` `HostedVm` tests (`ui_tradeskill::tests::trade_skill_link_*`) and lane guard test (`every_lane_reads_of_what_a_handler_writes_is_listed`): `benilla-app --lib --tests` type-checked and passed `cargo clippy --lib --tests` cleanly, but `rustc` codegen of the `benilla-app` test binary exceeded the 12 GB RAM-backed container limit (0 swap).
- Re-running Cargo after syncing `7ac7c8a..fdcbd82` (confirmed zero overlap with any of the 14 changed files and no modified signatures used by `ui_tradeskill.rs`).
- No client or AzerothCore server was launched, and no real MPQ/DBC game data was read.

## What is not proven in game

- No live client or AzerothCore server was launched and no account logged in.
- Stock Wrath `Blizzard_TradeSkillUI` link button click → chat editbox insertion → `ChatFrame_OnHyperlinkShow` → `SetItemRef` → `GameTooltip:SetHyperlink` has not been exercised in a running client with real 3.3.5a FrameXML/DBC archives.
- Real-data `SkillLine.dbc` column 55 (`m_canLink`) and `SkillLineAbility.dbc` raw record order have not been verified against live 3.3.5a MPQ archives or cross-checked against a stock 12340 client decoding links emitted by RustWrath (or vice versa).
- Live `CMSG_NAME_QUERY` / `SMSG_NAME_QUERY_RESPONSE` round trip against AzerothCore for an uncached remote character GUID is proven only by synthetic channel/cache injection in unit tests.
