# B-001B-1 — Live-rule source contract (Spellwright v3 A/B)

2026-10-24. Status: **PARTIAL — contract complete; graphs authored and statically checked against source; server-side Parse/Validate/Purchase execution pending (ticket 10)**. Every claim cites `SpellmakingV3.cpp` (S) or `SpellmakingV3Graph.cpp` (G) in `D:/WOW/wm-project/native_modules/mod-multiclass/src/`.

## 1. Scenario

- **A = "Sentinels Vigil"** (`graph-A.json`): self-cast Ward (absorb package, 30 s) whose *repeat timer reaction* (event 1, every 6 s) invokes a self-delivery applying **Mending** — an instant heal package (effect 7).
- **B = "Sentinels Alacrity"** (`graph-B.json`): identical Ward and timer; **the relation changes** — the every-6 s reaction now invokes **Swiftness**, a movement-speed package (effect 18, 30 %, 6 s maintained) through a different delivery/package chain. Not a value edit: different operation family, different child package.
- Noncombat-safe by construction (no hostile targets, no offensive ops), so concurrent authoring does not collide with `AtMaker`'s out-of-combat requirement.

## 2. Wire format (source-observed, G:15-26)

`{version:3, name, root, icon, nodes:[{id,name,kind,p:{...string props...},children:[ids]}]}`. Parser reads **only** `id/name/kind/p/children` — `next` and `package` links live **inside `p`**. Constraints enforced by `Validate` (G:38-76): name 1-40 no `|\n\r\t`; icon must start `Interface\Icons\`; root must be kind `cast`; delivery.package → package/reference; package children → operation/reaction/reference; effects 17/29 forbidden; percent ops (13,14,18,20-24) magnitude ≤ 95; field deliveries need shape 3-7; duration-0 packages may hold only instant non-timed operations (effects <8 or 15/16/19/30) and no event-1 reactions; immediate-edge cycle rejection (timer/event-1-4/periodic edges exempt).

Static mirror of all applicable rules passes for both graphs (see session log; script inline in ticket history). **Runtime Parse/Validate confirmation = ticket 10 step 0.**

## 3. Authoring transport (the only publication path)

Entry chain: addon whisper `WM_FORGE3\t...` → `V3Players::OnPlayerCanUseChat` (S:392) → `Command(p,message)` (S:341) → dispatcher (S:344-353): `LIST | REVISION | BEGIN/FINISH (upload, purpose QUOTE|SAVE) | BUY | DELETE`. Chunked upload: ≤64 KB doc, 150-char DATA frames (S:114-115), 30 s upload expiry (`Now()+30000`), 120 s quote expiry (S:346-350). Purchase flow is **two-step by design**: FINISH(QUOTE) → `SendQuote` nonce → `BUY|nonce` (S:316 rejects stale/mismatched nonces).

## 4. Publication semantics (S:314-329) — definitive answers

- `Purchase` = **new immutable spell identity**: cast slot from `CastFirst + family*128` (families by mode/target, S:320), carriers from the dynamic pool per package and per operation (`CarrierFirst..CarrierLast`, S:322-323). Gated: valid fresh quote + **`AtMaker`** (maker NPC interactable, player alive, out of combat — S:125-126,317) + **`Available`** (all operation effects must be known ingredients — S:123-124) + fee money.
- On DB-transaction success the callback runs **`Configure(d)` immediately** (S:329 → S:168-191): rewires live `SpellInfo` (cast time/range/cooldown, carrier auras), inserts into the in-process `compiled` map (S:190), teaches the spell (`learnSpell`), emits `CREATED|id`.
- **Therefore: definitions purchased through the addon flow are live with no restart.** External DB writes to `wm_spellforge_design` are loaded **only** at `OnStartup` (S:397-399) — **no runtime reload verb exists**. "DB publication + reload trigger" is not an available route.

## 5. WM publication route — decision

- **(a) Server-side dispatcher entry: MISSING ADAPTER.** `Command` is file-static; nothing outside the translation unit can submit addon-protocol payloads. Prerequisite implementation (small, owner-approved when scheduled): an exported `SpellmakingV3::SubmitCommand(Player*, std::string)` shim + a `mod-wm-bridge` verb wrapping it (reusing the bridge's typed request/policy machinery). Constraints the adapter inherits: player must be **online** (State, money, AtMaker) and the **two-step quote** must be honored or replicated via `Price` before purchase.
- **(b) DB + reload trigger: rejected** — no reload path (§4).
- **(c) Bot-client whisper: rejected** — requires a connected game client masquerading as the author.

## 6. Selection tiers

- **(i) Actor-casts-B (zero new code):** after BUY, B is a new spell in the actor's book; "the selected logical action" = whichever of A/B the actor executes. Rollback = `DELETE` B (S:352: retires the ID, ends its running instances via `End(...,13)`, unlearns).
- **(ii) WM-owned logical binding: MISSING IMPLEMENTATION.** Proposal for the prerequisite ticket: a WM-control-owned mapping `logical_action_id -> purchased spell_id` consulted by the existing grant/refresh path; rollback = repoint to A. Until it exists, evidence points 3-4 below cannot be claimed for WM automation.

## 7. Four-point evidence plan (ticket 10 executes)

1. **Library B saved while A purchased:** SAVE upload of B's modules (or direct FINISH) with A already purchased — exercises `SaveModule` (S:331-336) revision insert only.
2. **Newly purchased B executes while A runs:** BUY B; cast A (Ward + Mending pulses observed); cast B (Ward + Swiftness pulses); A's running instance must complete on A's graph (instances pin `design` shared_ptr — S:57-59,250-251).
3. **Selected logical action chooses B; rollback selects A:** tier (i) = cast B for the action, then DELETE B and cast A. Tier (ii) requires §6-ii implementation — record which tier the proof ran.
4. **WM performs publication/selection through its typed path:** blocked on §5-a/§6-ii adapters. If not yet implemented, ticket 10 records them as gate dependencies (never as silent downgrade).

No-reboot evidence: server PID + binary hash unchanged across the whole sequence; invalid-B rejection (malformed FINISH) leaves A castable; no duplicated pulses (per-reaction `lastReaction` cooldown — S:213-214).

## 8. Lifecycle (restated, source-pinned)

Instances end at logout (S:390 `End(...,13)` + job queue cleared); death/map-leave/dispel paths end instances with distinct reasons (S:299-300, S:380); `OnStartup` reloads definitions/bindings only (S:397-399) — **active effects are never rehydrated**. `DELETE` is the in-life rollback lane (S:352).

## 9. Preconditions & open items for ticket 10

- [ ] Lab character must know ingredients for effects **12, 7** (A) and **12, 7, 18** (B) via `SpellmakingAdvanced::IngredientKnown` — verify before the trial or pick known effects (S:123-124). [TV]
- [ ] Naram (maker NPC) present and interactable in the lab; player alive, out of combat at BUY time.
- [ ] Fee money on the lab character; free cast slot in family `mode=0,target=0` and ≥3 carriers (S:320-323).
- [ ] Malformed-B fixture for the invalid-B rejection step (e.g. percent magnitude >95 → G:65).
