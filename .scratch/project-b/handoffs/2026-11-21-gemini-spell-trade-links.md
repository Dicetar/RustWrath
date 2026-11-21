# Profession links from the spellbook: GetSpellLink's trade link

Status: ready to merge
Owner: Claude Code / Claude Opus 5.5 (run from the brief written for Gemini)
Branch: gemini/spell-trade-links
Issue: none
Next: integrator reviews and lands; owner shift-clicks a profession in the spellbook in game

## Status

Source finished. `benilla-ui` focused tests pass locally; `benilla-app` compiles (`cargo check`)
but its tests were not run locally. Not seen in game.

## Owner: Gemini

The brief named Gemini as owner. Claude Code (Claude Opus 5.5) did this run instead, from the same
brief, so commits carry Claude's trailer rather than `models/gemini`.

## Branch and bundle file

- Branch `gemini/spell-trade-links`, from `origin/main` b520b19aa.
- Bundle `gemini-spell-trade-links.bundle` in the worktree root, uncommitted. It is only a
  fallback; the branch was pushed if the push line in the delivery report says so.

## What changed

- `benilla-ui` `script/wrath_spell.rs`: `GetSpellLink` now returns two values.
  - First: the spell link, as before.
  - Second: the trade link, or nil. It comes from `encode_trade_skill_link` on main
    (`script/tradeskill/link.rs`), applied to the host's link source for that spell ID. No
    second encoder.
  - An unresolved spell still returns a single nil, as before.
- `benilla-ui` `script/host.rs`: new `CallHost::trade_skill_link_source(spell_id)`. It returns
  `None` by default.
- `benilla-app` `ui_tradeskill.rs`:
  - New `own_link_source(spell_id, …)`, one builder that follows `GetTradeSkillListLink(spellId)`:
    - checks that `Effect[0]` is `SPELL_EFFECT_TRADE_SKILL`;
    - finds the line through `spell_to_line(spell, race, class)`;
    - requires `m_canLink`;
    - takes the link rows, then the slot value and max (with the permanent bonus) and the known
      spells.
  - The open window's `GetTradeSkillListLink()` source now comes from this builder too. So the
    spellbook shift-click and the window's link button encode the same inputs the same way.
  - Leftover fixed, the traits fallback: `feed_trade_skill` no longer falls back to
    `SkillLineCatalog::first_line` when `NameCache` has the link owner's name without race and
    class. It sends one `CMSG_NAME_QUERY` (`LinkedTradeSkill::traits_asked`) and keeps the link
    pending until an answer with traits lands.
- `benilla-app` `ui_script/wrath_queries.rs`:
  - `SpellQueries::trade_skill_link_source` reads the existing owners: `Spells`, `SkillLines`,
    `PlayerActions`, and the self player's store.
  - The player query now also reads its `Guid` component; no new resource.
- `benilla-app` `script_calls.rs`: `ScriptCallHost` answers the new query through
  `SpellQueries`.
- Shift-click: the stock spellbook posts `GetSpellLink`'s second value when there is one (see
  Unverified facts). No FrameXML change is needed.

## Reference addresses used

Pin e67b6ffe.

- `Script_GetSpellLink` `0x5408E0` (`WoW/Source/Ui/SpellBookFrame_Scripts.cpp:1158`), confirmed:
  - It pushes `CGTooltip::GetSpellLink(spellId)`.
  - At line 1185 it calls `CGTradeSkillInfo::GetTradeSkillListLink(spellId, link, 256)`, pushing
    the link, or nil when that returns false. It always returns 2 values.
  - A missing record, or one with attributes `0x80` and `0x100` both set, returns nil, nil.
- `CGTradeSkillInfo::GetTradeSkillListLink` `0x5DB060` (`TradeSkillFrame.cpp:943`):
  - It takes the spell ID; no window is involved.
  - It needs an active player and `Effect[0]` `SPELL_EFFECT_TRADE_SKILL`.
  - It reaches the line through `GetSkillLineRec` and requires `m_canLink` and
    `SpellTableGetSkillLineLinkInfo` (`0x8104A0`).
  - It encodes value, max, GUID and the race/class-filtered bitmap, using the encoder already on
    main.
- `CGTradeSkillInfo::GetSkillLineRec` `0x5DA5F0` (`TradeSkillFrame.cpp:739`): the
  `g_nameDBCache` record's race and class, then `SpellTableLookupAbility`. With no record there
  is no line.
- `CGTradeSkillInfo::SetLink` `0x5DE300` and `TradeSkillLinkNameCallback` `0x5D9F50`: the link
  waits for the name record.

## Unverified facts

- Shift-click: the stock 3.3.5 `SpellBookFrame.lua` `SpellButton_OnModifiedClick` inserts
  `GetSpellLink`'s second value, when present, instead of the spell link. That is from general
  knowledge; no local FrameXML copy was read. If stock code does not, the native is still right.
- The hidden-spell rule (both `0x80` and `0x100` set gives nil, nil) is not modelled.
  `SpellInfoView` carries no attributes, and this was not changed here.
- Our own race and class: `GetSkillLineRec` reads them from our own name-cache record. The
  builder reads our descriptor's `UNIT_FIELD_BYTES_0` instead. They should agree; not checked
  against a live session.
- Re-asking the name query for a name cached without traits is our behaviour, not the
  reference's. The reference has no such state: its name record always carries race and class.

## Local checks run, and what was not run

Toolchain 1.98.1, sccache wrapper, `CARGO_TARGET_DIR=D:\Projects\project-b-client\target`.

- Run:
  - `cargo check -p benilla-ui -p benilla-app`: passed.
  - `cargo test -p benilla-ui --lib -- spell_trade_link_ trade_skill_link_ wrath_spell::`: 18
    passed. That is the 2 new tests, plus the 7 existing trade-link tests and 9 `wrath_spell`
    tests, run because this branch changes code they cover.
  - `cargo fmt`: clean.
  - `cargo clippy -p benilla-ui -p benilla-app --no-default-features -- -D warnings`: passed.
- Not run:
  - The `benilla-app` tests, locally, including the new
    `ui_tradeskill::tests::trade_skill_link_without_owner_traits_asks_once_then_opens` and the
    existing `ui_tradeskill` link tests that now run through `own_link_source`.
  - Clippy on test targets.
- CI 44930247435 on 05f155c63: compiles; tests pass, including the `benilla-app` tests not run
  locally.

## What is not proven in game

- A spellbook shift-click on Tailoring (or another linkable profession) posting the `|Htrade:`
  link, and that link opening the read-only window for another player.
- The name-query re-ask against AzerothCore.
