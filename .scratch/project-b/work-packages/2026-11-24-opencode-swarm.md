# Swarm result 2026-11-24

Owner: the OpenCode DeepSeek overseer. This is the only report for the session; per-ticket
source packets, reference addresses and checks are in the branch handoffs. No local Cargo,
client or server was run and no `target` directory was created. `rustfmt` was run directly on
the edited files; CI was the build authority.

## Main baseline

- Start: `b520b19aa`. The first fully green `player-clippy` run is **44840435291**
  (`f82c180f`); the latest green main run at admission was **44908089612** (`b520b19aa`).
  Task 2 of the last brief is closed.
- Main advanced several times while this session ran; each ticket was integrated by merge
  (never rebased by rewrite) and re-run green on the merge tip before `git push HEAD:main`.
- Landed tonight: periodic feed, cast feed, spell-execute feed and the dispel family. Every
  landed ticket has its own handoff note.

## Task 1

1. **SPELL_PERIODIC_* landed** (`71f04b134`). `SMSG_PERIODICAURALOG` (`0x24E`) now keeps
   overkill, school, absorbed, resisted, crit and hitInfo; the feed posts
   `SPELL_PERIODIC_DAMAGE`, `_MISSED`, `_HEAL`, `_ENERGIZE`, `_DRAIN` and `_LEECH`
   with the reference's 0x10/0x2000/0x4000/0x20/0x40/0x80 blocks. Green 44941803899 on the
   final merge tip.
2. **SPELL_AURA_APPLIED/REMOVED are client-side.** 12340 builds them from aura updates:
   `CGUnit_C::OnAuraAdded`/`OnAuraRemoved` (`Unit_C.cpp:16539/16529`) call
   `UnitCombatLogAuraAddedOrRemoved` (`UnitCombatLog_C.cpp:1851`, event 21/24); dose and
   refresh are `UnitCombatLogAuraDose` (`:2313`, events 22/23) and
   `UnitCombatLogAuraRefreshed` (`:2351`, event 25). There is no packet. The producer is the
   aura diff in `benilla-protocol-wrath::reducer` surfaced as
   `WrathEntity::auras`/`aura_receipts`; `ui_script/wrath_aura_feed.rs` only raises
   `UNIT_AURA`. No agent owned that diff tonight: **Blocked, owner unassigned**.
3. **Cast feed landed** (`3b6633f10`). `SPELL_CAST_START`/`SUCCESS`/`FAILED` reuse the
   existing `SpellStart`/`SpellGo`/`CastFailed` rows and post 11/11/12 values; GO names its
   packet target, START/FAILED have a nil destination, and FAILED reads the GlobalStrings text
   by `CAST_FAIL_KEYS`. Green 44954637681. Deviation: `%s` argument formatting inside the
   failure text is not filled (raw GlobalString).

## Task 2

### Final set, confirmed before ticketing

Produced by the reference and sent by AzerothCore:
- `SPELL_INTERRUPT`: `SMSG_SPELLLOGEXECUTE` `0x24C` effect 68.
- `SPELL_EXTRA_ATTACKS`: `0x24C` effect 19.
- `SPELL_SUMMON` / `SPELL_CREATE`: `0x24C` effects 28/50/56/76/81/83/104-107.
- `SPELL_RESURRECT`: `0x24C` effects 18/113.
- `SPELL_DRAIN` / `SPELL_LEECH`: `0x24C` effects 8/62.
- `SPELL_DURABILITY_DAMAGE` / `_ALL`: `0x24C` effect 111.
- `SPELL_DISPEL` / `SPELL_STOLEN`: `0x27B` / `0x333`.
- `SPELL_DISPEL_FAILED`: `0x262`.
- `SPELL_INSTAKILL`: `0x32F`.
- `DAMAGE_SHIELD`: `0x24F`.
- `DAMAGE_SPLIT`: `0x250` hitInfo 0x8.
- `RANGE_DAMAGE`: `0x250` rows whose spell has `AttributesExC & 0x8000`.
- `ENVIRONMENTAL_DAMAGE`: `0x1FC`.
- `ENCHANT_APPLIED` / `ENCHANT_REMOVED`: `0x1D7`.

Dropped:
- `DAMAGE_AURA_BROKEN` (`s_combatLogEventNames` 30): no producer path.
- `SPELL_AURA_BROKEN` / `_SPELL`: only `0x14F`, which AzerothCore never sends.
- `SPELL_DURABILITY_DAMAGE` from `SMSG_DURABILITY_DAMAGE_DEATH` (`0x2BD`): that packet is
  chat-only; the subevent comes from the execute log effect 111 above.

Client-side, not packets:
- `SPELL_AURA_APPLIED`/`_APPLIED_DOSE`/`_REMOVED_DOSE`/`_REMOVED`/`_REFRESH`.
- `UNIT_DIED` / `UNIT_DESTROYED` / `UNIT_DISSIPATES`: `UnitCombatLogUnitDead`
  (`UnitCombatLog_C.cpp:2763`, called from `UnitCombat_C.cpp:238`). The app has no unit death
  edge in `WrathEntity`; **Blocked, owner unassigned**.
- `RANGE_MISSED`: client-side missile/spell results, no decoded packet producer.
- `DAMAGE_SHIELD_MISSED`: only `UnitCombatLogSpellMissed(..., a5=true)` from `0x24B`, which
  the landed `SPELL_MISSED` slice already consumes as event 8 per the owner's brief. No second
  packet; not separately ticketed.

