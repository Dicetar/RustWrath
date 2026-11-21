You are working on RustWrath (github.com/Dicetar/RustWrath, PUBLIC repo): a Rust/Bevy WoW client
for 3.3.5a build 12340 against AzerothCore. This is one long, uninterrupted session. Do not stop
to report progress. Work until the task is finished or truly blocked, then write the task note.

## Setup
1. Fetch `origin`. Read AGENTS.md, CONTEXT.md, docs/agents/workflow.md, docs/agents/sync.md and
   docs/agents/12340-reference.md.
2. Create branch `gemini/trade-skill-links` from current `origin/main`. Commit and push that
   branch at every stopping point.
3. **Never push to `main`.** Your last task landed straight on main as one commit named "finally
   loaded". It carried a copy of its own patch, pasted build logs, deleted verification notes
   and dropped the executable bit on every script; all of that had to be cleaned up by hand.
   This time:
   - Push only `gemini/trade-skill-links`.
   - Stage files by name; never `git add -A` or `git add .`.
   - Before every commit, run `git diff --cached --stat` and confirm it lists only source, tests
     and your note. No `.patch` files, no `public/assets/`, no pastes or logs, no deletions you
     did not intend, and no mode changes (`git config core.fileMode false` if your checkout
     flips them).
   - Write commit messages that describe the change.
4. Do NOT touch `crates/benilla-app/src/souls/**`, `combat_*`, `creature_anim*`,
   `benilla-combat-core/`, `integrations/ale/**`, `wmo/`, `*.wgsl` or `minimap/`. Other agents
   own them.

## Task: trade-skill links, end to end (harder than the last one)
Currently `GetTradeSkillListLink()` answers nil for your own profession, and
`IsTradeSkillLinked()` is backed by `UiScript::set_trade_skill_link`, which nothing calls
(`crates/benilla-ui/src/script/tradeskill/`). In the real 3.3.5 client:
- **A. Your own link.** With a profession window open, the stock frame's link button calls
  `GetTradeSkillListLink()` and posts `|cffffd000|Htrade:<spellId>:<curRank>:<maxRank>:<guidHex>:<base64 bitmap>|h[Name]|h|r`.
  The bitmap marks which of that skill line's recipes you know, in the order the client builds
  from `SkillLineAbility.dbc`. Find the order, the bit packing and the 64-character alphabet
  in the 12340 reference. Do not guess any of them. Implement the encoder in the UI crate,
  with the app supplying the DBC rows and the player's known spells. Name the owner of each input.
- **B. Clicking someone's link.** Clicking a `|Htrade:` link in chat
  (`SetItemRef`/`ChatFrame_OnHyperlinkShow`) opens a read-only profession window for that
  player. Find what the 12340 client does here: whether it decodes the bitmap locally or asks
  the server. If it asks, find which opcode and look at what AzerothCore answers with
  (`SMSG_TRADE_SKILL`-family and spell-list handlers). Implement the path that matches, then
  call `set_trade_skill_link(Some((owner, link)))` and clear it again on close. The window
  must not let you craft from a linked list.
- **C. Round trip.** Encode your own link, decode it, and get the same known-recipe set back.

Rules:
- Match the reference's behavior and do not copy its code. Cite each address you rely on in a
  comment. If a fact is not in the reference, record it in the note as unverified. Do not
  ship an invented constant.
- No nil or constant stubs presented as done. If part B needs a server behavior AzerothCore
  lacks, implement A and C fully and name B's missing producer in the note and in the code.

## Verification (strict)
- At most 20 new test cases in total: bitmap encode, decode, round trip, link format, the
  linked window refusing to craft, and clearing on close. Use synthetic DBC rows and never
  read game data in tests.
- Run only `cargo check -p benilla-ui -p benilla-app`, `cargo fmt`, your tests by name filter,
  and `cargo clippy` on your changed files. Use sccache (see AGENTS.md). Do not launch a
  client or server, and do not log in.

## Finish
- If `origin/main` moved, merge it into your branch (merge, not rebase), recheck, and push.
- Write the task note `.scratch/project-b/handoffs/2026-11-20-gemini-trade-skill-links.md`
  in the existing handoff format, with these headings:
  - Status
  - Owner: Gemini
  - Branch
  - What changed
  - Reference addresses used
  - Unverified facts
  - CI run id and result
  - What is not proven in game
- Commit messages end with `Co-Authored-By: models/gemini` and say what was not run.
- Never commit game data, extracted FrameXML/GlueXML/DBC, credentials, `benilla-config/` or
  `Cargo.lock` changes you did not need.
