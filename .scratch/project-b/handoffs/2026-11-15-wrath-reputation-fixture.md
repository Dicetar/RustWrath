# Handoff: Wrath 128-slot reputation store and Faction.dbc fixture alignment

Status: ready to merge
Branch: `fix/wrath-reputation-fixture`
Base: `origin/main` (`9dc36e48`)

## Problem

1. Wrath 3.3.5a (`12340`) expands `Faction.dbc` reputation slots from 1.12's 64 (`FACTION_LIST_LEN`) to 128.
   `new_human_reputations` in `crates/benilla-app/src/target/relations/fixture.rs` allocated a fixed 64-element vector and indexed `slots[i]` by `info.rep_index`, panicking out-of-bounds when run against Wrath `Faction.dbc`.
2. `Reputations` (`crates/benilla-app/src/net/reputations.rs`), `reputation_slot` (`crates/benilla-app/src/net/session.rs`), `faction_standing`/`faction_standings` (`crates/benilla-app/src/ui_reputation.rs`), and `met`/`slot_of` (`crates/benilla-ui/src/script/reputation.rs`) dropped or ignored reputation indices `>= 64`.
3. Several `real_*` and shipped-DBC tests in `benilla-formats/src/factions.rs`, `benilla-app/src/ui_reputation.rs`, and `benilla-app/src/target/relations/tests.rs` hardcoded 1.12 slot indices, 54-row counts, exact 5-header sets, or slot `63` unused assumptions that do not hold on Wrath `Faction.dbc`.

## Changes

- `crates/benilla-protocol/src/messages/reputation.rs` & `mod.rs`: Added and re-exported `WRATH_FACTION_LIST_LEN: usize = 128` while retaining `FACTION_LIST_LEN: usize = 64` for 1.12 wire parsing.
- `crates/benilla-app/src/net/reputations.rs`: Expanded `Slots` and `Reputations::slots()` from 64 to `WRATH_FACTION_LIST_LEN` (128).
- `crates/benilla-app/src/net/session.rs`: Updated `reputation_slot` bounds check to `WRATH_FACTION_LIST_LEN`.
- `crates/benilla-ui/src/script/reputation.rs`: Used `max_slots = if model.is_wrath() { 128 } else { 64 }` in `met()` and `slot_of()`, and added unit test `wrath_lists_and_toggles_factions_in_slots_64_through_127`.
- `crates/benilla-app/src/target/relations/fixture.rs`: Sized `new_human_reputations` vector dynamically up to `WRATH_FACTION_LIST_LEN` (min 64) with bounds-checked writes, and added `new_human_reputations_handles_wrath_slots_past_64`.
- `crates/benilla-app/src/target/relations/tests.rs`: Looked up Cenarion Circle (`609`) and Booty Bay (`template 120`) slots dynamically and sized test vectors accordingly.
- `crates/benilla-app/src/ui_reputation.rs`: Updated `faction_standing`/`faction_standings` to `< WRATH_FACTION_LIST_LEN`, updated `real_*` tests to handle both 1.12 donor and Wrath `Faction.dbc` installs (including dynamic unused-slot lookup instead of slot `63`), and gated the 1.12 vmangos wire-capture test (`a_fresh_alliance_characters_pane_off_live_wire_bytes`) with `donor_data_or_skip!()`.
- `crates/benilla-formats/src/factions.rs`: Updated `real_reputation_factions_group_under_their_team` to assert `> 64` rows and classic parent subset on Wrath installs while preserving exact 1.12 checks on donor installs.

## Verification

Per `AGENTS.md`, `cargo` compilation, tests, and client launch were not run in this environment.