### Subevent table

| Subevent | Producer | Landed / status |
|---|---|---|
| SWING_DAMAGE / SWING_MISSED | SMSG_ATTACKERSTATEUPDATE 0x14A | landed before this session |
| SPELL_DAMAGE | SMSG_SPELLNONMELEEDAMAGELOG 0x250 | landed before this session |
| SPELL_MISSED | SMSG_SPELLLOG_MISS 0x24B | landed before this session |
| SPELL_HEAL | SMSG_SPELL_HEAL_LOG 0x150 | landed before this session |
| SPELL_ENERGIZE | SMSG_SPELL_ENERGIZE_LOG 0x151 | landed before this session |
| PARTY_KILL | SMSG_PARTYKILLLOG | landed before this session |
| SPELL_PERIODIC_DAMAGE / MISSED / HEAL / ENERGIZE / DRAIN / LEECH | SMSG_PERIODICAURALOG 0x24E | 71f04b134 |
| SPELL_CAST_START / SUCCESS / FAILED | SMSG_SPELL_START 0x131 / GO 0x132 / CAST_FAILED 0x130 | 3b6633f10 |
| SPELL_INTERRUPT / EXTRA_ATTACKS / SUMMON / CREATE / RESURRECT / DRAIN / LEECH / DURABILITY_DAMAGE / _ALL | SMSG_SPELL_EXECUTE_LOG 0x24C | 03a416504 |
| SPELL_DISPEL / SPELL_STOLEN / SPELL_DISPEL_FAILED | 0x27B / 0x333 / 0x262 | 8648b1521 |
| SPELL_INSTAKILL / ENCHANT_* / ENVIRONMENTAL_DAMAGE | 0x32F / 0x1D7 / 0x1FC | Blocked: branch `oc/combat-log-single-row` (c0bf5843d) does not compile; CI 44990691706 |
| DAMAGE_SHIELD / DAMAGE_SPLIT / RANGE_DAMAGE | 0x24F / 0x250 hitInfo 0x8 / 0x250 attr 0x8000 | Not started; reference facts confirmed above |
| SPELL_AURA_APPLIED / _DOSE / _REMOVED / _REFRESH | client-side aura diff | Blocked, owner unassigned |
| UNIT_DIED / DESTROYED / DISSIPATES | client-side unit death | Blocked, owner unassigned |
| DAMAGE_SHIELD_MISSED | 0x24B (already SPELL_MISSED) | no separate packet |
| RANGE_MISSED | client-side missile results | no decoded producer |
| SPELL_AURA_BROKEN / _SPELL | 0x14F | AzerothCore never sends |
| DAMAGE_AURA_BROKEN | none | reference produces no row |

## Tickets, models and CI

- `oc/spell-periodic` — **Iceman Free**; landed `71f04b134`. Review fixes `c49dd9ccb`
  (fmt, integer miss amount, nil divided drain extra, hitInfo crit) and `7445af993` (memo
  borrow split, `PeriodicAura` destructure). CI 44941803899.
- `oc/spell-cast-log` — **LongCat 2.5 Free** wrote the row structs/queues, then hit
  `provider.free-limit`; **Hy4 Flash Free** finished the feed and tests. Landed `3b6633f10`.
  Review fixes `1eff3eac8` (expose `CAST_FAIL_KEYS`, `Option` target, unused `mut`) and
  `3b6633f10` (clippy `doc_lazy_continuation`). CI 44954637681.
- `oc/spell-execute-log` — **Iceman Free**; landed `03a416504`. Review fixes `952c7d67b`
  (12340 gameobject guid test, not the 1.12 helper), `aa825905f` (unused test `mut`), and the
  merge reruns `3037ddaea`/`92a30689b`/`03a416504`. CI 44972609737.
- `oc/spell-dispel-family` — **Tidepool Alpha Free** looped on a path spelling and made no
  edits; cancelled. **Beam Free** wrote the decode/queues/feed. Landed `8648b1521`. Review fixes
  `ded3f3204` (imports, duplicate event variant), `dc7dd41fd` (3.3.5 gameobject flag, with a
  test), and `d53122407`/`8648b1521` (one queue, dead aliases/params removed). CI 44985009468.
- `oc/combat-log-single-row` — **Flint 40B Free** hit `provider.free-limit` before writing
  source; **Nemotron 3.5 Lightning Free** wrote a partial decode/event layer but the branch does
  not compile (missing world queues/counters, `EnchantRemoved`, and match arms). Parked with
  `Status: blocked` and a concrete next step; branch `c0bf5843d`.
- Damage classifications — not started. The confirmed facts are in this report; the next owner
  needs the `0x24F` decode plus the `0x250` split/range selection.

Counts: eight free-model attempts across five tickets; zero paid DeepSeek worker fallbacks.
Reassignments: LongCat → Hy4 on a free limit, Tidepool → Beam on a no-edit failure, Flint →
Nemotron on a free limit. The overseer's DeepSeek work was review/fix and integration only.

## Not proven

- No live client/server run; no row was observed in the stock combat-log frame.
- The periodic per-type queues preserve each family's order but not a mixed-type packet's
  cross-type order; AzerothCore writes one tick per packet.
- `SPELL_CAST_FAILED` posts the raw GlobalString, so a `%s` argument remains a literal.
- The dispel family was verified against the reference and CI; no live dispel row was observed.
- The blocked single-row and unstarted damage-classification rows have no replayable CI proof.
