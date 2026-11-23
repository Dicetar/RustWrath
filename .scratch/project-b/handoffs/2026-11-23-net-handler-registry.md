# net names no feature: the handler registry inverted

Status: in progress; bounded feature-handler extraction implemented, local verification complete; final CI pending
Owner: Codex / GPT-5
Branch: refactor/net-handler-registry
Issue: none
Next: commit the bounded extraction, push once, and read its single CI run

Step 3 of the build-speed plan (`2026-11-22-split-benilla-app.md`): make the core's `net` module
stop naming feature modules, so features can leave `crates/benilla-app` for crates that compile in
parallel. No gameplay change: packets are handled in the same order, with the same results.

## Design

### What already exists

`net::handlers` is the reference's opcode table (`NetClient +0x74`, dispatcher `0x537aa0`): each
subsystem registers one-shot systems per `SessionEventKind` with `app.net_handler(kind, f)`, and
the exclusive drain (`net::apply::apply_net_updates` → `handlers::dispatch`) runs every event's
handlers in packet order, each handler's commands applied before the next. Order within a kind is
registration order (plugin order in `GamePlugins`), plus `net_handler_ahead`, which puts a handler
first. Most features already register their own packets this way; what still ties `net` to the
features is *halves*: a `net` handler that does the session's part and then, inline, a feature's
part (a chat line, a pane's callback, a tutorial), and the object manager's notify walk, which
calls feature hooks per block.

### The change: slots the core reserves

A feature half cannot simply register its own handler: plugin order would decide where it runs,
and that is not where the half ran inside `net`'s handler. So the core reserves the place.

- `HandlerSlot` (in `net::handlers`) is a core enum of named places, each bound to one
  `SessionEventKind`. The core reserves a slot where the half used to run, around its own handler:
  `app.net_slot(K::ItemTemplate, HandlerSlot::ItemAnsweredLines)`, registered right after
  `net_handler(K::ItemTemplate, on_item)`.
- A feature fills it from its own plugin, whatever the plugin order:
  `app.net_handler_in(K::ItemTemplate, HandlerSlot::ItemAnsweredLines, on_answer)`. The kind is
  repeated so the census test can read the registration; a slot filled under another kind panics
  at build.
- The drain skips an empty slot; several fills run in registration order. The census
  (`NetHandlers::census`) lists filled handlers only, so it reads as before the move.
- The core names the slot, never the feature: the slot's doc says which reference callback stands
  there; which module fills it is the feature's business.
- What a half needs from the core's half (a delta's latches, the callbacks an answer took) the
  core publishes in a small core resource written just before the slot runs.

Why not messages per event family: a message is read by a system in a later stage, which moves the
feature's work after every other packet of the drain. The lanes (`lanes.rs`) restore UI event
order by stamp, but a half's own stamps (a chat line, an auction tail) would then be taken later
than the core's, and that changes the order the interface hears them. A slot runs inside the
drain, at the same point, and stamps where it always did.

### Feature state the core reads: move the type down

Where the core only *reads or writes a small type* a feature declares (a marker component, a
message, a catalog resource), the type moves down into `net` and the feature re-exports it at its
old path, as `souls` did. The feature's systems stay with the feature.

### Not inverted in this task: the object manager's notify walk

`net/objects/update.rs` runs the reference's watchers per block in one handler
(`on_update_end`: field watch, item watchers, guild timestamp, coinage → trade trim, pet deselect,
party records). Their order is interleaved per block and stamped, so a slot per packet would move
them. Inverting it needs an exclusive walk (`SystemState<Scene>` re-fetched per block) that calls
registered watch hooks (`SystemId<In<WatchCall>>`) where the inline calls are now; see "What is
left" below.

## Map: what `net` named on main 5d874e82f

`modgraph.py crates/benilla-app/src out.json --code` (non-test code): `net` → 41 modules, 241
references; 110 modules name `net`.

Where the edges sit, and why (non-test code; counts are `crate::x` references):

