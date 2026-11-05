# 2026-11-05 — P0 inventory + session-delivery integration

Batch: two AutoSwarm OpenCode workers plus coordinator review. Owner-selected model after the
free-provider quota block: `opencode-go/deepseek-v4.1-flash`.

## Branches and SHAs

| Ref | SHA | Scope |
|---|---|---|
| `origin/main` base | `46fd2f0c2af476b3659116f0fc8c95900bde95a0` | untouched |
| `feat/bounded-session-delivery` | `47cba73865a450218b99889cc61e392d7d4ba13c` | Worker B, R04 bounded session replies |
| `feat/wrath-inventory-lossless` | `f2c7a75a7b69b964a7019f5f194193dc3ebc6614` | Worker A, R06 lossless bank/bag state |
| `integration/release-p0-inventory-session-2026-11-05` | see remote HEAD | B + A merged, coordinator review/cleanup |

A is stacked on B (`f2c7a75a` contains `47cba738`). The integration branch merges B first
(`67f3b78c`) then A (`dbdc1ca1`), followed by coordinator-only cleanup commits.

## What was reviewed and corrected

- B: bounded, non-blocking `SessionReplies`; reserved terminal slots; observation coalescing;
  cancel-aware read backpressure; bounded drain; transfer prune; closed-queue teardown;
  `quest_trace.rs` and hotpatch crates untouched. Coordinator requested the post-pause cancel
  check and corrected implementation provenance in B's handoff.
- A: typed `WrathInventory` with presence windows and explicit clears; donor projection remains
  24 bank / 6 bank-bag / 7-enchant; 28/7/32 consumers migrated; bank-bag purchase cap 7; typed
  transfer retirement; item payload S3 deferred.
- Coordinator review returned A once for four source gaps, all repaired in `f2c7a75a`:
  target-aware `BankButtonIDToInvSlotID` and `ContainerIDToInventoryID`; bag-11 support in
  `GetContainerNumSlots`/`GetBagName`/withdraw/swap/world-enter; donor 1.12 range preservation;
  and target-aware key-at-object `find_item_in_target`.
- Coordinator corrected the deferred item-payload ledger rows so they no longer claim a typed
  item row that does not exist.

## Checks actually performed

- `git diff --check` clean on the integration branch.
- Feature branch and integration branch remote SHAs equal their local heads.
- Clean worktrees; explicit-path staging only; no reset/clean/rebase/force-push.
- No Cargo build/check/test/clippy/fmt, type check, client launch, server/DB access or live
  session. **Compilation, focused cases, receiver behavior and visible gameplay remain UNRUN.**

## Remaining acceptance

- Inventory: owner-driven character with an item in bank slot 25 and a bag in bank bag slot 7
  (seven purchased bank-bag slots); verify 28 bank rows, slot-25 withdraw, bag-11 name/capacity
  and withdraw, deposit into slot 25, reload and reconnect.
- Session delivery: owner-driven Northshire session with `WOW_STALL="1000,20,60,10"`, one quest
  and one loot, then a stall and a deliberate disconnect; record queue counters, terminal
  timing, relog/reconnect and visible result.
- Compilation and focused-test execution are still required before either package can claim
  more than source-level repair.
- Main was not merged: repository rules route publication through focused branches and pull
  requests, and no explicit authorization for a `main` merge was found in the inspected rules
  or handoffs. The integration branch is ready for review/PR.

## Next dependency-ready packages

1. **Inventory S3**: typed item payload (12 enchants, create-played-time, item text by guid,
   random suffix/socket/scaling) and tooltip/bind/visual migration.
2. **Read-only inventory walks**: thread `PlayerInventory` through `count_of`/`sum_of`/
   `action_count`/`stack_counts`/`collect_inventory`/`find_item` action bar, tradeskill, auction
   and tooltip consumers.
3. **Raw player window**: extend `0..1171` to `PLAYER_END` (1326) for max level, daily quests and
   combat ratings, with consumer evidence.
4. **Session delivery gate**: run the pending stall/disconnect acceptance and add receiver-level
   checks; then proceed to the roadmap's update-ownership extraction and live revision work.
