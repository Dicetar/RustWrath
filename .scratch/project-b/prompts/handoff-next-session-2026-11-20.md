# RustWrath handoff — 2026-11-20 evening

The next session is the owner's main coordinator session for RustWrath (Dicetar/RustWrath, a
**public** repo). Its jobs:
- land owner hotpatch requests;
- review the night agents' branches (Astra, Gemini, ClaudeJr, DeepSeek swarm);
- keep the server rebuild batch ready.

Read these first:
- `D:\Projects\wow\worktrees\live-dx6\AGENTS.md` (and CONTEXT.md, docs/agents/sync.md and
  docs/agents/workflow.md, which it links);
- the memory index, which is loaded automatically.

## 1. Where things stand

### Main
- My last commits:
  - `eace0985` "fix: mail auto-loot, enchant level gate, WMO interior light, souls mount/slide";
  - `d9d35507` "style: rustfmt the enchant-level tooltip test".
- Main has since moved about 62 commits, mostly DeepSeek's `oc/clippy-*` branches and
  `oc/spell-energize`. Run `git fetch` and `git log origin/main` to see them.
- CI on `d9d35507` (run 44770772111): fmt, clippy, test-build, test and windows all **pass**;
  only `player-clippy` fails.
- The latest failed main run is 44797018702, at 18:53. Its `player-clippy` has 5 errors, and
  `benilla-ui` does not compile under it:
  - 4 in `crates/benilla-ui/src/script/wrath_natives.rs`, which is Gemini's;
  - 1 in `crates/benilla-ui/src/script/unit/bindings.rs`.
  - More findings in `benilla-app` may appear once `benilla-ui` compiles.
  - The full log is saved at `F:\AI\tmp\fxv\ci-44797018702.log`.
- Run 44801353256 was still running at handoff.

### Client
- `D:\Projects\wow\client` was fast-forwarded to `d9d35507`.
- **The owner must restart it.** The EnchantView, MailOpen and MailCall layouts changed, and live
  code cannot patch layout. The owner said "I'll restart later".
- Fast-forward again to the newest green main before they restart.
- Launch only if asked; the command is in the constraints section.

### What eace0985 shipped (all hotpatched; confirm in game after the restart)
1. Souls: a jump attack no longer slides the body back after landing (airborne frames zero the
   root-motion debt). Mounted characters cannot swing, roll, guard, sprint, crouch or jump-attack.
2. Mail auto-loot takes the money, then each attachment in turn. This covers the owner's
   auction-house delivery with bags.
3. Indoor WMO lighting: `FixColorVertexAlpha` now runs only for unified roots (MOHD 0x2). It
   fixes the overbright inns and black corridors caused by ClaudeJr's doorway change.
4. The +16 shirt enchant tooltip: the client-side tooltip is fixed. The real stat bug, random
   enchants rolling any tier, is server batch item 5.
5. Warrior rage from Souls hits is live through ALE Lua in
   `integrations/ale/rustwrath_souls.lua`, deployed to the server's `lua_scripts`. The full
   auto-attack path is batch item 8.

### Known pre-existing failure
`abbey_matches_the_reference_capture` (WMO) fails on the 3.3.5a install: 519 vs 678 vertices.
The reference capture came from a 1.12 client. The test needs a Wrath recapture; the code is not
wrong. This is noted in the eace0985 message.