| File | Names | Why |
|---|---|---|
| `net.rs` | quest_trace 17, creature_anim 5, wrath_item 4, items 4, query_cache 3, names 3, lanes 3, player 2, go_templates 2, … | `ClientCommand`'s `trace: Option<quest_trace::Interaction>` on every interaction verb; `NetPlugin::build` inits the caches and lanes the handlers write (names, items, go templates, world states, item lane, bag cache, `ui_items` bag view, field-watch notice lane) and seats `player::regain_control_on_arrival` in the Net chain; `Objects` reads item countdowns and Wrath item rows; `creature_anim::move_flags` |
| `net/session.rs` | quest_trace 13, field_watch 9, transport 7, wrath 6, ui_reputation 5, spell 5, ui_unit 4, ui_chat 3, player 3, ui_party 2, tutorial 2, target 2, ui_quest_log, ui_quest, combat_log, names, items | the session edges' handlers: login milestones traced; the worldport's roster hook (`ui_party`) and view log (`ui_unit`); the session end's cooldown list (`spell`); the reputation messages' chat lines (`ui_reputation`, `combat_log`, `ui_chat`), quest re-ask and quest log rebuild, reputation tutorial, the faction catalog (`target::ring::Factions`) |
| `net/objects.rs` | items 18, field_watch 17, transport 16, creature_anim 14, item_watch 12, item_lane 7, ui_loot 5, go_anim 4, entities 4, ui_party 3, ui_chat 3, ui_auction 3, … | the object manager's `Scene` param: the item answer's callbacks (loot rows and pushes, the combat log's held lines, the auction lists), the XP mark (`ui_chat::combat::watch::XpAwarded`) kept across a release, and the state the notify walk reads |
| `net/objects/update.rs` | field_watch 13, items 5, ui_party 3, item_watch 3, death 3, transport 2, target 2, ui_unit, ui_trade, ui_chat, query_cache, lanes, go_anim | the notify walk per block (`on_update_end`): field watch, item watchers, aura watch, coinage → trade trim, guild timestamp, pet deselect (`target::DeselectGuid`), party records and member deactivate, corpse notes (`death`), the view log (`ui_unit::views::ViewLog`) |
| `net/motion/*` | player 36, creature_anim 25, ui_session 10, transport 4, entities, sound | movement relay, splines, remote extrapolation, display facing (reads `ui_session::InteractNpc` and `sound::EmoteSounds`' emote flags) |
| `net/handlers.rs` | creature_anim 1 | `apply_unit_edits` after every handler |
| others | client_state 5 (game time), live_code 3, wdb 1 (io), world_state 1, glue_session 1 (fake io) | |

Per kind, the handler order is the census (`NetHandlers::census`); a full dump of it and of every
schedule's executor order, before and after, is in "Checks run".

## What moved

| Half | From | Now | Slot / how |
|---|---|---|---|
| Standing chat lines (combat log "increased by", then rank lines) | `net/session.rs` `on_reputation_delta` | `ui_reputation::on_standing_lines` | `StandingLines`, ahead of the store write |
| Quest re-ask, quest log rebuild on a moved rank, reputation tutorial | `on_reputation_delta` | `ui_reputation::on_standing_follow_ups` | `StandingFollowUps`, after; reads `net::StandingLatches` |
| Visible-faction chat line | `on_reputations` | `ui_reputation::on_visible_line` | `VisibleLine`, ahead |
| Worldport roster hook + its view-log records | `on_worldport` | `ui_party::net::on_worldport_roster` | `WorldportRoster`, ahead |
| Session-end cooldown clear | `disconnected()` | `spell::net::on_session_end` | `SessionCooldowns`, after |
| Item answer: loot rows and pushes | `net/objects.rs` `on_item` (inline in the callback loop) | `ui_loot::net::on_item_answered` | `ItemLootCallbacks`, after; the core takes each callback's lane stamp at its place in the loop (`net::LootAnswers`) and the loot window queues its step at that stamp (`LootState::queue_at`), so the lanes fire in the same order |
| Item answer: combat log's held lines | `on_item` | `ui_chat::net::on_item_answered` | `ItemHeldLines` |
| Item answer: auction lists | `on_item` | `ui_auction::net::on_item_answered` | `ItemAuctionLists` |
| Unit-edit flush after every handler | `net::handlers::call` named `creature_anim::apply_unit_edits` | `CreatureAnimPlugin` registers it | `net_after_each_handler` |

Types moved down into `net`, re-exported at their old paths (no reader changed):
`Factions` (was `target::ring`), `slot_rank` (was `ui_reputation`), `XpAwarded` (was
`ui_chat::combat::watch`), `DeselectGuid` (was `target::click`), `InteractNpc` (was `ui_session`),
`EmoteSounds` and its `emote_flags` (was `sound::emote`; its other methods stay there).

## What is left, and why

`net` still names (after this task): see "Measure". The pane-side ones are all in the object
manager (`net/objects.rs` `Scene`, `net/objects/update.rs`):

- `ui_party` — the deactivate's member snapshot (`member_deactivated`), the create hook's party
  record notices and pet name snapshot, and the pet watcher's party check, all inside one block's
  notify. `GroupState` is read and written mid-walk.
- `ui_trade` — the coinage watcher's offer trim (`0x5ddf30`), at the block that moved coinage. It
  sends `CMSG_SET_TRADE_GOLD`; moving it to the packet's end would reorder it against the name and
  item queries later blocks send.
- `ui_guild` — the guild timestamp watcher (`stamp_changed`), same shape as the trim.
- `ui_pet` — the pet bar's pet guid, read by the field watch's `Around`.
- `ui_items` — the bag view's per-frame stamp (`BagView::wrote`).
- `death` — the corpse notes on create, values and destroy.
- `ui_unit` — the view log (`ViewLog`), written through every store write; `ui_unit` is a hub the
  call host reads (49 modules name it), so it stays core for now.
- World presentation the object store spawns and moves: `creature_anim`, `player`, `transport`,
  `go_anim`, `entities`. These are the core's own world, not panes.
- Core infrastructure: `quest_trace` (the diagnostics recorder; `ClientCommand` carries its
  `Interaction`), `field_watch`, `item_watch`, `item_lane`, `lanes`, `items`, `names`,
  `go_templates`, `query_cache`, `world_state`, `wrath_item`, `wrath`, `client_state`,
  `run_mode`, `live_code`, `wdb`, `glue_session`.

The next inversion for the object manager: make `on_update_end` an exclusive walk that fetches
`Scene` through a `SystemState` and, at each place a watcher hook runs inline today, runs the
hooks features registered for that watcher (`SystemId<In<WatchCall>>`, the call carrying the guid,
the block's edges and the store), re-fetching `Scene` after a hook ran. The hooks are rare (a
coinage edge on the player, a guild timestamp, a pet change), so the re-fetch costs little. Two
things to settle first: the lanes' guard tests read every handler's declared access
(`lanes/guard_tests.rs`), and an exclusive handler declares all of it; and `GroupState` is touched
at four places in the walk, so the party's hooks are four.

## Current checkpoint (2026-11-24)

The bounded handler-registry extraction is implemented in the worktree. The core reserves slots and
feature plugins fill them independent of plugin build order. The WIP includes the reputation halves,
worldport roster hook, item-answer callbacks (loot, chat and auction), session cooldown cleanup, and
the per-handler animation-edit hook. The item-answer loot callbacks retain the original callback
order by recording each callback's lane stamp in the core and replaying the feature work at that
stamp.

The watcher walk in `net/objects/update.rs` remains deliberately untouched: its per-block interleaving
and access guard need a separate bounded design. No client, game, or server validation was run.

## Checks run

- Registry tests passed 11/11.
- Object-update tests passed 27/27.
- Auction sort tests passed 27/27.
- Loot tests passed 119/122. The three failures hit the existing `UIParent.lua:329` syntax/loader
  failure; item-answer callback tests passed.
- Reputation tests passed 9/10. The unchanged remaining test assumes installed faction 1052 has the
  `HEADER` bit; installed data reports `0x00`.
- Built-client census: 307 event kinds, 380 handlers, zero unreserved fills, and zero probe failures.
- Before/after order dump: all 307 handler-kind sections and all 20 schedule headers match. After
  removing the eight intentionally extracted feature registrations from the current dump, the
  complete dump is identical to the detached-base dump. Artifacts:
  `.scratch/project-b/handler-order-before.txt` in the detached base worktree and
  `.scratch/project-b/handler-order-current.txt` in this worktree.
- `cargo fmt --all` and `git diff --check` passed.
- Nightly frontend measurement used `rustc/cargo 1.102.0-nightly`, the worktree target, and
  `C:\Users\Qraag\.cargo\bin\sccache.exe`: `--jobs-frontend=8` completed in 38.91s after a
  partially compiling launch; a warm `--jobs-frontend=1` run took 231.2s, and the repeated
  `--jobs-frontend=8` run took 1.22s because it reused the single-thread artifacts. These are not
  a cold parallelism comparison; the 1.22s result is incremental no-op latency.

No code commit or CI run has been made for this final WIP yet. The branch is ready for one final
local diff inspection, commit, push, and CI verification. The restored `record_answered` re-export
is test-only to avoid a normal-build unused-import warning.
