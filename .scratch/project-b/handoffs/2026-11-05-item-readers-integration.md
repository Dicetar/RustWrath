# 2026-11-05 — P0 item payload + read-only inventory readers integration

Batch: two AutoSwarm OpenCode workers plus coordinator review. Owner-selected model after the
free-provider quota block: `opencode-go/deepseek-v4.1-flash`. Both workers were given disjoint
file/function boundaries and exchanged proposals directly before editing.

## Branches and SHAs

| Ref | SHA | Scope |
|---|---|---|
| `origin/main` base | `46fd2f0c2af476b3659116f0fc8c95900bde95a0` | untouched |
| prior batch integration | `39e3c62a43f8967beb74c660fe3a76971d24924a` | R04 + R06 base |
| `feat/wrath-item-payload` | `f09f024ed71f7ffca6b1deea93249ecab6c7d29b` | Worker A, S3 item instance payload |
| `feat/wrath-inventory-readers` | `b1699b02b298d11648edee5a8b88e31bcfdc7049` | Worker B, read-only inventory consumers |
| `integration/release-p0-item-readers-2026-11-05` | see remote HEAD | A + B merged, coordinator review/cleanup |

Both feature branches are based on `39e3c62a` (not stacked on each other). The integration branch
merges A (`077f850e`) then B (`708abb32`), followed by coordinator-only commits `46a184a6`
(review repair) and `d116a9fd` (ledger wording).

## Worker A — S3 lossless item instance payload

- `WrathEntity.item_fields` is now a presence-aware `WrathFieldWindow` over
  `ITEM_END`/`CONTAINER_END`; never delivered = `None`, delivered zero = `Some(0)`, sparse
  omission retains the last delivered value. The dense `Vec<u32>` lost both presence and the
  fields the donor projection drops.
- `push_item_fields` projects only delivered donor fields: donor 22..42 and 58..61 -> 43,44,46,47;
  donor 45 stays unwritten; Wrath 43..57 (enchant slots 8..12) and 62 (`CREATE_PLAYED_TIME`) are
  never projected into donor slots.
- New `crates/benilla-app/src/wrath_item.rs`: `WrathItemRow` component on the existing
  item/container entity with 12 enchant slots (id/sign, duration, charges), property seed,
  random-properties id, durability/max, create-played-time, stack/duration/five spell
  charges/flags, owner/contained/creator/gift creator, container num slots and 36 slot pairs.
  `ItemRead` provides donor fallback per never-delivered field only.
- Migrated consumers: tooltip/bind/link/charges/durability/random-property lines
  (`ui_items/feed.rs`, `ui_items/mod.rs`), bind confirm, spell item gate, merchant
  buyback/repair, paper-doll/worn enchants (`ui_char.rs`), held-item visual enchant fold
  (`entities/equipment/resolve.rs`), `ITEMS` movement watch (`items.rs`), and the item-watch
  notice for typed enchant slots 8..12 (`wrath.rs`).
- Item text by guid: `ui_item_text::WrathItemTexts` reads `MessageReader<WrathItemText>` on the
  native message edge, keyed by item guid with `Text`/`NoText` and an in-flight FIFO; scope
  changes retire entries and pending addresses. `ReadSource::ItemText` shows the body or closes
  on the absent reply. The Wrath bag/letter fork sends `ClientCommand::ItemTextByGuid` and uses
  the `0x200` readable bit; 1.12 keeps its `ITEM_FIELD_ITEM_TEXT_ID` letter path.
- Unsupported branches named by A: sockets/gems beyond enchant slots 2..4, RandomSuffix factor,
  scaling and item-limit columns are not carried by the implemented row; Wrath countdown lanes
  exist for enchant slots 2..4 only (no 8..12 lane); `ITEM_FIELD_CREATE_PLAYED_TIME` has no
  consumer; a lost item-text reply has no timeout re-ask. `WrathItemTexts` treats the absent
  `has_text=false` reply as retiring the oldest in-flight guid because the packet carries no
  guid; that FIFO assumption is runtime-unverified.

## Worker B — read-only inventory consumers

- `ui_items` now has one read path: `walk_inventory_in_player(&PlayerInventory, ...)` plus
  `count_of_in_player`, `sum_in_player`, `action_count_in_player`, `stack_counts_in_player`,
  `find_item_in_player`, `collect_inventory_in_player`; `possession_count` takes the view.
  Existing `&ObjectFields` entry points remain donor wrappers, so a 1.12 session or absent owner
  keeps 24/6/16 exactly.