## 2. Server rebuild batch (owner rebuilds by hand; never for a single change)
The batch file is `.scratch/project-b/server-rebuild-batch.md`, with these items pending:
- **5.** Random enchants roll any tier, level-60 enchants included. A bug fix; needs a rebuild.
- **6.** Crafting while moving. Needs a rebuild and a client change.
- **7.** Disenchanting a random-enchanted item yields a "bottled enchant" bonus. ALE, no
  rebuild. **This needs owner decisions** (the open questions are at the item's "Open questions
  for the owner" line): whether the bottle binds (BoP or BoE), whether it can override an
  existing enchant, and so on. Ask the owner before building it.
- **8.** Souls hits count as the real auto-attack (procs, the Heroic Strike queue). Needs a
  rebuild. It replaces the ALE rage stopgap.

Leave the server source's uncommitted edits as they are.

## 3. Night agents: prompts and what each is doing
The prompts are in `D:\Projects\wow\worktrees\live-dx6\.scratch\project-b\prompts\`, which is
**untracked and must stay untracked**. The owner pastes them into each agent.

| Agent | Prompt file | Work | Rules |
|---|---|---|---|
| DeepSeek (OpenCode AutoSwarm) | `deepseek-autoswarm-2026-11-23.md` | The remaining SPELL_* combat-log families, in order (MISSED, HEAL, ENERGIZE, PERIODIC_*, AURA_*, CAST_*). Then player-clippy green, one file per ticket. Then the gap 26/27 tickets. Report in `work-packages/2026-11-23-opencode-swarm.md`. | Lands on main itself, after green CI. Already landed many `oc/clippy-*` branches and `oc/spell-energize`. |
| ClaudeJr | `claudejr-2026-11-21.md` | 0: close the stale jr notes (branch `origin/jr/close-stale-notes` exists). 1: the combat-log history ring. 2: unified WMO mode 0 and mode 2. 3: UNIT_SPELLCAST_(NOT_)INTERRUPTIBLE. 4: spirit-healer POI follow-through. | No local cargo. Lands after green CI. |
| Gemini | `gemini-trade-links-and-focus-2026-11-21.md` | `gemini/trade-skill-links`, then `gemini/focus-target`. Also owns the wrath_natives.rs clippy findings. | **Never pushes main.** You review and land its branches. No `gemini/*` branch for these existed at handoff. |
| Astra | `astra-stance-fix-and-outliers-2026-11-21.md` (the earlier `astra-sex-stance-all-rigs-2026-11-21.md` is done) | Fix the stance double-count, explain the shin outliers, add the stance to remote Souls players, merge all-rigs into sex-stance. | Owns `souls/retarget.rs` and the new `souls/config.rs` switches. Does not merge to main. |

### Astra review findings (why the new task exists)
- `astra/sex-stance` (d3f99bde), **not landable as is.** `apply_stance` adds the whole stock
  mean posture on top of the retargeted ER pose. That counts the posture twice, and it is on by
  default. The fix is `offset = stock − retargeted-ER mean`, with clamps of about 12° for the
  pelvis and 15° for the limbs.
- `astra/all-rigs` (b0d82fd5): a good 40-rig fixture, but shin errors of 126–143° on Gnome and
  Tauren male, and 71–95° on Tauren female and Draenei female, are unexplained.
- Both branches fail only `player-clippy` (old debt; neither diff adds a finding).
- Notes: `.scratch/project-b/handoffs/2026-11-21-astra-sex-stance.md` and
  `2026-11-21-astra-all-rigs.md`, on their branches.
- When Astra reports, review the stock/er/offset table and the pass limits before offering the
  owner a landing.

### Ownership tonight (keep it when you hotpatch)
- **Claude (you):** `souls/drive.rs`, `input.rs` and `server.rs`, `combat_kit*`,
  `combat_playback*`, `integrations/ale/**`.
- **Astra:** `souls/retarget.rs`.
- **Gemini:** tradeskill, chat hyperlinks, `script/target.rs` and `wrath_natives.rs`.
- **ClaudeJr:** `wmo/`, `*.wgsl`, `model_render*`, `minimap/`, `script/combat_log.rs` and
  `ui_script/wrath_cast.rs`.
- **DeepSeek:** `wrath_combat_log_feed.rs`, the protocol spell-log decode, and its clippy files.

## 4. Code knowledge graph (graphify)
- **Tool:** graphifyy 0.9.113, in the venv `F:\AI\tools\graphify-venv`. The CLI is
  `F:/AI/tools/graphify-venv/Scripts/graphify.exe`; there is also `graphify-mcp.exe`.
- **Graph:** `F:\AI\tools\graphify-work\rustwrath\graphify-out\graph.json`.
  - 68,072 nodes, 181,970 edges and 1,273 communities, plus Cargo crate dependencies.
  - The report is `GRAPH_REPORT.md` beside it. Communities are unlabelled; labelling is skipped
    because it needs an LLM.
- **Source:** a code-only AST index (no LLM, no API key) of a `git archive` snapshot of
  `d9d35507`. It covers `crates`, `integrations`, `scripts`, `tools`, `docs`, `src`, the root
  Cargo.toml, AGENTS.md, CONTEXT.md and README.md.
  - It excludes `third_party`, `target`, `public`, `reference`, configs and logs.
  - It is entirely outside the repo, so nothing needs a git exclude.
- **Refresh** to the current main: `bash F:/AI/tools/graphify-work/refresh-rustwrath.sh [rev]`
  (default `origin/main`).
- **Query.** Run from `F:\AI\tools\graphify-work\rustwrath`, or pass
  `--graph <path to graph.json>`:
  - `graphify.exe query "how does mail auto-loot take attachments" --budget 2000`
  - `graphify.exe explain "UiScript"`
  - `graphify.exe path "SessionEvent" "UiScript"`
  - `graphify.exe affected "WrathActiveCast"`
  - `graphify.exe god-nodes --top 20`
  - Set `PYTHONIOENCODING=utf-8` in Git Bash.
- **Hubs:** UiScript (1887 edges), ObjectStore, SessionEvent, Model, NetCommands, ClientCommand,
  FrameHandle, ServerPacket.
- **Caveat:** graph paths are relative to the snapshot (`crates/...`), and they match the
  worktree layout. Treat the graph as a guide for locating code; read the real file before you
  trust it.

## 5. Standing constraints (from the owner and the memory; all still in force)

### Owner directives
- "Just hotpatch everything I request": land on main, then fast-forward the client.
- "DON'T STOP UNTIL IT'S 100% DONE". Fix root causes, not stopgaps.
- Cargo, builds and launches were authorized in the last session. Re-check if unsure: the
  default rule in AGENTS.md is no Cargo or launch unless the owner asks in that session.

### Git and landing
- Work in `D:\Projects\wow\worktrees\live-dx6`. Use `git -c safe.directory=*`.
- Stage explicit paths only. No force-push, rebase, bare stash or `add -A`.
- Leave these as they are:
  - `Cargo.lock` stays unstaged (it is dirty);
  - `.scratch/project-b/prompts/` and `crates/benilla-formats/examples/m2_probe.rs` stay
    untracked.
- Commit with `git -c safe.directory=* -c user.name=Dicetar commit -F <file in F:\AI\tmp\>`.
  - The message says what was not run.
  - It ends with `Co-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>`.
- Land: `git fetch`, merge `origin/main`, then `git push origin HEAD:main` **only** (one ref, so
  CI runs once).
- Client update: in `D:\Projects\wow\client`, run `git merge --ff-only origin/main`.

### Builds and tests
- Run `rustfmt --edition 2021` on **every** touched .rs file before committing. The CI fmt gate
  failed once this session because of this.
- Use sccache: `export PATH="/c/Users/Qraag/.cargo/bin:$PATH" CARGO_INCREMENTAL=0 RUSTC_WRAPPER=sccache`
  with `--target-dir target`.
- Run long cargo in the background, logging to `F:/AI/tmp/fxv/tN.log`; the next N is **t33**.
- Run at most 20 test cases per task.
- Bash `sleep` is a no-op. Use `timeout 5 tail -f /dev/null`.

### CI logs
- `gh run view <id> --repo Dicetar/RustWrath --log-failed | sed 's/\^\[\[[0-9;]*m//g'`.
- In Git Bash, `git show rev:path` needs `MSYS_NO_PATHCONV=1`.

### Server, data and the live session
- The server, DB and game install are read-only. The one exception is deploying the repo's own
  ALE Lua to the server's `run\lua_scripts`.
- Do not restart or reconfigure the server. Server C++ changes go only into the batch file.
- The owner logs in and drives combat. Never kill the client or another agent's build.
- Do not browse the owner's Screenshots folder.

### Secrets and privacy
- Some local files hold secrets. **Never print, quote or commit them**, and filter
  `@`/`TOKEN`/`assword` lines when reading them:
  - the launcher `dx.log` files;
  - the MySQL `.err` log;
  - `worldserver.conf`;
  - the `WM_SOAP_*` environment variables.
- Never put the owner's identity in external requests.

### Downloads
Untrusted downloads go in their own directory. Run Python that reads them with `-I`.

## 6. Open threads, in priority order
1. **Restart check.** When the owner restarts the client, confirm the mail auto-loot, indoor
   lighting, enchant tooltip, mounted lockout and jump-attack slide in game. Ask for their
   observations; do not log in.
2. **Night agent results.** Review each agent's branch and note when it reports:
   - land Gemini's branches yourself after review and green CI;
   - check ClaudeJr's and DeepSeek's landings against their tickets;
   - hold Astra's branch for owner judgement.
3. **player-clippy.** Once only Gemini's `wrath_natives.rs` findings are left, make sure Gemini's
   branch fixes them. Then record the first green run id.
4. **Item 7 decisions.** Get the owner's bottled-enchant answers, then finish the batch for the
   owner's rebuild night (items 5, 6, 8 and 7 if it is chosen).
5. **Abbey test.** Recapture the abbey WMO reference from the Wrath install. It is a small
   ticket; it could go to ClaudeJr or DeepSeek.

## Suggested skills
- `diagnosing-bugs`: for any owner bug report (the hotpatch loop).
- `code-review`: when reviewing agent branches before landing.
- `zaebal`: the owner's escalation and persistence mode, when they are frustrated with a
  repeated bug.
- `research`: for checks against the 12340 reference
  (`D:/Projects/wow/reference/wowdecomp-reference`, pin e67b6ffe) and AzerothCore behaviour.
- `resolving-merge-conflicts`: when merging agent branches with `origin/main`.
- `tdd`: for new focused tests within the 20-case budget.
- The `project-b-12340-reference` skill is not installed in `~/.claude/skills`. Follow
  `docs/agents/12340-reference.md` instead.
