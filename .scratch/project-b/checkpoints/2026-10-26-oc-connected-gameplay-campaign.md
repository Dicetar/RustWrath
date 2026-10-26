# OC — connected gameplay campaign (2026-10-26)

Base: `cc65287b` on `wotlk`. This claim covers the client/protocol/gameplay lane requested in the
six-hour campaign. Astra owns UI-host/API compatibility, installed-addon compatibility and
combat-mode presentation; procgen files stay out of scope.

## Claimed paths

- `crates/benilla-app/src/wrath.rs` — Wrath feed, tap, view and dispatch integration.
- `crates/benilla-protocol-wrath/src/**` — opcodes, wire encoders, server-event parsing.
- `crates/benilla-app/src/net.rs` — only if a shared `ClientCommand` addition is unavoidable
  (prefer existing commands).
- `crates/benilla-app/src/inventory/**`, `crates/benilla-app/src/ui_merchant/**` — read-only for
  owner interfaces; vendor wire work stays in the Wrath crate and tap unless a typed handoff is
  required.
- `.scratch/project-b/checkpoints/2026-10-26-oc-*` — this record and slice records.

Not claimed: `crates/benilla-procgen/**`, `docs/procgen/**`, `.scratch/project-b/compatibility/**`,
`docs/architecture/**`, `tools/patch_procgen_dungeon.py` (untracked, preserved, never run).

## Campaign order

1. Finish the in-flight WM seam defects (committed `cc65287b`).
2. Vendor authority path: the stock 1.12 UI already owns `ListInventory`, `BuyItem`, `SellItem`,
   `BuybackItem` and consumes `VendorInventory`/`VendorBuyResult`/`VendorBuyFailed`/
   `VendorSellFailed`; 12340 wire translation was absent. Add 12340 encoders/parsers, feed the
   open vendor rows into the Wrath view, and map the existing commands through the tap.
3. Toggleable combat authority boundary: server-owner audit + typed client capability contract
   and Astra UI handoff. No fabricated stamina/dodge/i-frames and no experimental traffic in
   baseline mode.
4. Continue the next demonstrated gameplay/world gap after each commit.

## Slice: vendor authority path (committed with this record)

The stock 1.12 merchant owner already emits ListInventory, BuyItem, BuyItemInSlot,
SellItem, BuybackItem and RepairItem and consumes VendorInventory, VendorBuyResult,
VendorBuyFailed and VendorSellFailed; the Wrath tap dropped all six commands and decoded no
vendor reply. This slice closes the wire half:

- CMSG_LIST_INVENTORY 0x19E, SMSG_LIST_INVENTORY 0x19F, CMSG_SELL_ITEM 0x1A0,
  SMSG_SELL_ITEM 0x1A1, CMSG_BUY_ITEM 0x1A2, CMSG_BUY_ITEM_IN_SLOT 0x1A3,
  SMSG_BUY_ITEM 0x1A4, SMSG_BUY_FAILED 0x1A5, CMSG_BUYBACK_ITEM 0x290,
  CMSG_REPAIR_ITEM 0x2A8 (12340 Opcodes.h).
- 12340 encoders follow ItemPackets.cpp:71-106 (BuyItem is guid, entry, 1-based row, u32
  count, u8 1; SellItem count is u32; repair adds the guild-bank byte).
- 12340 list rows are eight words (ItemHandler.cpp:941-948), including the extended cost that
  the shared 1.12 VendorItem cannot carry. The Wrath view keeps the full rows; the shared event
  keeps the seven fields the merchant owner consumes.
- SMSG_BUY_FAILED / SMSG_SELL_ITEM write an optional u32 word before the reason when non-zero
  (PlayerStorage.cpp:4156-4177); the parser reads it by remaining length instead of desyncing.
- Buy commands carry only the template entry, while 12340 addresses the row; the tap resolves the
  1-based row from the open list fed by the session (WrathFeedEvent::VendorInventory). A buy
  with no matching open row is dropped visibly rather than encoded against a wrong row.
- Vendor state is cleared at world transfer, session end and a new epoch.

Correction: the stock buyback command names the absolute 1.12 slot 69-80, while 12340 starts
buyback at 74; the tap now rebases the 12 slots (69..=80 -> 74..=85) instead of sending the
1.12 index. The same index table differs for bank bags (1.12 63-68, 12340 67-73) and keyring
(1.12 81-112, 12340 86-117); bank commands below use the bank-bag rebase.

Gaps (explicit): the 1.12 merchant presenter ignores the 12340 extended cost (stored, not shown),
failure params are logged only, and a duplicate template entry resolves to its first open row.
Live list/buy/sell/buyback/refusal remains execution-gated.