- `PlayerInventory::typed` now reads `WrathInventory::presence`: an observed `Some(0)` is an
  observed clear and does not fall back to the donor projection; only a never-observed address
  falls back. Donor accessors are bounds-checked, so an absent bank slot 25 / bank bag 6 cannot
  read an occupied neighbour.
- Migrated read-only callers: action-bar state/identity/recount, item lane, quest log counts,
  tradeskill, craft, auction bid count, mail stationery, spell-tooltip reagents, shapeshift,
  lock openers, `GetItemCount`/`possession_count`, death self-res.
- Intentionally donor-only with named reasons: pre-send cast gates, command producers
  (target/click, target/lock, ui_action/drain, petition TurnIn), disarmed slot, cursor sale/drop
  and capture probes; command positions were not changed.
- `find_item`/`action_count`/`stack_counts` remain bank-excluded because the reference's mask-0
  walk does not recurse into bank or bank bags; bank-scoped `count_of`, `collect_inventory` and
  `possession_count(bank=true)` see bank slots 25..28 and bank bag 7; all mask-0 walks see keyring
  17..32. This is reference behavior, not a regression.

## Coordinator review and repairs

- Merge conflict in `docs/architecture/architecture-traceability.md` resolved by keeping A's
  INV-13..18 rows and B's INV-19/20 rows; no source conflict occurred because the two packages
  edited disjoint functions.
- `46a184a6`: `ItemRead::creator`/`gift_creator` now treat a delivered row zero as an observed
  clear instead of falling back to a stale donor creator, with a focused unrun case. The
  item-watch trigger now also fires on `!row_diffs.is_empty()`, so a typed-only enchant change
  (Wrath slots 8..12) reaches the synthetic notice path instead of waiting for a donor field
  diff.
- `d116a9fd`: corrected the RFC's "single in-flight guid queue" wording to the actual FIFO (at
  most one request per guid, absent reply retires the oldest) and re-anchored the
  `CREATE_PLAYED_TIME` ledger row at "stored only, no consumer" wording.

## Checks actually performed

- `git status --short --branch` clean on the integration branch; both feature worktrees clean
  apart from Worker B's untracked coordination notes (left unstaged).
- `git diff --check` clean from `39e3c62a` to HEAD and on the working tree.
- Feature branch remote SHAs equal their local heads; the integration branch is pushed and its
  remote SHA equals local HEAD.
- Explicit-path staging only; no reset/clean/stash/rebase/force-push/branch switch.
- No Cargo build/check/test/clippy/fmt, type check, client launch, server/DB access or live
  session. **Compilation, focused cases, receiver behavior and visible gameplay remain UNRUN.**

## Remaining acceptance

- Item payload: owner-driven character with an item carrying a Wrath-only enchant in slot 8+ and
  a readable item; verify tooltip/bind/link, item glow, bag read (text present and
  `has_text=false`), reconnect/transfer retirement and no stale donor enchant after a clear.
- Read-only consumers: same character as the prior bank acceptance (item in bank slot 25, bag in
  bank bag slot 7, seven purchased bank-bag slots); verify action-bar counts, quest log,
  tradeskill/craft, auction bid, tooltip reagents and `GetItemCount` see the bank-scoped item
  where the reference does; confirm `find_item` remains bank-excluded.
- Compilation and focused-test execution are still required before either package can claim more
  than source-level repair.
- Main was not merged: repository rules route publication through focused branches and pull
  requests, and no explicit authorization for a `main` merge was found in the inspected rules or
  handoffs. The integration branch is ready for review/PR.

## Next dependency-ready packages

1. **Raw player window**: extend `0..1171` to `PLAYER_END` (1326) for max level, daily quests and
   combat ratings, with consumer evidence.
2. **Item payload remainder**: random-suffix factor/scaling and socket/gem rows where the client
   protocol actually carries them; Wrath countdown lanes for enchant slots 8..12; a timeout
   re-ask for item text.
3. **Session delivery gate**: run the pending stall/disconnect acceptance from
   `2026-11-05-p0-inventory-session-integration.md` and add receiver-level checks.
4. **Update-ownership extraction** and live revision work from the roadmap once the above land.
