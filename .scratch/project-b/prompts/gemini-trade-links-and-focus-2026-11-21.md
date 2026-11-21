You are working on RustWrath (github.com/Dicetar/RustWrath, PUBLIC repo): a Rust/Bevy WoW client
for 3.3.5a build 12340 against AzerothCore. This is one long, uninterrupted session. Do not stop
to report progress. Work until both tasks are finished or truly blocked, then write the task
notes.

The last trade-skill-links run produced no branch and no note, so nothing of it exists. Start
over from current `origin/main`.

## Setup
1. Fetch `origin`. Read AGENTS.md, CONTEXT.md, docs/agents/workflow.md, docs/agents/sync.md and
   docs/agents/12340-reference.md. The Wrath reference is
   D:/Projects/wow/reference/wowdecomp-reference (pin e67b6ffe).
2. One branch per task, each from current `origin/main` (at or after d9d35507):
   `gemini/trade-skill-links`, then `gemini/focus-target`. Commit and push the branch at every
   stopping point. **Push within the first hour**, even if it is only the note with your
   findings.
3. **Never push to `main`.** An earlier task of yours landed straight on main as one commit named
   "finally loaded". It carried a copy of its own patch, pasted build logs, deleted verification
   notes and dropped the executable bit on every script. This time:
   - Push only your `gemini/*` branches.
   - Stage files by name; never `git add -A` or `git add .`.
   - Before every commit, run `git diff --cached --stat` and confirm it lists only source, tests
     and your note. No `.patch` files, no `public/assets/`, no pastes or logs, no deletions you
     did not intend, and no mode changes (`git config core.fileMode false` if your checkout
     flips them).
   - Write commit messages that describe the change.
4. Do NOT touch these; other agents own them tonight:
   - `crates/benilla-app/src/souls/**`, `combat_*`, `creature_anim*`, `benilla-combat-core/`,
     `integrations/ale/**`;
   - `wmo/`, `*.wgsl`, `minimap/`, `script/combat_log.rs`, `ui_script/wrath_cast.rs`,
     `ui_script/wrath_combat_log_feed.rs`.
   `script/wrath_natives.rs` is yours tonight, including its clippy findings.

## Task 1: trade-skill links, end to end
What is on main now (`crates/benilla-ui/src/script/tradeskill/`):
- ClaudeJr landed the natives' host side. `GetTradeSkillListLink()` and `IsTradeSkillLinked()`
  read `Model.trade_skill_link`, set by `UiScript::set_trade_skill_link(Option<(owner, link)>)`
  (`tradeskill/mod.rs:182`).
- Nothing in the app calls it yet, so the link is always nil, and `api.rs:104` records it as an
  unsupported branch.

In the real 3.3.5 client:
- **A. Your own link.** With a profession window open, the stock frame's link button calls
  `GetTradeSkillListLink()` and posts
  `|cffffd000|Htrade:<spellId>:<curRank>:<maxRank>:<guidHex>:<base64 bitmap>|h[Name]|h|r`.
  - The bitmap marks which of that skill line's recipes you know, in the order the client builds
    from `SkillLineAbility.dbc`.
  - Find the order, the bit packing and the 64-character alphabet in the 12340 reference. Do not
    guess any of them.
  - Put the encoder in the UI crate. The app supplies the DBC rows and the player's known
    spells, and calls `set_trade_skill_link` when the window opens or changes. Name the owner of
    each input.
- **B. Clicking someone's link.** Clicking a `|Htrade:` link in chat
  (`SetItemRef`/`ChatFrame_OnHyperlinkShow`) opens a read-only profession window for that
  player.
  - Find what the 12340 client does here: does it decode the bitmap locally or ask the server?
  - If it asks, find the opcode and look at what AzerothCore answers with
    (`SMSG_TRADE_SKILL`-family and spell-list handlers).
  - Implement the path that matches. Set the link with its owner and clear it on close.
  - The window must not let you craft from a linked list.
- **C. Round trip.** Encode your own link, decode it, and get the same known-recipe set back.

## Task 2: the focus target
On main, `ClearFocus` (`wrath_natives.rs:1190`) is a recorded-unsupported no-op. DeepSeek's audit
marks `PLAYER_FOCUS_CHANGED` as `Blocked: no producer`, but focus is client-owned in 12340, so
the producer is the client itself.
- Find in the reference:
  - where the focus GUID lives;
  - what `FocusUnit`, `ClearFocus`, `/focus` and `/clearfocus` do (protected, secure-template
    `type="focus"`);
  - what clears it (the unit despawning or dying, a zone change);
  - the events: `PLAYER_FOCUS_CHANGED`, plus the `UNIT_*` events that fire for the `focus` token.
- Implement them:
  - the focus GUID in the app's target owner, beside the target;
  - the natives and slash commands;
  - `focus` and `focustarget` unit-token resolution;
  - the event edges.
- The stock FocusFrame must show and track the unit. A secure button with `type="focus"` must
  work.
- If a part needs server help (it should not), name it.

## Rules (both tasks)
- Match the reference's behavior and do not copy its code. Cite each address you rely on in a
  comment. If a fact is not in the reference, record it in the note as unverified. Do not ship
  an invented constant.
- No nil or constant stubs presented as done. Remove the `record_unsupported_native_once` calls
  you make obsolete; keep the others.

## Verification (strict)
- At most 20 new test cases per task. Use synthetic DBC rows and never read game data in tests.
- Task 1 tests: bitmap encode, decode, round trip, link format, the linked window refusing to
  craft, and clearing on close.
- Task 2 tests: set, clear and the event edge, focus clearing on despawn, and `focustarget`
  resolution.
- Run only:
  - `cargo check -p benilla-ui -p benilla-app`;
  - `cargo fmt` (CI fails on any fmt diff; run it before every push);
  - your tests by name filter;
  - `cargo clippy` on your changed files.
  Use sccache (see AGENTS.md). Do not launch a client or server, and do not log in.
- CI logs are readable with `gh run view <id> --repo Dicetar/RustWrath --log-failed`. Strip
  the literal colour codes with `sed 's/\^\[\[[0-9;]*m//g'`.

## Finish
- If `origin/main` moved, merge it into your branch (merge, not rebase), recheck, and push.
- Write one note per task, `.scratch/project-b/handoffs/2026-11-21-gemini-<slug>.md`, in the
  existing handoff format, with these headings:
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