## Slice: chat and addon lane (committed with this record)

Chat was absent in both directions, so the stock chat bar and the installed addons had no wire
path under the Wrath session. Added CMSG_MESSAGECHAT 0x095, SMSG_MESSAGECHAT 0x096 and
SMSG_GM_MESSAGECHAT 0x3B3, the uniform 3.3.5 body from Chat.cpp:374-443, and the outbound
ChatHandler.cpp:59 layout. Outbound kinds map to the 3.3.5 ChatMsg values; the language is
the frame's explicit choice or the race's native tongue (Common 7 / Orcish 1; universal 0 only
for AFK/DND). SendAddonMessage uses LANG_ADDON with the distribution type. Inbound types map
back to the 1.12 owner's wire values; Wrath-only achievement/monster-party/skill/loot/money/
battlenet lines are dropped explicitly rather than mislabelled. Achievement ids are consumed and
not presented (the 1.12 host has no achievement UI). Live send/receive remains execution-gated.

## Slice: bank authority path (committed with this record)

Bank open, slot purchase and item deposit/withdraw were absent from the Wrath tap. Added the six
12340 opcodes (CMSG_BANKER_ACTIVATE 0x1B7, SMSG_SHOW_BANK 0x1B8, CMSG_BUY_BANK_SLOT 0x1B9,
SMSG_BUY_BANK_SLOT_RESULT 0x1BA, CMSG_AUTOSTORE_BANK_ITEM 0x282,
CMSG_AUTOBANK_ITEM 0x283) and the BankPackets.cpp layouts. Withdraw commands rebase the six
stock bank bags (63-68) to 12340's 67-72; bank slots 39-62 and inventory/backpack indices agree.
12340 sends ERR_BANKSLOT_OK (3) on success, while the stock handler treats every result as a
refusal, so OK is filtered and the raised PLAYER_BYTES_2 count is the success signal.
Explicit gap: the 1.12 bank presents 24 of 28 slots and six of seven bank bags; the extra
positions remain unpresented, not silently remapped.

## Slice: cancel verbs and WM cast-cancel resolution (committed with this record)

ClientCommand::CancelCast, CancelChannelling, CancelAura and CancelAutoRepeat were still
dropped by the Wrath tap, including the command the WM cast_cancel action maps onto. Added the
four 12340 opcodes and encoders (CMSG_CANCEL_CAST 0x12F with its ignored counter byte,
CMSG_CANCEL_AURA 0x136, CMSG_CANCEL_CHANNELLING 0x13B,
CMSG_CANCEL_AUTO_REPEAT_SPELL 0x26D). A pending WM cast now records the spell it commits to, so
a self SMSG_SPELL_FAILED_OTHER resolves the matching cast as failed and the matching cancel as
succeeded; a resolved cast clears the remembered spell so a later cancel cannot name a stale one.

## Slice: trainer authority path (committed with this record)

Trainer list and purchase were also absent from the Wrath tap. Added the five 12340 opcodes
(CMSG_TRAINER_LIST 0x1B0, SMSG_TRAINER_LIST 0x1B1, CMSG_TRAINER_BUY_SPELL 0x1B2,
SMSG_TRAINER_BUY_SUCCEEDED 0x1B3, SMSG_TRAINER_BUY_FAILED 0x1B4), the 38-byte row layout
from NPCPackets.cpp:25-41, and the two request encoders from NPCPackets.cpp:48-52. The
12340 row collapses the 1.12 boolean pair into PointCost[1] (primary-profession first rank);
both 1.12 booleans derive from that word. State values (0 available, 1 unavailable, 2 known)
match the 1.12 owner, so they pass through. Refresh and purchase now reach the existing trainer
UI. Live purchase/refusal remains execution-gated.

## Slice: toggleable combat boundary and UI contract (committed with this record)

`crates/benilla-app/src/combat_mode.rs` owns the only new client state and it is a read model, not
an authority:

| Field | Meaning | Source / lifecycle |
|---|---|---|
| `requested` | last user mode request | request API only; `None` before any request |
| `effective` | authoritative accepted mode | always `Baseline` today; only a server mode message may change it |
| `availability` | whether a server mode channel exists | `Unavailable(ServerModeChannelMissing)` from the installed server/mod audit |
| `last_refusal` | why the last experimental request was not sent | request API |
| `resources` | health/max/power type/power | existing `WrathWorldView.self_observation.resources`; each field independently `None` until delivered |
| `committed_action` | cast/channel currently owned by the cast owner | existing `PendingCast` / `ActiveChannel` |
| `cooldown` | ordinary recovery of the committed spell | existing `Cooldowns::info`; not an experimental stamina/i-frame window |
| `revision` | observed-state change counter | projection |

`request_experimental` returns `NotSent { ServerModeChannelMissing }` and emits no packet;
`request_baseline` returns `AlreadyEffective` and sends nothing. Cancellation is observed as
`committed_action` becoming `None`; the existing cast owner publishes no separate reason.
Experimental stamina, dodge/i-frames, commitment windows and telegraphs remain absent because the
server owner is absent. Astra's UI should render baseline availability and the refusal reason
instead of a working toggle.

## Combat authority audit (source evidence)

The installed mod set has no dodge/stamina/mode primitive. `mod-wm-bridge` emits only
`combat.kill`, quest, loot, gossip and area rows (`modules/mod-wm-bridge/README.md`,
`src/wm_bridge_player_script.cpp`), and `challenge-modes` is a ruleset modifier, not a hit-outcome
owner. The old offline Rust combat source is a reference model only
(`PROJECT_B_COMPLETION_PLAN.md` §E.1), not deployed authority.

Missing server owners, with the exact hook points that must change:

- Mode request/accept/refusal: custom `CMSG_*`/`SMSG_*` pair; current 12340 opcode table
  (`src/server/game/Server/Protocol/Opcodes.h`) has no combat-mode entry and AC modules cannot
  register `CMSG` opcodes without a core edit.
- Authoritative stamina: no custom player resource/field; a server-owned state packet or a
  deliberately leased stock power type would be required. None exists in the installed modules.
- Dodge eligibility at hit construction: melee damage is resolved in `Unit::MeleeDamage` /
  `Unit::DealMeleeDamage` (`src/server/game/Entities/Unit/Unit.cpp`), direct spells in
  `Spell::DoSpellHitOnUnit` (`SpellEffects.cpp`); no script hook exposes "replace this outcome
  with dodge before packets/procs/logging". A core/module patch is required.
- Movement commitment/roll displacement: server movement/root ownership is in
  `MovementHandler.cpp` / `Unit::SetRooted`; no mode-scoped commitment window exists.
- Telegraph: existing `SMSG_SPELL_START`/visual packets can only telegraph a real server spell;
  no custom telegraph/owner is installed.

Client consequence: the only honest client state today is baseline mode with experimental
availability = false and a typed refusal reason. The client must not send experimental traffic or
invent resources/outcomes. The UI contract is `CombatModeView` (requested/effective/refusal,
authoritative resource snapshots, committed action, recovery, cancellation) fed from existing
owners; see the combat-mode slice record once committed.

## Campaign milestones (this checkout, explicit paths)

Base `cc65287b` through `00043f25`:

- `cc65287b` restores the missing v2.2 observation types and bounds the native action queues.
- `d6298139` carries the 12340 vendor path; `5d87f253` rebases the 1.12 buyback slot.
- `9b8cad61` adds the baseline-only combat boundary and the UI read model.
- `c1c7517e` carries the trainer list/purchase path.
- `f4ccb1c4` carries the cancel verbs and resolves WM cast cancellation.
- `db7507de` carries the bank open/purchase/move path.
- `ee1656b9` carries chat and the addon lane; `452c9b67` adds chat notices.
- `0575dee0` restores vendor-row documentation.
- `00043f25` clears gossip/vendor state on a new session or world scope.

## Acceptance scenarios prepared (all execution-gated)

- Baseline combat: ordinary attack/cast/interrupt/cooldown/aura under Wrath; `CombatModeView`
  reports `Baseline`, unavailable with `ServerModeChannelMissing`; no experimental packet.
- Experimental refusal: `request_experimental` returns `NotSent`; effective stays `Baseline`;
  revision increments; no traffic.
- Transfer: entities, gossip, vendor rows and the self observation clear; committed action and
  cooldown clear through their existing owners.
- Death: health/power and death state come from the replicated self; the cast owner clears the
  commitment; no fabricated resurrection or mode change.
- Reconnect: session end/epoch change clears observed combat state, vendor/gossip scope and WM
  pending actions (`Unknown`); the replacement session starts in baseline.
- Vendor/trainer/bank/chat: normal, refusal and scope-loss paths described in their slices.
- WM action: cast/cancel/attack/etc. reach the stock command owners; results come only from the
  matching authoritative reply or a bounded Unknown.

## Pending acceptance (execution-gated)

- Vendor: live list, buy, sell, buyback and refusal against the pinned server.
- Combat: server hook + zone/mod deployment decision; then dodge/refusal/transfer/death/reconnect.
- WM seam: receiver/WM run and action round-trip (source-only until granted).
